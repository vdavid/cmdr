package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// RunCICoverage validates the contract between this check registry and the
// GitHub workflows, in both directions, plus the change-detection filters.
// It exists because all three invariants have silently broken before:
//
//  1. Every `--check <name>` in a workflow must resolve to a registry ID or
//     nickname. (The eslint-typecheck check was split into two named variants
//     and the slow-checks workflow kept invoking the old name, which the
//     check tool rejects, so the nightly job would have failed on every run.)
//  2. Every registry check must either be invoked by some workflow or carry a
//     `NotInCI` reason on its definition. (A dozen checks, including
//     bindings-fresh, which AGENTS.md explicitly described as CI-enforced,
//     were never wired into any workflow.)
//  3. Every path in ci.yml's dorny/paths-filter block must exist: plain
//     entries as files/dirs, glob entries via their static directory prefix.
//     (The svelte filter watched `vite.config.ts` long after the file was
//     renamed to `vite.config.js`, so Vite config changes triggered nothing.)
//
// Scope notes: rule 1 only scans lines that invoke the check tool (the line
// must mention `scripts/check`), so prose in workflow comments doesn't count
// as a reference. Rule 3 treats `**/foo` patterns (empty static prefix) as
// rooted at the repo root, which always exists; that's fine, the rule's job
// is catching renamed/deleted concrete paths, not validating glob semantics.
//
// ciCoverageRegistry is assigned in init() instead of reading AllChecks
// directly: AllChecks's initializer references RunCICoverage (this check is
// itself registered), so a direct reference here would be a compile-error
// initialization cycle. Same reason this uses a local lookup instead of
// GetCheckByID.
var ciCoverageRegistry []CheckDefinition

func init() { ciCoverageRegistry = AllChecks }

func ciCoverageLookup(name string) *CheckDefinition {
	for i := range ciCoverageRegistry {
		if ciCoverageRegistry[i].ID == name || ciCoverageRegistry[i].Nickname == name {
			return &ciCoverageRegistry[i]
		}
	}
	return nil
}

func RunCICoverage(ctx *CheckContext) (CheckResult, error) {
	wfDir := filepath.Join(ctx.RootDir, ".github", "workflows")
	referenced, scannedWorkflows, err := collectWorkflowCheckReferences(wfDir)
	if err != nil {
		return CheckResult{}, err
	}
	if scannedWorkflows == 0 {
		return Skipped("no .github/workflows/"), nil
	}

	var violations []string
	violations = append(violations, validateReferencedNamesResolve(referenced)...)
	violations = append(violations, validateRegistryIsCovered(referenced)...)
	violations = append(violations, validateFilterPathsExist(ctx.RootDir, filepath.Join(wfDir, "ci.yml"))...)

	if len(violations) > 0 {
		return CheckResult{}, fmt.Errorf("registry/CI contract violations:\n  %s", strings.Join(violations, "\n  "))
	}
	return Success(fmt.Sprintf("%d checks reconciled against %d workflows", len(ciCoverageRegistry), scannedWorkflows)), nil
}

// collectWorkflowCheckReferences maps each `--check` name to the workflow
// files invoking it, plus the number of workflow files scanned.
func collectWorkflowCheckReferences(wfDir string) (map[string][]string, int, error) {
	entries, err := os.ReadDir(wfDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, 0, nil
		}
		return nil, 0, fmt.Errorf("failed to read workflows dir: %w", err)
	}
	referenced := make(map[string][]string)
	scanned := 0
	for _, e := range entries {
		if e.IsDir() || (!strings.HasSuffix(e.Name(), ".yml") && !strings.HasSuffix(e.Name(), ".yaml")) {
			continue
		}
		content, readErr := os.ReadFile(filepath.Join(wfDir, e.Name()))
		if readErr != nil {
			return nil, 0, fmt.Errorf("failed to read %s: %w", e.Name(), readErr)
		}
		scanned++
		for _, name := range extractWorkflowCheckNames(string(content)) {
			referenced[name] = append(referenced[name], e.Name())
		}
	}
	return referenced, scanned, nil
}

// validateReferencedNamesResolve is rule 1: every `--check` name in a
// workflow must resolve to a registry ID or nickname.
func validateReferencedNamesResolve(referenced map[string][]string) []string {
	var names []string
	for name := range referenced {
		names = append(names, name)
	}
	sort.Strings(names)
	var violations []string
	for _, name := range names {
		if ciCoverageLookup(name) == nil {
			violations = append(violations, fmt.Sprintf(
				"%s: `--check %s` doesn't match any registry ID or nickname (renamed or removed check?)",
				strings.Join(referenced[name], ", "), name))
		}
	}
	return violations
}

