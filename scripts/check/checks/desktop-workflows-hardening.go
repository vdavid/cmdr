package checks

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// RunWorkflowsHardening enforces GitHub Actions invariants. The first four
// protect against the wave-4 (TanStack, May 2026) class of supply-chain attack:
//
//  1. Every third-party action `uses:` reference must be SHA-pinned (40 hex
//     chars), not tag- or branch-pinned. Tag refs are mutable; a malicious
//     re-tag would silently land. Local `./...` actions and reusable
//     workflows that resolve to the same repo are exempt.
//
//  2. No workflow may use the `pull_request_target` trigger. It runs in the
//     base repo's security context with access to the cache scope and
//     GITHUB_TOKEN — the exact entry vector that let attacker code poison
//     TanStack's pnpm store and steal the OIDC token.
//
//  3. `id-token: write` must be job-scoped, never workflow-scoped. A
//     workflow-level grant means every job in the workflow can mint OIDC
//     tokens; if any one of them runs attacker-controlled code, the token
//     can be exfiltrated. Job-scoping isolates the privilege to the single
//     publishing step.
//
//  4. Every workflow declares a workflow-level `permissions:` block that grants
//     no `write`; jobs that need write ask for it in their own block. A
//     missing block inherits the repo default, which a settings change can
//     widen to write-all without touching the repo. This is also exactly what
//     OpenSSF Scorecard's Token-Permissions check scores (`scorecard.yml`).
//
// All four classes are silent in normal review: tag pins look identical to
// SHA pins, `pull_request_target` looks like a typo of `pull_request`, and
// permissions blocks are usually skimmed. The check makes them loud.
//
//  5. A workflow that runs CodeQL analysis also triggers on `pull_request`, or
//     Scorecard's SAST check scores every merged (Renovate) PR as unchecked.
//
//  6. A `workflow_run` workflow gates every job on a push to `main` in this repo
//     (`workflowRunGuard`). It runs from the default branch with the repo's secrets
//     and a cache-write opt-in (`ci-upkeep.yml`), while the run it follows may be a
//     fork PR's, so an unguarded job could check out and run untrusted code.
func RunWorkflowsHardening(ctx *CheckContext) (CheckResult, error) {
	wfDir := filepath.Join(ctx.RootDir, ".github", "workflows")
	entries, err := os.ReadDir(wfDir)
	if err != nil {
		if os.IsNotExist(err) {
			return Skipped("no .github/workflows/"), nil
		}
		return CheckResult{}, fmt.Errorf("failed to read workflows dir: %w", err)
	}

	var files []string
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		if strings.HasSuffix(name, ".yml") || strings.HasSuffix(name, ".yaml") {
			files = append(files, filepath.Join(wfDir, name))
		}
	}
	sort.Strings(files)

	var violations []string
	scanned := 0
	for _, f := range files {
		v, err := scanWorkflowFile(f, ctx.RootDir)
		if err != nil {
			return CheckResult{}, err
		}
		violations = append(violations, v...)
		content, err := os.ReadFile(f)
		if err != nil {
			return CheckResult{}, fmt.Errorf("failed to read workflow: %w", err)
		}
		rel, _ := filepath.Rel(ctx.RootDir, f)
		lines := strings.Split(string(content), "\n")
		violations = append(violations, checkWorkflowPermissions(lines, rel)...)
		violations = append(violations, checkCodeQLRunsOnPullRequests(lines, rel)...)
		violations = append(violations, checkWorkflowRunGuard(string(content), rel)...)
		scanned++
	}

	if len(violations) > 0 {
		return CheckResult{}, fmt.Errorf("workflow hardening violations\n%s",
			indentOutput(strings.Join(violations, "\n")))
	}

	result := Success(fmt.Sprintf("%d %s, all hardened", scanned, Pluralize(scanned, "workflow", "workflows")))
	result.Total = scanned
	return result, nil
}

var (
	// uses: <owner>/<repo>@<ref>  (optionally followed by space + comment)
	// Captures: 1=owner/repo[/subpath], 2=ref
	usesRefRE = regexp.MustCompile(`^(\s*)(?:-\s+)?uses:\s+([^@\s]+)@([^\s#]+)`)

	// 40-char hex SHA — the only acceptable ref form for third-party actions.
	sha40RE = regexp.MustCompile(`^[a-f0-9]{40}$`)

	// `id-token: write` line. We care about its indentation level relative
	// to the containing job to tell workflow-scope from job-scope.
	idTokenWriteRE = regexp.MustCompile(`^(\s*)id-token:\s*write\s*$`)

	// Top-level `on:` block introducer.
	onIntroRE = regexp.MustCompile(`^on:\s*$`)
	// Inline `on:` form like `on: [push, pull_request_target]` or `on: pull_request_target`.
	onInlineRE = regexp.MustCompile(`^on:\s+(.+)$`)
	// A trigger key under `on:`, like `  pull_request_target:` (any indent).
	triggerKeyRE = regexp.MustCompile(`^(\s+)([a-z_]+):`)
)

