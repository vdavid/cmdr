package main

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

// testOrigin makes a bare origin with one commit on `main` and returns it. Git runs with no global
// or system config, so the developer's hooks and settings stay out of it.
func testOrigin(t *testing.T) string {
	t.Helper()
	for k, v := range map[string]string{
		"GIT_CONFIG_GLOBAL":   os.DevNull,
		"GIT_CONFIG_NOSYSTEM": "1",
		"GIT_AUTHOR_NAME":     "Land Test",
		"GIT_AUTHOR_EMAIL":    "land@example.com",
		"GIT_COMMITTER_NAME":  "Land Test",
		"GIT_COMMITTER_EMAIL": "land@example.com",
	} {
		t.Setenv(k, v)
	}
	dir := t.TempDir()
	origin := filepath.Join(dir, "origin.git")
	mustGit(t, runner{dir: dir}, "init", "--quiet", "--bare", "-b", "main", origin)
	seed := testClone(t, origin, "main")
	mustGit(t, seed, "commit", "--quiet", "--allow-empty", "-m", "init")
	mustGit(t, seed, "push", "--quiet", "origin", "main")
	return origin
}

// testClone clones origin into a fresh dir and checks out branch, creating it from `main` when
// origin doesn't have it.
func testClone(t *testing.T, origin, branch string) runner {
	t.Helper()
	dir := t.TempDir()
	mustGit(t, runner{dir: dir}, "clone", "--quiet", origin, "clone")
	r := runner{dir: filepath.Join(dir, "clone")}
	if branch != "main" {
		if sha, _ := r.remoteSHA(branch); sha != "" {
			mustGit(t, r, "switch", "--quiet", branch)
		} else {
			mustGit(t, r, "switch", "--quiet", "-c", branch)
		}
	}
	return r
}

func mustGit(t *testing.T, r runner, args ...string) string {
	t.Helper()
	out, err := r.git(args...)
	if err != nil {
		t.Fatal(err)
	}
	return out
}

func commitAndPush(t *testing.T, r runner, branch, message string) string {
	t.Helper()
	mustGit(t, r, "commit", "--quiet", "--allow-empty", "-m", message)
	head := mustGit(t, r, "rev-parse", "HEAD")
	if _, err := pushBranch(r, branch, head); err != nil {
		t.Fatal(err)
	}
	return head
}

func originSHA(t *testing.T, r runner, branch string) string {
	t.Helper()
	sha, err := r.remoteSHA(branch)
	if err != nil {
		t.Fatal(err)
	}
	return sha
}

// Regression: PR #481, where origin's branch had a commit from GitHub's "Update branch" button
// and the push overwrote it.
func TestPushBranchKeepsCommitsOnlyOriginHas(t *testing.T) {
	origin := testOrigin(t)
	a := testClone(t, origin, "feat")
	commitAndPush(t, a, "feat", "feat: from laptop A")

	b := testClone(t, origin, "feat")
	mustGit(t, b, "commit", "--quiet", "--allow-empty", "-m", "feat: from laptop B")
	mustGit(t, b, "push", "--quiet", "origin", "feat")
	theirs := mustGit(t, b, "rev-parse", "HEAD")

	mustGit(t, a, "commit", "--quiet", "--allow-empty", "-m", "feat: more from laptop A")
	_, err := pushBranch(a, "feat", mustGit(t, a, "rev-parse", "HEAD"))
	if !errors.Is(err, errOriginAhead) {
		t.Errorf("got %v, want errOriginAhead", err)
	}
	if got := originSHA(t, a, "feat"); got != theirs {
		t.Errorf("origin/feat is %s, want laptop B's %s kept", got, theirs)
	}
}

// The race backstop: past `pushBranch`'s own check, git's `--force-if-includes` still refuses a
// tracking ref that was fetched but never integrated. Also pins that git honors it with the bare
// lease (it ignores it next to an explicit lease value).
func TestPushToOriginRefusesAFetchedButUnintegratedTip(t *testing.T) {
	origin := testOrigin(t)
	a := testClone(t, origin, "feat")
	commitAndPush(t, a, "feat", "feat: from laptop A")

	b := testClone(t, origin, "feat")
	mustGit(t, b, "commit", "--quiet", "--allow-empty", "-m", "feat: from laptop B")
	mustGit(t, b, "push", "--quiet", "origin", "feat")
	theirs := mustGit(t, b, "rev-parse", "HEAD")

	if _, err := a.fetchBranch("feat"); err != nil {
		t.Fatal(err)
	}
	if err := pushToOrigin(a, "feat"); err == nil {
		t.Error("the push went through")
	}
	if got := originSHA(t, a, "feat"); got != theirs {
		t.Errorf("origin/feat is %s, want laptop B's %s kept", got, theirs)
	}
}

func TestPushBranchOverwritesWhatThisBranchRewrote(t *testing.T) {
	origin := testOrigin(t)
	a := testClone(t, origin, "feat")
	commitAndPush(t, a, "feat", "feat: one")
	commitAndPush(t, a, "feat", "feat: two")

	// What `-squash` does.
	mustGit(t, a, "reset", "--soft", "origin/main")
	mustGit(t, a, "commit", "--quiet", "--allow-empty", "-m", "feat: one and two")
	head := mustGit(t, a, "rev-parse", "HEAD")
	if _, err := pushBranch(a, "feat", head); err != nil {
		t.Fatal(err)
	}
	if got := originSHA(t, a, "feat"); got != head {
		t.Errorf("origin/feat is %s, want the squash %s", got, head)
	}
}

