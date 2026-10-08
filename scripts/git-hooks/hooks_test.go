package main

import (
	"slices"
	"strings"
	"testing"
)

// These tests drive real git against a throwaway repo whose hooks call back into
// this package, with `gofmt` as the formatter under test: it's the one tool
// guaranteed to be present wherever these tests run. `scripts/` is a Go directory
// the gofmt formatter covers.

func TestPreCommitFormatsStagedFiles(t *testing.T) {
	r := newTestRepo(t)
	r.write("scripts/a.go", unformattedGo)
	r.git("add", "scripts/a.go")
	r.git("commit", "-q", "-m", "add a")

	assertEqual(t, "committed content", r.committed("HEAD", "scripts/a.go"), formattedGo)
	assertEqual(t, "working tree content", r.read("scripts/a.go"), formattedGo)
	assertEqual(t, "status", r.status(), "")
}

func TestPreCommitLeavesPartiallyStagedFilesAlone(t *testing.T) {
	r := newTestRepo(t)
	r.write("scripts/a.go", unformattedGo)
	r.git("add", "scripts/a.go")
	// An edit made after staging: formatting the file and re-staging it would pull
	// this into the commit too.
	later := unformattedGo + "\nfunc  B( ) {}\n"
	r.write("scripts/a.go", later)
	r.git("commit", "-q", "-m", "add a")

	assertEqual(t, "committed content", r.committed("HEAD", "scripts/a.go"), unformattedGo)
	assertEqual(t, "working tree content", r.read("scripts/a.go"), later)
}

func TestPreCommitLeavesFilesOutsideEveryFormatterAlone(t *testing.T) {
	r := newTestRepo(t)
	r.write("elsewhere/a.go", unformattedGo)
	r.write("scripts/notes.txt", "some   text\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add files")

	assertEqual(t, "go file outside the Go dirs", r.committed("HEAD", "elsewhere/a.go"), unformattedGo)
	assertEqual(t, "non-go file", r.committed("HEAD", "scripts/notes.txt"), "some   text\n")
	assertEqual(t, "status", r.status(), "")
}

func TestPreCommitHandlesDeletions(t *testing.T) {
	r := newTestRepo(t)
	r.write("scripts/a.go", formattedGo)
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add a")
	r.git("rm", "-q", "scripts/a.go")
	r.git("commit", "-q", "-m", "remove a")

	assertEqual(t, "status", r.status(), "")
}

// `git commit <path>` commits from a temporary index and then restores its own
// copy of the real one, built before the hook ran. Without the post-commit hook
// the file would show as both staged and modified right after a clean commit.
func TestCommitByPathLeavesACleanStatus(t *testing.T) {
	r := newTestRepo(t)
	r.write("scripts/a.go", formattedGo)
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add a")
	r.write("scripts/a.go", unformattedGo+"\nfunc  B( ) {}\n")
	r.write("scripts/b.go", formattedGo)
	r.git("add", "scripts/b.go")
	r.git("commit", "-q", "-m", "add B", "scripts/a.go")

	assertEqual(t, "committed content", r.committed("HEAD", "scripts/a.go"), formattedGo+"\nfunc B() {}\n")
	assertEqual(t, "status", r.status(), "A  scripts/b.go")
}

func TestCommitAllFormats(t *testing.T) {
	r := newTestRepo(t)
	r.write("scripts/a.go", formattedGo)
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add a")
	r.write("scripts/a.go", unformattedGo+"\nfunc  B( ) {}\n")
	r.git("commit", "-q", "-am", "add B")

	assertEqual(t, "committed content", r.committed("HEAD", "scripts/a.go"), formattedGo+"\nfunc B() {}\n")
	assertEqual(t, "status", r.status(), "")
}

func TestPrePushPassesAFormattedBranch(t *testing.T) {
	r := newTestRepo(t)
	r.write("scripts/a.go", formattedGo)
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add a")
	before := r.commitCount()
	r.git("push", "-q", "origin", "main")

	assertEqual(t, "pushed content", r.committed("remote:main", "scripts/a.go"), formattedGo)
	if r.commitCount() != before {
		t.Errorf("the hook added a commit to an already formatted branch")
	}
}

func TestPrePushCommitsTheFormattingAndStopsThePushOnce(t *testing.T) {
	r := newTestRepo(t)
	r.commitUnformatted("scripts/a.go", unformattedGo)
	before := r.commitCount()

	out, err := r.tryGit("push", "origin", "main")
	if err == nil {
		t.Fatalf("the first push went through with unformatted content:\n%s", out)
	}
	if !strings.Contains(out, "Push again") {
		t.Errorf("the stopped push doesn't say what to do next:\n%s", out)
	}
	if got := r.commitCount(); got != before+1 {
		t.Fatalf("commits after the stopped push: got %d, want %d", got, before+1)
	}
	assertEqual(t, "format commit subject", strings.TrimSpace(r.git("log", "-1", "--format=%s")), formatCommitSubject)
	assertEqual(t, "committed content", r.committed("HEAD", "scripts/a.go"), formattedGo)
	assertEqual(t, "status", r.status(), "")

	r.git("push", "-q", "origin", "main")
	assertEqual(t, "pushed content", r.committed("remote:main", "scripts/a.go"), formattedGo)
	if got := r.commitCount(); got != before+1 {
		t.Errorf("the second push added another commit: got %d commits, want %d", got, before+1)
	}
}

