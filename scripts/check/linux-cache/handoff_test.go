package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// A worktree's cache may become the main clone's only when the worktree's tree IS the
// main clone's tree, which git ancestry plus a clean status decides. The `--force` flag
// of a teardown says nothing about that, so nothing here looks at it.
func TestLinuxCacheMergedByAncestry(t *testing.T) {
	root := t.TempDir()
	mainClone := filepath.Join(root, "main")
	worktree := filepath.Join(root, "wt")
	if err := os.Mkdir(mainClone, 0o755); err != nil {
		t.Fatal(err)
	}
	runGitIn(t, mainClone, "init", "-q", "-b", "main")
	writeHandoffFile(t, filepath.Join(mainClone, "lib.rs"), "fn a() {}\n")
	runGitIn(t, mainClone, "add", ".")
	runGitIn(t, mainClone, "commit", "-qm", "init")
	runGitIn(t, mainClone, "worktree", "add", "-q", worktree, "-b", "wt")

	expect := func(step string, want bool) {
		t.Helper()
		got, why, err := mergedIntoMain(worktree, mainClone)
		if err != nil {
			t.Fatalf("%s: %v", step, err)
		}
		if got != want {
			t.Errorf("%s: merged = %v (%s), want %v", step, got, why, want)
		}
	}

	expect("a fresh worktree on main's commit", true)

	writeHandoffFile(t, filepath.Join(worktree, "scratch.txt"), "notes\n")
	expect("an untracked file only", true)

	writeHandoffFile(t, filepath.Join(worktree, "lib.rs"), "fn b() {}\n")
	expect("an uncommitted edit to a tracked file", false)

	runGitIn(t, worktree, "commit", "-qam", "edit")
	expect("a commit main doesn't have", false)

	runGitIn(t, mainClone, "merge", "-q", "--ff-only", "wt")
	expect("after the fast-forward into main", true)

	writeHandoffFile(t, filepath.Join(mainClone, "other.rs"), "fn c() {}\n")
	runGitIn(t, mainClone, "add", ".")
	runGitIn(t, mainClone, "commit", "-qm", "main moves on")
	expect("main moved past the merged branch", true)
}

// A checkout with no `rust-toolchain.toml` has no Linux lanes: both directions are a
// silent no-op before Docker is even asked, so a non-Rust project's hooks cost nothing.
func TestLinuxCacheHandoffIsSilentWithoutAToolchainFile(t *testing.T) {
	worktree, mainClone := t.TempDir(), t.TempDir()
	if msg, err := seed(worktree, mainClone); msg != "" || err != nil {
		t.Errorf("seed: got %q, %v; want silence", msg, err)
	}
	if msg, err := promote(worktree, mainClone, false); msg != "" || err != nil {
		t.Errorf("promote: got %q, %v; want silence", msg, err)
	}
}

// The sweep is cross-built for the Docker VM, whose architecture Docker names the kernel's
// way; one it doesn't know gets no sweep rather than a binary that can't run.
func TestSweeperBuildsForDockersArchitecture(t *testing.T) {
	for in, want := range map[string]string{"aarch64": "arm64", "arm64": "arm64", "x86_64": "amd64", "amd64": "amd64", "riscv64": ""} {
		got, ok := goarchFor(in)
		if got != want || ok != (want != "") {
			t.Errorf("goarchFor(%q) = %q, %v; want %q", in, got, ok, want)
		}
	}
}

// The handoff's line carries the sweep's outcome on a second line: its summary when it
// ran, or why it didn't.
func TestWithSweepLine(t *testing.T) {
	cases := []struct{ out, note, want string }{
		{"swept the Linux cache from 9 to 3\n", "", "seeded x\nswept the Linux cache from 9 to 3"},
		{"", "didn't sweep the Linux cache: no arch", "seeded x\ndidn't sweep the Linux cache: no arch"},
		{"", "", "seeded x"},
	}
	for _, c := range cases {
		if got := withSweepLine("seeded x", c.out, c.note); got != c.want {
			t.Errorf("withSweepLine(%q, %q) = %q, want %q", c.out, c.note, got, c.want)
		}
	}
}

func writeHandoffFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func runGitIn(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(),
		"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=test@example.com",
		"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=test@example.com",
	)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v failed: %v\n%s", args, err, out)
	}
}
