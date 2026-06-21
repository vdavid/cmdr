package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// findRootDir finds the project root directory.
// For monorepo structure, it looks for apps/desktop/src-tauri/Cargo.toml.
func findRootDir() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}

	for {
		cargoToml := filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")
		if _, err := os.Stat(cargoToml); err == nil {
			return dir, nil
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("could not find project root (looking for apps/desktop/src-tauri/Cargo.toml)")
		}
		dir = parent
	}
}

// enforceMainCloneGuard exits with a message when checks are run in the main
// clone without the override. The checks auto-fix and reformat files, and the
// solo-dev workflow only ever does that in a worktree. CI is exempt (it runs
// with --ci in the main checkout); pass --allow-main / -m for a deliberate local
// run. Call this AFTER the read-only early-exit flags (--help, freestyle,
// --docs-graph), so those still work in the main clone.
func enforceMainCloneGuard(flags *cliFlags, rootDir string) {
	if flags.ciMode || flags.allowMain || !isMainWorkingTree(rootDir) {
		return
	}
	printError("Refusing to run in the main clone (%s).\n"+
		"Checks run in a worktree — cd into .claude/worktrees/<slug>, "+
		"or pass --allow-main (-m) if you really mean it.", rootDir)
	os.Exit(1)
}

// isMainWorkingTree reports whether dir is the repo's MAIN clone rather than a
// linked `git worktree`. In the main clone, --git-dir and --git-common-dir
// resolve to the same .git; in a linked worktree, --git-dir is
// .git/worktrees/<slug> while --git-common-dir stays the shared .git.
//
// Used to keep tree-mutating runs (the checks auto-fix and reformat files) out
// of the main clone, where the solo-dev workflow never intends them. Returns
// false when git is absent or the dir isn't a repo, so a non-git context never
// blocks.
func isMainWorkingTree(dir string) bool {
	gitDir := gitRevParse(dir, "--git-dir")
	commonDir := gitRevParse(dir, "--git-common-dir")
	if gitDir == "" || commonDir == "" {
		return false
	}
	return resolveAbs(dir, gitDir) == resolveAbs(dir, commonDir)
}

// gitRevParse runs `git rev-parse <arg>` in dir, returning the trimmed output or
// "" on any error.
func gitRevParse(dir, arg string) string {
	cmd := exec.Command("git", "rev-parse", arg)
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

// resolveAbs makes p absolute, treating a relative p as relative to base. Lets
// the git-dir/common-dir compare work across git versions (which return either
// form).
func resolveAbs(base, p string) string {
	if !filepath.IsAbs(p) {
		p = filepath.Join(base, p)
	}
	abs, err := filepath.Abs(p)
	if err != nil {
		return p
	}
	return abs
}