func scanWorkflowFile(path, repoRoot string) ([]string, error) {
	rel, _ := filepath.Rel(repoRoot, path)
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("open %s: %w", rel, err)
	}
	defer f.Close()

	var violations []string
	tracker := onBlockTracker{onIndent: -1}
	lineNum := 0
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		lineNum++
		line := scanner.Text()
		trimmed := strings.TrimRight(line, " \t")
		if trimmed == "" || strings.HasPrefix(strings.TrimSpace(trimmed), "#") {
			continue
		}

		if v := checkUsesLine(trimmed, lineNum, rel); v != "" {
			violations = append(violations, v)
		}
		if v := tracker.update(line, trimmed, lineNum, rel); v != "" {
			violations = append(violations, v)
		}
		if v := checkIdTokenLine(line, lineNum, rel); v != "" {
			violations = append(violations, v)
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("read %s: %w", rel, err)
	}
	return violations, nil
}

// checkUsesLine flags tag/branch-pinned third-party action references.
func checkUsesLine(trimmed string, lineNum int, rel string) string {
	m := usesRefRE.FindStringSubmatch(trimmed)
	if m == nil {
		return ""
	}
	repo, ref := m[2], m[3]
	if isExemptUsesRef(repo) || sha40RE.MatchString(ref) {
		return ""
	}
	return fmt.Sprintf("%s:%d: tag/branch-pinned action: %s@%s (SHA-pin: '@<40-hex> # %s')",
		rel, lineNum, repo, ref, ref)
}

// checkIdTokenLine flags workflow-scoped `id-token: write` permissions.
// Workflow-level `permissions:` is at column 0, so its children sit at
// 2-space indent. Job-level `permissions:` lives under `jobs.<name>.`,
// so its children sit at ≥6-space indent. Anything ≤4 is workflow-scoped.
func checkIdTokenLine(line string, lineNum int, rel string) string {
	m := idTokenWriteRE.FindStringSubmatch(line)
	if m == nil {
		return ""
	}
	if len(m[1]) > 4 {
		return ""
	}
	return fmt.Sprintf("%s:%d: workflow-scoped 'id-token: write' (must be job-scoped)", rel, lineNum)
}

var (
	// Workflow-level `permissions:`, either a block introducer or an inline
	// scalar/map (`permissions: read-all`, `permissions: {}`).
	topPermissionsRE = regexp.MustCompile(`^permissions:\s*([^#\s]*)`)
	// A grant inside the workflow-level block (2-space indent).
	topGrantRE = regexp.MustCompile(`^  ([a-z-]+):\s*([a-z-]+)`)
)

// checkWorkflowPermissions enforces invariant 4: a workflow-level
// `permissions:` exists and grants no write. `id-token: write` is skipped here
// because `checkIdTokenLine` already reports it with its own message.
func checkWorkflowPermissions(lines []string, rel string) []string {
	var violations []string
	found, inBlock := false, false
	for i, line := range lines {
		if m := topPermissionsRE.FindStringSubmatch(line); m != nil {
			found = true
			inBlock = m[1] == ""
			if m[1] == "write-all" {
				violations = append(violations,
					fmt.Sprintf("%s:%d: workflow-scoped 'permissions: write-all' (use read, grant write per job)", rel, i+1))
			}
			continue
		}
		if !inBlock {
			continue
		}
		if line != "" && !strings.HasPrefix(line, " ") && !strings.HasPrefix(line, "#") {
			inBlock = false
			continue
		}
		if m := topGrantRE.FindStringSubmatch(line); m != nil && m[2] == "write" && m[1] != "id-token" {
			violations = append(violations,
				fmt.Sprintf("%s:%d: workflow-scoped '%s: write' (move it to the jobs that need it)", rel, i+1, m[1]))
		}
	}
	if !found {
		violations = append(violations,
			fmt.Sprintf("%s: no workflow-level 'permissions:' (add 'permissions:\\n  contents: read')", rel))
	}
	return violations
}

var (
	codeqlAnalyzeRE = regexp.MustCompile(`^\s*(?:-\s+)?uses:\s+github/codeql-action/analyze@`)
	// `pull_request` as a whole word, so `pull_request_target` doesn't count.
	pullRequestWordRE = regexp.MustCompile(`\bpull_request\b`)
)

// checkCodeQLRunsOnPullRequests enforces invariant 5: a workflow that runs CodeQL analysis also
// triggers on `pull_request`. OpenSSF Scorecard's SAST check scores the merged PRs in its window
// by whether CodeQL checked their head commit, and Renovate's PRs land as merged PRs, so a
// push-only CodeQL scores 0 on them however clean the code is.
func checkCodeQLRunsOnPullRequests(lines []string, rel string) []string {
	runsCodeQL := false
	for _, line := range lines {
		if codeqlAnalyzeRE.MatchString(line) {
			runsCodeQL = true
			break
		}
	}
	if !runsCodeQL || triggersOnPullRequest(lines) {
		return nil
	}
	return []string{fmt.Sprintf("%s: runs CodeQL but not on 'pull_request' (Scorecard's SAST check scores merged PRs by it)", rel)}
}

