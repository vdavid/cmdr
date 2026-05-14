package checks

import (
	"fmt"
	"os"
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

// reminderSkipDirs lists directories that should never be scanned for CLAUDE.md
// files (matches the directories `git status` would never report on either).
var reminderSkipDirs = map[string]bool{
	"vendor":        true,
	"node_modules":  true,
	".cargo-docker": true,
	"target":        true,
	"build":         true,
	"dist":          true,
}

type reminderMiss struct {
	dir   string
	count int
}

// RunClaudeMdReminder warns when source files were changed (in the working tree
// or on the current branch vs the base branch) under a directory that has a
// colocated CLAUDE.md, but the CLAUDE.md itself was not also touched. Always
// succeeds (emits warnings, never fails).
//
// The intent is a low-friction nudge to the agent that just made the change:
// "you touched code under X/, did you mean to update X/CLAUDE.md too?"
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

	changedClaude := make(map[string]bool)
	bucket := make(map[string]int) // CLAUDE.md dir → count of changed source files under it
	for _, f := range changed {
		if filepath.Base(f) == "CLAUDE.md" {
			changedClaude[f] = true
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
		if changedClaude[claudeDirs[dir]] {
			continue
		}
		misses = append(misses, reminderMiss{dir, count})
	}

	if len(misses) == 0 {
		return Success(fmt.Sprintf("All touched directories had matching CLAUDE.md updates (%d %s checked)",
			len(claudeFiles), Pluralize(len(claudeFiles), "doc", "docs"))), nil
	}

	sort.Slice(misses, func(i, j int) bool { return misses[i].dir < misses[j].dir })

	var sb strings.Builder
	for _, m := range misses {
		sb.WriteString(fmt.Sprintf("  - %s/ (%d %s)\n", m.dir, m.count, Pluralize(m.count, "file", "files")))
	}

	msg := fmt.Sprintf("%d %s with source changes but no CLAUDE.md update:\n%s"+
		"Just a friendly reminder: if your changes affect the documented architecture, decisions, or gotchas, consider updating these.",
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

// findClaudeMdFiles walks the repo and returns paths to all CLAUDE.md files
// relative to rootDir. Skips vendor, node_modules, build outputs, hidden dirs.
func findClaudeMdFiles(rootDir string) ([]string, error) {
	var files []string
	err := filepath.WalkDir(rootDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			name := d.Name()
			if strings.HasPrefix(name, ".") || reminderSkipDirs[name] {
				return filepath.SkipDir
			}
			return nil
		}
		if d.Name() == "CLAUDE.md" {
			rel, _ := filepath.Rel(rootDir, path)
			files = append(files, rel)
		}
		return nil
	})
	return files, err
}

// changedFiles returns repo-relative paths of files that differ between the
// working tree and the base branch. The set is the union of:
//
//   - `git status --porcelain=v1 -z` (staged, unstaged, untracked)
//   - `git diff --name-only -z <base>...HEAD` (committed on this branch since
//     diverging from base)
//
// Renames and copies contribute both old and new paths so the doc check fires
// on either side. If neither `origin/main` nor `main` exists, only the working
// tree is consulted.
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

// pickBaseRef returns the first existing ref from the candidate list, or ""
// if none exist (single-branch repo, fresh init, etc.).
func pickBaseRef(rootDir string) string {
	for _, ref := range []string{"origin/main", "main"} {
		if _, err := runGitOut(rootDir, "rev-parse", "--verify", "--quiet", ref); err == nil {
			return ref
		}
	}
	return ""
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
