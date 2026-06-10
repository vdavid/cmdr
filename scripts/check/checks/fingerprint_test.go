package checks

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// initFpGitRepo creates a throwaway git repo with the given files and one
// initial commit, returning its root dir. Files are a map of repo-relative path
// to content.
func initFpGitRepo(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	run := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v failed: %v\n%s", args, err, out)
		}
	}
	run("init", "-q")
	run("config", "user.email", "test@example.com")
	run("config", "user.name", "Test")
	writeFiles(t, dir, files)
	run("add", "-A")
	run("commit", "-q", "-m", "initial")
	return dir
}

func writeFiles(t *testing.T, dir string, files map[string]string) {
	t.Helper()
	for path, content := range files {
		abs := filepath.Join(dir, path)
		if err := os.MkdirAll(filepath.Dir(abs), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(abs, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

func fingerprint(t *testing.T, dir string, inputs []string) string {
	t.Helper()
	data, err := CollectRepoFingerprintData(dir)
	if err != nil {
		t.Fatalf("CollectRepoFingerprintData: %v", err)
	}
	def := &CheckDefinition{ID: "x", Inputs: inputs}
	return data.FingerprintFor(def)
}

func TestFingerprintStableAcrossRuns(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{
		"a.txt":         "hello",
		"sub/b.txt":     "world",
		".mise.toml":    "[tools]",
		"scripts/x.txt": "noise",
	})
	fp1 := fingerprint(t, dir, []string{"a.txt", "sub/**"})
	fp2 := fingerprint(t, dir, []string{"a.txt", "sub/**"})
	if fp1 != fp2 {
		t.Fatalf("same tree should yield same fingerprint: %s vs %s", fp1, fp2)
	}
}

func TestFingerprintTrackedContentChange(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"a.txt": "v1", "b.txt": "other"})
	before := fingerprint(t, dir, []string{"a.txt"})

	// Commit a new version of a.txt: its index blob SHA changes.
	writeFiles(t, dir, map[string]string{"a.txt": "v2"})
	mustGit(t, dir, "add", "-A")
	mustGit(t, dir, "commit", "-q", "-m", "change a")

	after := fingerprint(t, dir, []string{"a.txt"})
	if before == after {
		t.Fatal("changing a tracked input file must change the fingerprint")
	}
}

func TestFingerprintDirtyWorkingTree(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"a.txt": "v1"})
	before := fingerprint(t, dir, []string{"a.txt"})

	// Modify without committing: index SHA is stale, working-tree content differs.
	writeFiles(t, dir, map[string]string{"a.txt": "v1-dirty"})
	after := fingerprint(t, dir, []string{"a.txt"})
	if before == after {
		t.Fatal("an uncommitted edit to an input must change the fingerprint")
	}
}

func TestFingerprintUntrackedAdd(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"src/a.txt": "v1"})
	before := fingerprint(t, dir, []string{"src/**"})

	// Add a new untracked file under the input glob.
	writeFiles(t, dir, map[string]string{"src/new.txt": "fresh"})
	after := fingerprint(t, dir, []string{"src/**"})
	if before == after {
		t.Fatal("an untracked add under an input glob must change the fingerprint")
	}
}

func TestFingerprintDeletion(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"src/a.txt": "v1", "src/b.txt": "v2"})
	before := fingerprint(t, dir, []string{"src/**"})

	if err := os.Remove(filepath.Join(dir, "src/b.txt")); err != nil {
		t.Fatal(err)
	}
	after := fingerprint(t, dir, []string{"src/**"})
	if before == after {
		t.Fatal("deleting an input file must change the fingerprint")
	}
}

func TestFingerprintGlobFiltering(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{
		"rust/a.rs":  "fn main() {}",
		"web/b.ts":   "export {}",
		"shared.txt": "x",
	})
	rustFp := fingerprint(t, dir, []string{"rust/**"})

	// Changing a file OUTSIDE the rust glob must NOT change the rust fingerprint.
	writeFiles(t, dir, map[string]string{"web/b.ts": "export const y = 1"})
	rustFpAfter := fingerprint(t, dir, []string{"rust/**"})
	if rustFp != rustFpAfter {
		t.Fatal("a change outside the input glob must NOT affect the fingerprint")
	}

	// Changing a file INSIDE the rust glob must change it.
	writeFiles(t, dir, map[string]string{"rust/a.rs": "fn main() { let x = 1; }"})
	rustFpChanged := fingerprint(t, dir, []string{"rust/**"})
	if rustFp == rustFpChanged {
		t.Fatal("a change inside the input glob must affect the fingerprint")
	}
}

func TestFingerprintGlobalInputsAffectEveryCheck(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{
		"unrelated.txt": "x",
		".mise.toml":    "[tools]\ngo = '1.25'",
	})
	// A check whose only Inputs are unrelated.txt still picks up .mise.toml via
	// GlobalInputs.
	before := fingerprint(t, dir, []string{"unrelated.txt"})
	writeFiles(t, dir, map[string]string{".mise.toml": "[tools]\ngo = '1.26'"})
	after := fingerprint(t, dir, []string{"unrelated.txt"})
	if before == after {
		t.Fatal("a .mise.toml change must change every check's fingerprint (global input)")
	}
}

func TestFingerprintExactFileMatch(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{"Cargo.lock": "v1", "Cargo.toml": "x"})
	before := fingerprint(t, dir, []string{"Cargo.lock"})
	writeFiles(t, dir, map[string]string{"Cargo.toml": "y"})
	after := fingerprint(t, dir, []string{"Cargo.lock"})
	if before != after {
		t.Fatal("Cargo.toml change must not affect a fingerprint scoped to Cargo.lock")
	}
}

func TestCollectFailsOutsideGitRepo(t *testing.T) {
	dir := t.TempDir() // no git init
	if _, err := CollectRepoFingerprintData(dir); err == nil {
		t.Fatal("expected an error in a non-git directory")
	}
}

func mustGit(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v\n%s", args, err, out)
	}
}

func TestMatchGlob(t *testing.T) {
	cases := []struct {
		pattern, path string
		want          bool
	}{
		{"a.txt", "a.txt", true},
		{"a.txt", "b.txt", false},
		{"src/**", "src/a.txt", true},
		{"src/**", "src/deep/nested/a.txt", true},
		{"src/**", "src", false},
		{"src/**", "srcother/a.txt", false},
		{"apps/desktop/src-tauri/**", "apps/desktop/src-tauri/src/main.rs", true},
		{"apps/desktop/src-tauri/**", "apps/website/index.html", false},
		{"**/package.json", "apps/desktop/package.json", true},
		{"**/package.json", "package.json", true},
		{"*.toml", "Cargo.toml", true},
		{"*.toml", "sub/Cargo.toml", false},
	}
	for _, c := range cases {
		if got := matchGlob(c.pattern, c.path); got != c.want {
			t.Errorf("matchGlob(%q, %q) = %v, want %v", c.pattern, c.path, got, c.want)
		}
	}
}