// workflowRunGuard are the conditions every job of a `workflow_run` workflow's `if:` must hold:
// the run it follows was a push (a fork can't push here, and a fork PR's run is a
// `pull_request` one even when its branch is named `main`), to `main`, in this repo.
var workflowRunGuard = []string{
	"github.event.workflow_run.event == 'push'",
	"github.event.workflow_run.head_branch == 'main'",
	"github.event.workflow_run.head_repository.full_name == github.repository",
}

var workflowRunWordRE = regexp.MustCompile(`\bworkflow_run\b`)

// checkWorkflowRunGuard enforces invariant 6. Per job, not once per workflow: a job with
// `always()` runs even when the jobs it needs skipped on their guard.
func checkWorkflowRunGuard(content, rel string) []string {
	if !triggersOn(strings.Split(content, "\n"), workflowRunWordRE, "workflow_run") {
		return nil
	}
	jobs := extractWorkflowJobs(content)
	var ids []string
	for id := range jobs {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	var violations []string
	for _, id := range ids {
		cond := jobScalar(jobs[id], "if")
		for _, g := range workflowRunGuard {
			if !strings.Contains(cond, g) {
				violations = append(violations, fmt.Sprintf(
					"%s: job `%s` runs on `workflow_run` without `%s` in its `if:`; every job must require a push to `main` in this repo, or a fork PR's run could reach it",
					rel, id, g))
				break
			}
		}
	}
	return violations
}

// triggersOnPullRequest reports whether the top-level `on:` names `pull_request`, in either the
// inline form (`on: [push, pull_request]`) or as a key at the block's first indent.
func triggersOnPullRequest(lines []string) bool {
	return triggersOn(lines, pullRequestWordRE, "pull_request")
}

// triggersOn reports whether the top-level `on:` names the trigger `name` (`word` matches it as a
// whole word in the inline form).
func triggersOn(lines []string, word *regexp.Regexp, name string) bool {
	inOn, onIndent := false, -1
	for _, line := range lines {
		trimmed := strings.TrimRight(line, " \t")
		if m := onInlineRE.FindStringSubmatch(trimmed); m != nil {
			return word.MatchString(m[1])
		}
		if onIntroRE.MatchString(trimmed) {
			inOn = true
			continue
		}
		if !inOn || trimmed == "" || strings.HasPrefix(strings.TrimSpace(trimmed), "#") {
			continue
		}
		if !strings.HasPrefix(line, " ") && !strings.HasPrefix(line, "\t") {
			return false
		}
		if m := triggerKeyRE.FindStringSubmatch(line); m != nil {
			if onIndent == -1 {
				onIndent = len(m[1])
			}
			if len(m[1]) == onIndent && m[2] == name {
				return true
			}
		}
	}
	return false
}

// onBlockTracker walks the top-level `on:` block to flag pull_request_target
// triggers, in either inline (`on: [push, pull_request_target]`) or
// block-mapping (`on:\n  pull_request_target:`) form. State is per-file.
type onBlockTracker struct {
	inOnBlock bool
	onIndent  int
}

func (t *onBlockTracker) update(line, trimmed string, lineNum int, rel string) string {
	if onIntroRE.MatchString(trimmed) {
		t.inOnBlock = true
		t.onIndent = -1
		return ""
	}
	if m := onInlineRE.FindStringSubmatch(trimmed); m != nil {
		if strings.Contains(m[1], "pull_request_target") {
			return fmt.Sprintf("%s:%d: pull_request_target trigger (wave-4 entry vector)", rel, lineNum)
		}
		return ""
	}
	if !t.inOnBlock {
		return ""
	}
	return t.processInsideOnBlock(line, trimmed, lineNum, rel)
}

func (t *onBlockTracker) processInsideOnBlock(line, trimmed string, lineNum int, rel string) string {
	if m := triggerKeyRE.FindStringSubmatch(line); m != nil {
		indent := len(m[1])
		if t.onIndent == -1 {
			t.onIndent = indent
		}
		if indent == t.onIndent && m[2] == "pull_request_target" {
			return fmt.Sprintf("%s:%d: pull_request_target trigger (wave-4 entry vector)", rel, lineNum)
		}
		return ""
	}
	// Any line at column 0 (a new top-level key) ends the on: block.
	if !strings.HasPrefix(line, " ") && !strings.HasPrefix(line, "\t") && trimmed != "" {
		t.inOnBlock = false
		t.onIndent = -1
	}
	return ""
}

// isExemptUsesRef returns true for `uses:` targets that don't need SHA pinning:
// local action paths (./...) and own-repo references (where the SHA pin would
// be pointless since the action and the workflow are versioned together).
func isExemptUsesRef(repo string) bool {
	if strings.HasPrefix(repo, "./") || strings.HasPrefix(repo, "../") {
		return true
	}
	return false
}
