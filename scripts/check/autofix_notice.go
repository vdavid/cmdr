package main

import (
	"fmt"
	"os/exec"
	"sort"
	"strings"

	"cmdr/scripts/check/checks"
)

// The auto-fix notice.
//
// Formatters (`oxfmt`, `rustfmt`, prettier, eslint --fix) rewrite the tree on a
// local run and only CHECK in CI. So a reformat that lands AFTER an agent's last
// commit reads green here and red there: the working tree holds the fix, the
// commit doesn't, and nothing in the run's output says the difference matters.
// That reddened CI on 2026-08-18.
//
// The per-check `SuccessWithChanges` line is easy to miss in a fifty-check run, so
// the run brackets itself with a `git status` snapshot and ends by naming, loudly
// and last, every file that was committed and clean when the run started and isn't
// any more.

// dirtyFiles is the set of tracked paths with uncommitted modifications.
type dirtyFiles map[string]bool

// parseDirtyFiles reads `git status --porcelain -z` into the set of MODIFIED
// tracked paths. Untracked entries are left out on purpose: a file a check
// CREATED can't fail CI's check-only run of a formatter, and untracked noise
// would drown the signal.
func parseDirtyFiles(porcelain string) dirtyFiles {
	dirty := dirtyFiles{}
	// -z output is NUL-separated, and a rename entry spends a second field on the
	// old path, so the fields are walked rather than split into lines.
	fields := strings.Split(porcelain, "\x00")
	for i := 0; i < len(fields); i++ {
		entry := fields[i]
		if len(entry) < 4 {
			continue
		}
		status, path := entry[:2], entry[3:]
		if status == "??" || status == "!!" {
			continue
		}
		// `R`/`C` entries are followed by the ORIGINAL path as its own field; the
		// path in this entry is the one that exists now.
		if status[0] == 'R' || status[0] == 'C' {
			i++
		}
		dirty[path] = true
	}
	return dirty
}

// gitDirtyFiles snapshots the tracked modifications in rootDir. A git failure
// (not a work tree, no git) yields nil, which makes the comparison silent rather
// than wrong.
func gitDirtyFiles(rootDir string) dirtyFiles {
	cmd := exec.Command("git", "status", "--porcelain", "-z")
	cmd.Dir = rootDir
	out, err := cmd.Output()
	if err != nil {
		return nil
	}
	return parseDirtyFiles(string(out))
}

// newlyDirty names the files this run dirtied: sorted, and only the ones that were
// clean before it started. A file the author was already editing is theirs, not an
// auto-fixer's, so it stays out.
func newlyDirty(before, after dirtyFiles) []string {
	if before == nil || after == nil {
		return nil
	}
	var out []string
	for path := range after {
		if !before[path] {
			out = append(out, path)
		}
	}
	sort.Strings(out)
	return out
}

// formatAutoFixNotice renders the closing warning.
func formatAutoFixNotice(paths []string) string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "%s📝 This run rewrote %d committed %s.%s CI only CHECKS formatting, so this tree passes here and fails there. Commit:\n",
		colorYellow, len(paths), checks.Pluralize(len(paths), "file", "files"), colorReset)
	for _, path := range paths {
		fmt.Fprintf(&sb, "     %s\n", path)
	}
	return sb.String()
}

// printAutoFixNotice prints the notice when the run dirtied a committed file. It
// goes last, after the summary, because the summary is what a reader stops at.
func printAutoFixNotice(before, after dirtyFiles) {
	paths := newlyDirty(before, after)
	if len(paths) == 0 {
		return
	}
	fmt.Print(formatAutoFixNotice(paths))
}
