package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// reminderSourceExts lists file extensions considered "source code" for the
// purpose of the doc-touch reminder. Other files (json, md, lockfiles, etc.)
// rarely warrant a CLAUDE.md update on their own, so they don't count.
var reminderSourceExts = map[string]bool{
	".rs":     true,
	".ts":     true,
	".svelte": true,
	".css":    true,
	".go":     true,
	".js":     true,
}

// reminderRootDoc is the repo-root push-tier doc. The root `CLAUDE.md` is only
// an `@`-import manifest, so `AGENTS.md` is the doc a root-level change updates.
const reminderRootDoc = "AGENTS.md"

type reminderMiss struct {
	dir   string
	count int
}

// RunClaudeMdReminder warns when source files were changed (in the working tree
// or on the current branch vs local `main`) under a directory that has a
// colocated CLAUDE.md, but no CLAUDE.md, DETAILS.md, or root AGENTS.md on that
// directory's ancestor chain was also touched. Always succeeds (emits warnings,
// never fails).
//
// The intent is a low-friction nudge to the agent that just made the change:
// "you touched code under X/, did you mean to update its colocated docs too?"
// Both halves of that intent constrain the check, and getting either wrong turns
// it into noise the reader learns to skip:
//
//   - "just made" bounds the window (see changedFiles and pickBaseRef): work
//     already merged into `main` is not the change being made now.
//   - "its docs" accepts any tier that covers the code (see docTouchedOnChain):
//     the nearest doc dir isn't always where the change belongs.
func RunClaudeMdReminder(ctx *CheckContext) (CheckResult, error) {
	claudeFiles, err := findClaudeMdFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find CLAUDE.md files: %w", err)
	}
	if len(claudeFiles) == 0 {
		return Success("No CLAUDE.md files found"), nil
	}

	// Map dir → CLAUDE.md path so we can both look up enclosing docs by directory
	// and check whether each doc itself was touched.
	claudeDirs := make(map[string]string, len(claudeFiles))
	for _, f := range claudeFiles {
		claudeDirs[filepath.Dir(f)] = f
	}

	changed, err := changedFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to enumerate changed files: %w", err)
	}
	if len(changed) == 0 {
		return Success(fmt.Sprintf("No changes; %d CLAUDE.md %s left alone",
			len(claudeFiles), Pluralize(len(claudeFiles), "file", "files"))), nil
	}

	changedDocDirs := make(map[string]bool) // dirs whose agent docs were touched
	bucket := make(map[string]int)          // CLAUDE.md dir → count of changed source files under it
	for _, f := range changed {
		if isReminderDoc(f) {
			changedDocDirs[filepath.Dir(f)] = true
			continue
		}
		if !reminderSourceExts[filepath.Ext(f)] {
			continue
		}
		if dir := nearestClaudeDir(f, claudeDirs); dir != "" {
			bucket[dir]++
		}
	}

	var misses []reminderMiss
	for dir, count := range bucket {
		if docTouchedOnChain(dir, changedDocDirs) {
			continue
		}
		misses = append(misses, reminderMiss{dir, count})
	}

	if len(misses) == 0 {
		return Success(fmt.Sprintf("All touched directories had matching CLAUDE.md or DETAILS.md updates (%d %s checked)",
			len(claudeFiles), Pluralize(len(claudeFiles), "doc", "docs"))), nil
	}

	sort.Slice(misses, func(i, j int) bool { return misses[i].dir < misses[j].dir })

	var sb strings.Builder
	for _, m := range misses {
		sb.WriteString(fmt.Sprintf("  - %s/ (%d %s)\n", m.dir, m.count, Pluralize(m.count, "file", "files")))
	}

	msg := fmt.Sprintf("%d %s with source changes but no CLAUDE.md or DETAILS.md update:\n%s"+
		"Friendly reminder: if your changes affect the documented architecture, decisions, or gotchas, updating D.md (default) or C.md (must knows)",
		len(misses),
		Pluralize(len(misses), "directory", "directories"),
		sb.String(),
	)

	return CheckResult{
		Code:    ResultWarning,
		Message: msg,
		Total:   len(claudeFiles),
		Issues:  len(misses),
		Changes: -1,
	}, nil
}

// findClaudeMdFiles returns repo-relative paths to all first-party CLAUDE.md
// files. Git-aware (reuses findMarkdownDocs): it excludes .gitignored scratch and
// build output, vendored/generated trees, and hidden dirs, so the CLAUDE.md checks
// agree with the doc graph on what's in scope (a plain filesystem walk would flag
// gitignored CLAUDE.md files that never get committed).
func findClaudeMdFiles(rootDir string) ([]string, error) {
	docs, err := findMarkdownDocs(rootDir)
	if err != nil {
		return nil, err
	}
	var files []string
	for _, rel := range docs {
		if rel == "CLAUDE.md" || strings.HasSuffix(rel, "/CLAUDE.md") {
			files = append(files, rel)
		}
	}
	return files, nil
}

