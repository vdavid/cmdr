package main

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"
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

// enforceMainCloneGuard stops a run started in the main clone, so a check meant
// for a worktree doesn't auto-fix files there by surprise. It's a signpost, not
// a policy: -m is an ordinary way to run, and CI is exempt (it uses --ci). Call
// this AFTER the read-only early-exit flags (--help, --docs-graph), so those
// still work in the main clone.
func enforceMainCloneGuard(flags *cliFlags, rootDir string) {
	if flags.ciMode || flags.allowMain || !isMainWorkingTree(rootDir) {
		return
	}
	printNotice("In the main clone (%s), not a worktree.\n"+
		"Checks auto-fix files. Add -m to run here, "+
		"or cd into .claude/worktrees/<slug>.", rootDir)
	os.Exit(1)
}

// warmingSentinel is written by `~/.claude/scripts/new-worktree.sh` for exactly
// as long as it is still cloning target/ and node_modules into a fresh worktree
// in the background. Building against a half-cloned target/ isn't unsafe, but it
// throws away the warm start the clone exists to provide, so we wait it out.
const warmingSentinel = ".warming-worktree"

// awaitWorktreeWarming blocks while a fresh worktree is still being warmed.
//
// This lives in the check runner rather than in a note to the reader because
// `pnpm check` is the only sanctioned way to build here (AGENTS.md), so one wait
// covers every lane, and nobody has to remember anything. Call it after the main
// clone guard: the main clone is never warming.
//
// A sentinel whose worker has died is treated as "proceed", not "wait forever".
// That's the difference between a stalled clone costing a cold build and it
// costing every future run in the worktree.
func awaitWorktreeWarming(rootDir string) {
	path := filepath.Join(rootDir, warmingSentinel)
	announced := false
	for {
		data, err := os.ReadFile(path)
		if err != nil {
			if announced {
				printNotice("Worktree warm-up finished; continuing.")
			}
			return
		}
		pid := warmingWorkerPID(data)
		if pid <= 0 || !processAlive(pid) {
			printNotice("Found a stale %s (worker pid %d is gone).\n"+
				"Its clone of target/ and node_modules didn't finish, so this run may build cold.\n"+
				"Removing the marker and continuing.", warmingSentinel, pid)
			_ = os.Remove(path)
			return
		}
		if !announced {
			printNotice("This worktree is still warming up (cloning target/ and node_modules, pid %d).\n"+
				"Waiting, so the build starts from a complete cache. Progress: %s",
				pid, filepath.Join(rootDir, warmingSentinel+".log"))
			announced = true
		}
		time.Sleep(500 * time.Millisecond)
	}
}

// warmingWorkerPID pulls the `pid=N` line out of a sentinel, returning 0 when
// it's absent or unparseable (which callers treat as a dead worker).
func warmingWorkerPID(data []byte) int {
	for line := range strings.SplitSeq(string(data), "\n") {
		rest, ok := strings.CutPrefix(strings.TrimSpace(line), "pid=")
		if !ok {
			continue
		}
		pid, err := strconv.Atoi(rest)
		if err != nil {
			return 0
		}
		return pid
	}
	return 0
}

// processAlive reports whether pid is a live process. Signal 0 runs the kernel's
// existence and permission checks without delivering anything; EPERM means the
// process is there but owned by someone else, which still counts as alive.
func processAlive(pid int) bool {
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	err = proc.Signal(syscall.Signal(0))
	return err == nil || errors.Is(err, os.ErrPermission)
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