// validateRegistryIsCovered is rule 2: every registry check is either invoked
// by some workflow or excused via a NotInCI reason — and not both.
func validateRegistryIsCovered(referenced map[string][]string) []string {
	var violations []string
	for i := range ciCoverageRegistry {
		c := &ciCoverageRegistry[i]
		isReferenced := len(referenced[c.ID]) > 0 || (c.Nickname != "" && len(referenced[c.Nickname]) > 0)
		switch {
		case !isReferenced && c.NotInCI == "":
			violations = append(violations, fmt.Sprintf(
				"check `%s` is not invoked by any workflow; add a workflow step or set a NotInCI reason in registry.go",
				c.ID))
		case isReferenced && c.NotInCI != "":
			violations = append(violations, fmt.Sprintf(
				"check `%s` has a NotInCI reason but IS invoked by a workflow; remove the stale reason from registry.go",
				c.ID))
		}
	}
	return violations
}

// validateFilterPathsExist is rule 3: every concrete path in ci.yml's
// change-detection filter block must exist on disk.
func validateFilterPathsExist(rootDir, ciPath string) []string {
	ciContent, err := os.ReadFile(ciPath)
	if err != nil {
		return nil
	}
	var violations []string
	for _, p := range extractCIFilterPaths(string(ciContent)) {
		probe := staticPathPrefix(p)
		if probe == "" {
			continue // pattern starts with a glob; nothing concrete to verify
		}
		if _, statErr := os.Stat(filepath.Join(rootDir, probe)); statErr != nil {
			violations = append(violations, fmt.Sprintf(
				"ci.yml change filter references '%s' but '%s' doesn't exist (renamed or removed?)", p, probe))
		}
	}
	return violations
}

// workflowCheckArgRe captures the value passed to a `--check` flag. Values may
// be comma-separated per the CLI contract.
var workflowCheckArgRe = regexp.MustCompile(`--check[= ]+([A-Za-z0-9_,-]+)`)

// extractWorkflowCheckNames returns every check name passed via `--check` on
// lines that invoke the check tool. The `scripts/check` anchor keeps workflow
// comments that merely mention a flag from counting as references.
func extractWorkflowCheckNames(content string) []string {
	var names []string
	for line := range strings.SplitSeq(content, "\n") {
		if !strings.Contains(line, "scripts/check") {
			continue
		}
		for _, m := range workflowCheckArgRe.FindAllStringSubmatch(line, -1) {
			for name := range strings.SplitSeq(m[1], ",") {
				if name != "" {
					names = append(names, name)
				}
			}
		}
	}
	return names
}

// filterPathRe matches one quoted path entry inside the dorny/paths-filter
// block, e.g. `              - 'apps/desktop/src/**'`.
var filterPathRe = regexp.MustCompile(`^\s+- '([^']+)'\s*$`)

// extractCIFilterPaths returns the quoted path patterns inside the
// `filters: |` literal block of ci.yml. The block ends at the first
// non-blank line indented at or left of the `filters:` key itself.
func extractCIFilterPaths(ciYml string) []string {
	var paths []string
	lines := strings.Split(ciYml, "\n")
	inBlock := false
	blockIndent := 0
	for _, line := range lines {
		trimmed := strings.TrimLeft(line, " ")
		indent := len(line) - len(trimmed)
		if !inBlock {
			if strings.TrimSpace(line) == "filters: |" {
				inBlock = true
				blockIndent = indent
			}
			continue
		}
		if strings.TrimSpace(line) == "" {
			continue
		}
		if indent <= blockIndent {
			break
		}
		if m := filterPathRe.FindStringSubmatch(line); m != nil {
			paths = append(paths, m[1])
		}
	}
	return paths
}

// staticPathPrefix returns the longest leading path that contains no glob
// metacharacters: the full pattern if it has none, else the directory prefix
// before the first segment with a glob. Empty when the pattern globs from the
// first segment (e.g. `**/package.json`).
func staticPathPrefix(pattern string) string {
	if !strings.ContainsAny(pattern, "*?[") {
		return pattern
	}
	segments := strings.Split(pattern, "/")
	var static []string
	for _, seg := range segments {
		if strings.ContainsAny(seg, "*?[") {
			break
		}
		static = append(static, seg)
	}
	return strings.Join(static, "/")
}