func TestPrePushHandlesAForcePush(t *testing.T) {
	r := newTestRepo(t)
	r.write("scripts/a.go", formattedGo)
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add a")
	r.git("push", "-q", "origin", "main")

	// Rewrite the pushed commit with unformatted content.
	r.write("scripts/a.go", unformattedGo)
	r.git("commit", "-q", "--no-verify", "--amend", "-am", "add a")
	if out, err := r.tryGit("push", "--force", "origin", "main"); err == nil {
		t.Fatalf("the force push went through with unformatted content:\n%s", out)
	}
	r.git("push", "-q", "--force", "origin", "main")

	assertEqual(t, "pushed content", r.committed("remote:main", "scripts/a.go"), formattedGo)
}

// An unformatted file with uncommitted edits can't be fixed without sweeping those
// edits into a commit, so the hook stays out of the way and CI reports it.
func TestPrePushLeavesFilesWithUncommittedChangesAlone(t *testing.T) {
	r := newTestRepo(t)
	r.commitUnformatted("scripts/a.go", unformattedGo)
	before := r.commitCount()
	wip := unformattedGo + "\nfunc  WIP( ) {}\n"
	r.write("scripts/a.go", wip)
	r.write("scripts/untracked.go", unformattedGo)

	r.git("push", "-q", "origin", "main")

	assertEqual(t, "working tree content", r.read("scripts/a.go"), wip)
	assertEqual(t, "untracked file", r.read("scripts/untracked.go"), unformattedGo)
	if r.commitCount() != before {
		t.Errorf("the hook committed while the only unformatted files had uncommitted changes")
	}
}

func TestPrePushKeepsStagedWorkOutOfTheFormatCommit(t *testing.T) {
	r := newTestRepo(t)
	r.commitUnformatted("scripts/a.go", unformattedGo)
	r.write("scripts/staged.go", formattedGo)
	r.git("add", "scripts/staged.go")

	if out, err := r.tryGit("push", "origin", "main"); err == nil {
		t.Fatalf("the push went through with unformatted content:\n%s", out)
	}

	files := strings.TrimSpace(r.git("show", "--name-only", "--format=", "HEAD"))
	assertEqual(t, "files in the format commit", files, "scripts/a.go")
	assertEqual(t, "status", r.status(), "A  scripts/staged.go")
}

// The working tree only says something about the commit that's checked out, so a
// push of a tag or of another branch's commit goes through untouched.
func TestPrePushIgnoresRefsThatAreNotCheckedOut(t *testing.T) {
	r := newTestRepo(t)
	r.commitUnformatted("scripts/a.go", unformattedGo)
	before := r.commitCount()
	r.git("tag", "v1")
	r.git("branch", "other", "HEAD~1")

	r.git("push", "-q", "origin", "v1")
	r.git("push", "-q", "origin", "other")
	r.git("push", "-q", "origin", ":other")

	if r.commitCount() != before {
		t.Errorf("the hook committed for a push that didn't include the checked-out branch")
	}
	assertEqual(t, "working tree content", r.read("scripts/a.go"), unformattedGo)
}

func TestPrePushStaysOutOfAnUnfinishedMerge(t *testing.T) {
	r := newTestRepo(t)
	r.commitUnformatted("scripts/a.go", unformattedGo)
	r.git("checkout", "-q", "-b", "side", "HEAD~1")
	r.write("other.txt", "side\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "side")
	r.git("checkout", "-q", "main")
	r.git("merge", "-q", "--no-commit", "--no-ff", "side")
	before := r.commitCount()

	r.git("push", "-q", "origin", "main")

	if r.commitCount() != before {
		t.Errorf("the hook committed in the middle of a merge")
	}
}

func TestParsePushedRefs(t *testing.T) {
	zero := strings.Repeat("0", 40)
	input := strings.Join([]string{
		"refs/heads/main aaa refs/heads/main bbb",
		"HEAD eee refs/heads/next " + zero,
		"refs/tags/v1 ccc refs/tags/v1 " + zero,
		"(delete) " + zero + " refs/heads/gone ddd",
		"",
	}, "\n")

	got, err := pushedBranches(strings.NewReader(input))
	if err != nil {
		t.Fatal(err)
	}
	want := []pushedRef{
		{localRef: "refs/heads/main", localSha: "aaa", remoteRef: "refs/heads/main", remoteSha: "bbb"},
		{localRef: "HEAD", localSha: "eee", remoteRef: "refs/heads/next", remoteSha: zero},
	}
	if !slices.Equal(got, want) {
		t.Errorf("pushed branches: got %v, want %v", got, want)
	}
}