// changedFiles returns repo-relative paths of the files making up the change
// currently being worked on. The set is the union of:
//
//   - `git status --porcelain=v1 -z` (staged, unstaged, untracked)
//   - `git diff --name-only -z <base>...HEAD` (committed on this branch since
//     diverging from base), when pickBaseRef names a base
//
// Renames and copies contribute both old and new paths so the doc check fires
// on either side. On the base branch itself, or in a repo with no base branch,
// only the working tree is consulted.
func changedFiles(rootDir string) ([]string, error) {
	seen := make(map[string]bool)

	statusOut, err := runGitOut(rootDir, "status", "--porcelain=v1", "-z")
	if err != nil {
		return nil, err
	}
	for _, p := range parsePorcelainZ(statusOut) {
		seen[p] = true
	}

	if base := pickBaseRef(rootDir); base != "" {
		// `<base>...HEAD` diffs against the merge-base, ignoring commits on the
		// base branch since divergence. Falls back gracefully if the diff fails
		// (for example, shallow clone without the merge-base).
		if diffOut, err := runGitOut(rootDir, "diff", "--name-only", "-z", base+"...HEAD"); err == nil {
			for p := range strings.SplitSeq(diffOut, "\x00") {
				if p != "" {
					seen[p] = true
				}
			}
		}
	}

	out := make([]string, 0, len(seen))
	for p := range seen {
		out = append(out, p)
	}
	return out, nil
}

// pickBaseRef returns the ref this branch's committed work should be compared
// against, or "" when there's no branch-shaped unit of work to scope to (we're
// on the base branch, or the repo has no base branch at all).
//
// LOCAL `main` is the base, never `origin/main`, and that ordering is the whole
// point: branches are cut from local `main` and fast-forwarded back into it, so
// local `main` is the actual branch point. `main` here also routinely sits many
// unpushed commits ahead of the remote, so basing on `origin/main` widens the
// window to "everything not yet pushed": every source change in that pile
// re-warns on every run for days, until a push that has nothing to do with
// whether the change was documented. This check never runs in CI (see its
// registry entry), so the remote's view of the branch point is never the
// relevant one.
func pickBaseRef(rootDir string) string {
	branch := currentBranch(rootDir)
	for _, ref := range []string{"main", "origin/main"} {
		if _, err := runGitOut(rootDir, "rev-parse", "--verify", "--quiet", ref); err != nil {
			continue
		}
		if branch != "" && strings.TrimPrefix(ref, "origin/") == branch {
			// On the base branch itself: the working tree is the current change.
			return ""
		}
		return ref
	}
	return ""
}

// currentBranch returns the short name of the checked-out branch, or "" when
// HEAD is detached.
func currentBranch(rootDir string) string {
	out, err := runGitOut(rootDir, "symbolic-ref", "--short", "--quiet", "HEAD")
	if err != nil {
		return ""
	}
	return strings.TrimSpace(out)
}

func runGitOut(rootDir string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = rootDir
	var stdout, stderr strings.Builder
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("git %s: %w\n%s", strings.Join(args, " "), err, stderr.String())
	}
	return stdout.String(), nil
}

// parsePorcelainZ extracts file paths from `git status --porcelain=v1 -z` output.
//
// Each record is `XY<space>path` terminated by NUL. Renames (`R`) and copies (`C`)
// add a second NUL-terminated field after the new path: the original path. We
// surface both so doc-touch attribution works whichever side the agent thinks of.
func parsePorcelainZ(s string) []string {
	var paths []string
	rest := s
	for len(rest) > 0 {
		idx := strings.IndexByte(rest, 0)
		if idx < 0 {
			break
		}
		entry := rest[:idx]
		rest = rest[idx+1:]
		if len(entry) < 4 {
			continue
		}
		xy := entry[:2]
		paths = append(paths, entry[3:])
		if xy[0] == 'R' || xy[0] == 'C' {
			origIdx := strings.IndexByte(rest, 0)
			if origIdx >= 0 {
				if orig := rest[:origIdx]; orig != "" {
					paths = append(paths, orig)
				}
				rest = rest[origIdx+1:]
			}
		}
	}
	return paths
}

// isReminderDoc reports whether a changed path is one of the agent docs that
// count as documenting a change: either tier of a colocated pair, or the
// repo-root AGENTS.md.
func isReminderDoc(rel string) bool {
	if rel == reminderRootDoc {
		return true
	}
	base := filepath.Base(rel)
	return base == "CLAUDE.md" || base == "DETAILS.md"
}

// docTouchedOnChain reports whether dir or any of its ancestors (up to and
// including the repo root) had an agent doc changed.
//
// Attribution buckets a source file into its NEAREST CLAUDE.md dir, but that
// isn't always the tier the change belongs in: a detail about a deep module can
// legitimately live in a parent's DETAILS.md, or in the root AGENTS.md when it's
// repo-wide. Demanding the nearest doc specifically nags at changes that are
// already documented, one directory up.
//
// This does mean a change touching AGENTS.md silences the reminder repo-wide,
// including for a second subsystem the author didn't document. That's the right
// trade for a warn-only nudge: someone editing the hub doc has demonstrably
// thought about docs, and over-nagging is what stops the check being read at all.
func docTouchedOnChain(dir string, changedDocDirs map[string]bool) bool {
	for {
		if changedDocDirs[dir] {
			return true
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return false
		}
		dir = parent
	}
}

// nearestClaudeDir walks up from filePath's directory and returns the nearest
// directory that has a CLAUDE.md, or "" if no ancestor has one.
func nearestClaudeDir(filePath string, claudeDirs map[string]string) string {
	dir := filepath.Dir(filePath)
	for {
		if _, ok := claudeDirs[dir]; ok {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}
		dir = parent
	}
}
