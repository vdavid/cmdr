package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// asHookEnv makes the test binary behave as the hook program instead of running
// tests, so a throwaway repo's hooks can call back into the code under test and
// git drives it exactly as it does in the real repo.
const asHookEnv = "CMDR_GIT_HOOKS_AS_MAIN"

func TestMain(m *testing.M) {
	if os.Getenv(asHookEnv) != "" {
		os.Exit(run(os.Args[1:], os.Stdin, os.Stderr))
	}
	os.Exit(m.Run())
}

const (
	unformattedGo = "package a\n\nfunc  A( ) {}\n"
	formattedGo   = "package a\n\nfunc A() {}\n"
)

// testRepo is a throwaway git repo with the hooks installed, plus a bare remote.
type testRepo struct {
	t      *testing.T
	dir    string
	remote string
}

func newTestRepo(t *testing.T) *testRepo {
	t.Helper()
	// Git reports the resolved path as the top level; on macOS the temp dir sits
	// behind a symlink.
	base, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	r := &testRepo{t: t, dir: filepath.Join(base, "repo"), remote: filepath.Join(base, "remote.git")}
	if err := os.MkdirAll(r.dir, 0o755); err != nil {
		t.Fatal(err)
	}
	r.git("init", "-q", "-b", "main")
	r.git("config", "user.name", "Test")
	r.git("config", "user.email", "test@example.com")
	r.git("init", "-q", "--bare", r.remote)
	r.git("remote", "add", "origin", r.remote)

	self, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	for _, hook := range []string{"pre-commit", "post-commit", "pre-push"} {
		script := fmt.Sprintf("#!/bin/sh\n%s=1 exec '%s' %s \"$(git rev-parse --show-toplevel)\"\n", asHookEnv, self, hook)
		if err := os.WriteFile(filepath.Join(r.dir, ".git", "hooks", hook), []byte(script), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	r.git("commit", "-q", "--allow-empty", "-m", "init")
	return r
}

// tryGit runs git in the repo, isolated from the machine's own git config.
func (r *testRepo) tryGit(args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = r.dir
	for _, kv := range os.Environ() {
		if !strings.HasPrefix(kv, "GIT_") {
			cmd.Env = append(cmd.Env, kv)
		}
	}
	cmd.Env = append(cmd.Env, "GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_SYSTEM=/dev/null")
	out, err := cmd.CombinedOutput()
	return string(out), err
}

func (r *testRepo) git(args ...string) string {
	r.t.Helper()
	out, err := r.tryGit(args...)
	if err != nil {
		r.t.Fatalf("git %s: %v\n%s", strings.Join(args, " "), err, out)
	}
	return out
}

func (r *testRepo) write(path, content string) {
	r.t.Helper()
	full := filepath.Join(r.dir, filepath.FromSlash(path))
	if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
		r.t.Fatal(err)
	}
	if err := os.WriteFile(full, []byte(content), 0o644); err != nil {
		r.t.Fatal(err)
	}
}

func (r *testRepo) read(path string) string {
	r.t.Helper()
	content, err := os.ReadFile(filepath.Join(r.dir, filepath.FromSlash(path)))
	if err != nil {
		r.t.Fatal(err)
	}
	return string(content)
}

// committed returns a file's content at a revision, in the repo or (with a
// `remote:` prefix on the revision) in the bare remote.
func (r *testRepo) committed(rev, path string) string {
	r.t.Helper()
	if remoteRev, ok := strings.CutPrefix(rev, "remote:"); ok {
		return r.git("--git-dir", r.remote, "show", remoteRev+":"+path)
	}
	return r.git("show", rev+":"+path)
}

func (r *testRepo) status() string {
	r.t.Helper()
	return strings.TrimRight(r.git("status", "--porcelain"), "\n")
}

func (r *testRepo) commitCount() int {
	r.t.Helper()
	return len(strings.Fields(r.git("rev-list", "HEAD")))
}

// commitUnformatted lands a file in a commit without the pre-commit hook seeing it,
// which is how unformatted content reaches a branch in practice: a rebase, a
// conflict resolution, or a `--no-verify` commit.
func (r *testRepo) commitUnformatted(path, content string) {
	r.t.Helper()
	r.write(path, content)
	r.git("add", path)
	r.git("commit", "-q", "--no-verify", "-m", "add "+path)
}

func assertEqual(t *testing.T, what, got, want string) {
	t.Helper()
	if got != want {
		t.Errorf("%s:\n got: %q\nwant: %q", what, got, want)
	}
}