func TestPushBranchFirstPush(t *testing.T) {
	a := testClone(t, testOrigin(t), "feat")
	head := commitAndPush(t, a, "feat", "feat: new")
	if got := originSHA(t, a, "feat"); got != head {
		t.Errorf("origin/feat is %q, want %s", got, head)
	}
}

// A land deletes the remote branch; landing more on the same branch later pushes it afresh even
// though the local remote-tracking ref still names the deleted tip.
func TestPushBranchAfterOriginDeletedIt(t *testing.T) {
	origin := testOrigin(t)
	a := testClone(t, origin, "feat")
	commitAndPush(t, a, "feat", "feat: landed")
	other := testClone(t, origin, "main")
	mustGit(t, other, "push", "--quiet", "origin", "--delete", "feat")

	head := commitAndPush(t, a, "feat", "feat: more")
	if got := originSHA(t, a, "feat"); got != head {
		t.Errorf("origin/feat is %q, want %s", got, head)
	}
}

// A GitHub rebase merge lands the branch's patches on `main` under fresh SHAs; a re-run of land
// must still see the branch as landed.
func TestAllUpstreamAfterAGitHubRebaseMerge(t *testing.T) {
	origin := testOrigin(t)
	a := testClone(t, origin, "feat")
	commitFile := func(r runner, name string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(r.dir, name), []byte(name), 0o644); err != nil {
			t.Fatal(err)
		}
		mustGit(t, r, "add", name)
		mustGit(t, r, "commit", "--quiet", "-m", "feat: "+name)
	}
	commitFile(a, "feat.txt")
	feat := mustGit(t, a, "rev-parse", "HEAD")

	other := testClone(t, origin, "main")
	commitFile(other, "other.txt")
	mustGit(t, other, "push", "--quiet", "origin", "main")
	mustGit(t, a, "fetch", "--quiet", "origin", "main")
	if a.allUpstream() {
		t.Error("before the merge: the branch's commit isn't on main yet")
	}

	// What GitHub's rebase merge does.
	mustGit(t, other, "fetch", "--quiet", "origin", "main")
	mustGit(t, other, "fetch", "--quiet", a.dir, feat)
	mustGit(t, other, "cherry-pick", feat)
	mustGit(t, other, "push", "--quiet", "origin", "main")
	mustGit(t, a, "fetch", "--quiet", "origin", "main")
	if !a.allUpstream() {
		t.Error("after the merge: every commit is on main, patch for patch")
	}
	if a.isAncestor("HEAD", "origin/main") {
		t.Error("the test should exercise fresh SHAs, but HEAD is an ancestor of main")
	}
}

// The pre-push hook may commit a formatting fix and stop the push; land pushes again with it.
func TestPushBranchRetriesWithTheHooksCommit(t *testing.T) {
	a := testClone(t, testOrigin(t), "feat")
	hook := filepath.Join(a.dir, ".git", "hooks", "pre-push")
	script := "#!/bin/sh\n[ -f .git/hook-ran ] && exit 0\ntouch .git/hook-ran\ngit commit --quiet --allow-empty -m 'style: format'\nexit 1\n"
	if err := os.WriteFile(hook, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	mustGit(t, a, "commit", "--quiet", "--allow-empty", "-m", "feat: new")
	pushed, err := pushBranch(a, "feat", mustGit(t, a, "rev-parse", "HEAD"))
	if err != nil {
		t.Fatal(err)
	}
	if head := mustGit(t, a, "rev-parse", "HEAD"); pushed != head || originSHA(t, a, "feat") != head {
		t.Errorf("pushed %s, origin has %s, want the hook's commit %s", pushed, originSHA(t, a, "feat"), head)
	}
}

// `-merge` exists for a release commit whose tag must stay on `main`'s history; a plain land
// spots the tag and refuses to rebase it.
func TestTaggedFindsATagOnTheBranch(t *testing.T) {
	a := testClone(t, testOrigin(t), "release/v1.2.3")
	if a.tagged("origin/main..HEAD") {
		t.Error("no commits, no tags")
	}
	mustGit(t, a, "commit", "--quiet", "--allow-empty", "-m", "chore(release): v1.2.3")
	if a.tagged("origin/main..HEAD") {
		t.Error("an untagged commit isn't tagged")
	}
	mustGit(t, a, "tag", "-a", "-m", "Cmdr v1.2.3", "v1.2.3")
	if !a.tagged("origin/main..HEAD") {
		t.Error("an annotated tag on the branch's commit")
	}
	mustGit(t, a, "tag", "-d", "v1.2.3")
	mustGit(t, a, "tag", "light")
	if !a.tagged("origin/main..HEAD") {
		t.Error("a lightweight tag on the branch's commit")
	}
	mustGit(t, a, "tag", "-d", "light")
	mustGit(t, a, "tag", "old", "origin/main")
	if a.tagged("origin/main..HEAD") {
		t.Error("a tag on main itself isn't on the branch")
	}
}
