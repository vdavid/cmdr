package main

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

const (
	noticesFile          = "THIRD-PARTY-NOTICES.md"
	noticesPackagesFile  = "apps/desktop/src/lib/licensing/third-party-packages.gen.json"
	noticesCommitSubject = "chore(deps): regenerate third-party notices"

	// checkRunnerEnv points the hooks at a stand-in for `scripts/check`. Only the
	// tests set it: the throwaway repos they push from have no check runner.
	checkRunnerEnv = "CMDR_GIT_HOOKS_CHECK_RUNNER"
)

// noticesInputs are the files whose change in a push can make the committed
// notices stale: the two lockfiles plus the license policy and the hand-kept
// credits, mirroring the `desktop-third-party-notices` check's own inputs.
var noticesInputs = map[string]bool{
	"Cargo.lock":     true,
	"pnpm-lock.yaml": true,
	"deny.toml":      true,
	"scripts/check/checks/third-party-vendored.json": true,
}

// checkRunner runs the repo's check runner with args and returns its combined output.
type checkRunner func(args ...string) (string, error)

// newCheckRunner returns the worktree's check runner, or nil when it has none (a
// checkout from before `scripts/check` existed), which makes the checks below skip.
func newCheckRunner(root string) checkRunner {
	if fake := os.Getenv(checkRunnerEnv); fake != "" {
		return commandRunner(root, fake)
	}
	dir := filepath.Join(root, "scripts", "check")
	if !isDir(dir) {
		return nil
	}
	return commandRunner(dir, "go", "run", ".")
}

func commandRunner(dir, name string, prefix ...string) checkRunner {
	return func(args ...string) (string, error) {
		cmd := exec.Command(name, append(append([]string{}, prefix...), args...)...)
		cmd.Dir = dir
		out, err := cmd.CombinedOutput()
		return string(out), err
	}
}

// regenerateNotices keeps a dependency push from reaching CI with stale license
// notices, which turned `main` red 26 times in three months
// (`docs/notes/ci-health-2026-10.md`). It only runs when the push moves one of
// `noticesInputs`, because the regeneration is a fresh `cargo-about` walk of the
// whole dependency graph (about a minute). Like the formatter step, it commits what
// it rewrote and stops the push.
func regenerateNotices(r repo, check checkRunner, pushed pushedRef, stderr io.Writer) (stopPush bool, err error) {
	if check == nil || !touchesNoticesInputs(r, pushed) {
		return false, nil
	}
	fixable, err := fixableFiles(r, []string{noticesFile, noticesPackagesFile})
	if err != nil || len(fixable) == 0 {
		return false, err
	}
	fmt.Fprintln(stderr, "This push moves a lockfile, so the hook is regenerating THIRD-PARTY-NOTICES.md (about a minute)...")
	// `--fresh`: a cache hit would skip the very rewrite this is for. `--allow-main`:
	// the main clone is where pushes happen.
	if out, err := check("--check", "desktop-third-party-notices", "--fresh", "--allow-main", "--no-log"); err != nil {
		fmt.Fprintf(stderr, "notices hook: skipped, the check runner didn't finish:\n%s", out)
		return false, nil
	}
	modified, err := r.paths("diff", "--name-only", "-z")
	if err != nil {
		return false, err
	}
	regenerated := intersect(fixable, toSet(modified))
	if len(regenerated) == 0 {
		return false, nil
	}
	sha, err := commitOnly(r, regenerated, noticesCommitSubject)
	if err != nil {
		return false, err
	}
	fmt.Fprintf(stderr, "Regenerated the third-party notices and committed them as %s (%q).\n"+
		"Git can't add a commit to a push that's already running, so this push stopped. %s\n",
		sha, noticesCommitSubject, pushAgain(r, pushed))
	return true, nil
}

// touchesNoticesInputs reports whether the push changes any of `noticesInputs`. The
// range runs from the remote tip to the pushed commit. A new branch has no remote
// tip, and this clone may not have the one it's told about, so then the range is
// what the pushed commit adds over every remote-tracking ref. When even that can't
// be read, it counts as a change: regenerating once too often only costs a minute.
func touchesNoticesInputs(r repo, pushed pushedRef) bool {
	var changed []string
	var err error
	if !isZeroSha(pushed.remoteSha) && r.hasCommit(pushed.remoteSha) {
		changed, err = r.paths("diff", "--name-only", "-z", pushed.remoteSha, pushed.localSha)
	} else {
		changed, err = r.paths("log", "--format=", "--name-only", "-z", pushed.localSha, "--not", "--remotes")
	}
	if err != nil {
		return true
	}
	for _, p := range changed {
		if noticesInputs[p] {
			return true
		}
	}
	return false
}

// checkBudgets runs CI's two whole-repo size limits, `file-length` and
// `claude-md-length`. Two commits can each sit under a limit and cross it together,
// which only a run over the combined tree sees. They take milliseconds, so they run
// on every push, in `--ci` mode so they report without rewriting their allowlists.
//
// They read the working tree, so they skip it when uncommitted or untracked work
// would stand in for the pushed commit. Unlike the steps above there's nothing to
// commit: a failure stops the push and shows why.
func checkBudgets(r repo, check checkRunner, stderr io.Writer) (stopPush bool, err error) {
	if check == nil {
		return false, nil
	}
	dirty, err := r.git("status", "--porcelain")
	if err != nil {
		return false, err
	}
	if strings.TrimSpace(dirty) != "" {
		fmt.Fprintln(stderr, "budget hook: skipped file-length and claude-md-length, the working tree has uncommitted changes. CI still checks them.")
		return false, nil
	}
	out, err := check("--ci", "--check", "file-length", "--check", "claude-md-length")
	if err == nil {
		return false, nil
	}
	fmt.Fprintf(stderr, "%s\nThis push would fail CI's size limits, so it stopped. Split or trim the file (or bump its allowlist entry with a reason), "+
		"then push again. `git push --no-verify` sends it anyway.\n", strings.TrimRight(out, "\n"))
	return true, nil
}
