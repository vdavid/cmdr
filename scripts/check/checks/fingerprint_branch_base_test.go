package checks

import (
	"os/exec"
	"testing"
)

func gitIn(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v failed: %v\n%s", args, err, out)
	}
}

func keyedFingerprint(t *testing.T, dir string) string {
	t.Helper()
	data, err := CollectRepoFingerprintData(dir)
	if err != nil {
		t.Fatal(err)
	}
	return data.FingerprintFor(&CheckDefinition{ID: "x", Inputs: []string{"a.txt"}, KeyedOnBranchBase: true})
}

// A lane that diffs against the branch base selects its work from that base, so
// the same files under a different base are different inputs.
func TestFingerprintKeyedOnBranchBaseFollowsTheMergeBase(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"a.txt": "v1"})
	gitIn(t, dir, "update-ref", "refs/remotes/origin/main", "HEAD")
	keyedBefore, plainBefore := keyedFingerprint(t, dir), fingerprint(t, dir, []string{"a.txt"})

	writeFiles(t, dir, map[string]string{"b.txt": "outside the inputs"})
	gitIn(t, dir, "add", "-A")
	gitIn(t, dir, "commit", "-q", "-m", "branch work")
	if keyedFingerprint(t, dir) != keyedBefore {
		t.Error("a commit on the branch leaves the merge base, and so the keyed fingerprint, alone")
	}

	gitIn(t, dir, "update-ref", "refs/remotes/origin/main", "HEAD")
	if keyedFingerprint(t, dir) == keyedBefore {
		t.Error("a moved merge base must change the keyed fingerprint")
	}
	if fingerprint(t, dir, []string{"a.txt"}) != plainBefore {
		t.Error("a check not keyed on the branch base must ignore it")
	}
}

// Without an origin/main the lane diffs against HEAD, so HEAD is the base.
func TestFingerprintKeyedOnBranchBaseFallsBackToHead(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"a.txt": "v1"})
	before := keyedFingerprint(t, dir)
	gitIn(t, dir, "commit", "-q", "--allow-empty", "-m", "x")
	if keyedFingerprint(t, dir) == before {
		t.Error("with no origin/main, a new HEAD must change the keyed fingerprint")
	}
}

// The lane and its fingerprint must agree on the base, so both read it from BranchBase.
func TestBranchBaseIsTheMergeBaseWithOriginMain(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"a.txt": "v1"})
	gitIn(t, dir, "update-ref", "refs/remotes/origin/main", "HEAD")
	base, err := BranchBase(dir)
	if err != nil {
		t.Fatal(err)
	}
	gitIn(t, dir, "commit", "-q", "--allow-empty", "-m", "x")
	if again, _ := BranchBase(dir); again != base {
		t.Errorf("a branch commit must leave the base at %s, got %s", base, again)
	}
}
