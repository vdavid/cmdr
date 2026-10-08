package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestCargoWorkspaceFindsTheOwningPackage(t *testing.T) {
	workspace, err := parseCargoMetadata([]byte(`{
		"workspace_root": "/repo",
		"packages": [
			{"manifest_path": "/repo/apps/desktop/src-tauri/Cargo.toml", "edition": "2024"},
			{"manifest_path": "/repo/crates/fsevent-stream/Cargo.toml", "edition": "2021"},
			{"manifest_path": "/repo/crates/fsevent-stream/nested/Cargo.toml", "edition": "2018"}
		]
	}`))
	if err != nil {
		t.Fatal(err)
	}

	cases := []struct {
		file    string
		edition string // "" when no package owns the file
	}{
		{"apps/desktop/src-tauri/src/lib.rs", "2024"},
		{"crates/fsevent-stream/src/lib.rs", "2021"},
		{"crates/fsevent-stream/nested/src/lib.rs", "2018"},
		{"vendor/mdns-sd/src/lib.rs", ""},
		{"crates/fsevent-stream-other/src/lib.rs", ""},
	}
	for _, c := range cases {
		pkg, ok := workspace.owner(c.file)
		if got := pkg.edition; got != c.edition || ok != (c.edition != "") {
			t.Errorf("owner(%q): got edition %q (found: %v), want %q", c.file, got, ok, c.edition)
		}
	}
}

const (
	// 105 columns once formatted: inside the workspace's 120, past rustfmt's default
	// 100, so it shows whether the workspace's `rustfmt.toml` was honored.
	unformattedRust = "pub fn  f( ) {\n    let some_long_variable_name = compute_something(argument_one, argument_two, argument_three, arg4);\n}\n"
	formattedRust   = "pub fn f() {\n    let some_long_variable_name = compute_something(argument_one, argument_two, argument_three, arg4);\n}\n"
)

func newRustTestRepo(t *testing.T) *testRepo {
	t.Helper()
	for _, tool := range []string{"cargo", "rustfmt"} {
		if err := exec.Command(tool, "--version").Run(); err != nil {
			t.Skipf("%s isn't available here", tool)
		}
	}
	r := newTestRepo(t)
	r.write("Cargo.toml", "[workspace]\nmembers = [\"crates/a\"]\nresolver = \"2\"\n")
	r.write("rustfmt.toml", "max_width = 120\n")
	r.write("crates/a/Cargo.toml", "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n")
	r.write("crates/a/src/lib.rs", "pub mod child;\n")
	r.write("crates/a/src/child.rs", "pub fn child() {}\n")
	r.write(".gitignore", "target/\nCargo.lock\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "workspace")
	return r
}

func TestPreCommitFormatsRustInWorkspaceMembersOnly(t *testing.T) {
	r := newRustTestRepo(t)
	r.write("crates/a/src/child.rs", unformattedRust)
	r.write("vendor/b/src/lib.rs", unformattedRust)
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add rust")

	assertEqual(t, "member file", r.committed("HEAD", "crates/a/src/child.rs"), formattedRust)
	assertEqual(t, "file outside the workspace", r.committed("HEAD", "vendor/b/src/lib.rs"), unformattedRust)
	assertEqual(t, "status", r.status(), "")
}

// Formatting `lib.rs` by path would also rewrite the modules it declares.
func TestPreCommitLeavesUnstagedRustModulesAlone(t *testing.T) {
	r := newRustTestRepo(t)
	r.write("crates/a/src/child.rs", unformattedRust)
	r.write("crates/a/src/lib.rs", "pub mod child;\npub fn  g( ) {}\n")
	r.git("add", "crates/a/src/lib.rs")
	r.git("commit", "-q", "-m", "add g")

	assertEqual(t, "staged file", r.committed("HEAD", "crates/a/src/lib.rs"), "pub mod child;\npub fn g() {}\n")
	assertEqual(t, "unstaged module", r.read("crates/a/src/child.rs"), unformattedRust)
}

func TestPrePushFormatsRust(t *testing.T) {
	r := newRustTestRepo(t)
	r.commitUnformatted("crates/a/src/child.rs", unformattedRust)

	if out, err := r.tryGit("push", "origin", "main"); err == nil {
		t.Fatalf("the push went through with unformatted Rust:\n%s", out)
	}
	assertEqual(t, "committed content", r.committed("HEAD", "crates/a/src/child.rs"), formattedRust)
	r.git("push", "-q", "origin", "main")
}

// oxfmtNodeModules finds the repo's own `node_modules` holding oxfmt by walking up
// from the test's directory. The tool ships in `node_modules`, so a checkout without
// an install skips these tests.
func oxfmtNodeModules(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for {
		if isFile(filepath.Join(dir, "node_modules", ".bin", "oxfmt")) {
			return filepath.Join(dir, "node_modules")
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Skip("oxfmt isn't installed here")
		}
		dir = parent
	}
}

const (
	unformattedTS = "const  a = {\"b\":1}\n"
	formattedTS   = "const a = { b: 1 }\n"
)

func newOxfmtTestRepo(t *testing.T) *testRepo {
	t.Helper()
	nodeModules := oxfmtNodeModules(t)
	r := newTestRepo(t)
	if err := os.Symlink(nodeModules, filepath.Join(r.dir, "node_modules")); err != nil {
		t.Fatal(err)
	}
	r.write(".gitignore", "node_modules\n")
	r.write(".oxfmtrc.json", `{ "semi": false, "singleQuote": true, "ignorePatterns": ["vendor/"] }`+"\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "config")
	return r
}

func TestPreCommitFormatsWithOxfmtAndHonorsItsIgnores(t *testing.T) {
	r := newOxfmtTestRepo(t)
	// A route-style name that git and oxfmt could both mistake for a glob.
	r.write("pages/[slug]/(group)/+page.ts", unformattedTS)
	r.write("vendor/lib.ts", unformattedTS)
	r.write("image.png", "\x89PNG not really\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "add files")

	assertEqual(t, "formatted file", r.committed("HEAD", "pages/[slug]/(group)/+page.ts"), formattedTS)
	assertEqual(t, "ignored file", r.committed("HEAD", "vendor/lib.ts"), unformattedTS)
	assertEqual(t, "status", r.status(), "")
}

// A commit with nothing oxfmt handles mustn't surface oxfmt's "no target file"
// complaint.
func TestPreCommitIsQuietWhenOxfmtHasNothingToDo(t *testing.T) {
	r := newOxfmtTestRepo(t)
	r.write("vendor/lib.ts", unformattedTS)
	r.write("image.png", "\x89PNG not really\n")
	r.git("add", ".")
	out := r.git("commit", "-q", "-m", "add files")

	if strings.TrimSpace(out) != "" {
		t.Errorf("the commit printed:\n%s", out)
	}
}

func TestPrePushFormatsWithOxfmt(t *testing.T) {
	r := newOxfmtTestRepo(t)
	r.commitUnformatted("pages/[slug]/index.ts", unformattedTS)
	r.commitUnformatted("vendor/lib.ts", unformattedTS)

	if out, err := r.tryGit("push", "origin", "main"); err == nil {
		t.Fatalf("the push went through with unformatted TypeScript:\n%s", out)
	}
	assertEqual(t, "committed content", r.committed("HEAD", "pages/[slug]/index.ts"), formattedTS)
	assertEqual(t, "ignored file", r.committed("HEAD", "vendor/lib.ts"), unformattedTS)
	r.git("push", "-q", "origin", "main")
}
