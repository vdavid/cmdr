package checks

import (
	"strings"
	"testing"
)

// The lane exists to answer CI's question from a Mac, so it has to ask CI's question:
// `desktop-rust-clippy --ci` on ubuntu runs `cargo clippy --locked --all-targets
// --workspace --exclude cmdr-fsevent-stream -- -D warnings`.
func TestLinuxClippyAsksCIsQuestion(t *testing.T) {
	selection := repoSelection(t)
	script := containerCargoScript(linuxClippyArgs(selection)...)

	for _, want := range []string{
		"cargo 'clippy' '--locked' '--all-targets' '--workspace'",
		"'--exclude' 'cmdr-fsevent-stream'",
		"'--' '-D' 'warnings'",
	} {
		if !strings.Contains(script, want) {
			t.Errorf("the Linux clippy command must contain %q, got:\n%s", want, script)
		}
	}
	if strings.Contains(script, "--fix") {
		t.Errorf("the Linux lane reports; the host lane fixes:\n%s", script)
	}
}

// The lane's second question is CI's `desktop-rust-rustdoc --ci` on ubuntu: the same doc
// build under the same denied lints, over the members that build on Linux. A link to an
// item gated to macOS resolves on a Mac and breaks only there, which is how 27 CI runs
// went red on rustdoc between 2026-07 and 2026-10 (`docs/notes/ci-health-2026-10.md`).
func TestLinuxRustdocAsksCIsQuestion(t *testing.T) {
	root := repoRootForTest(t)
	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	args, documented := rustdocArgs(members, "linux")
	if documented == 0 {
		t.Fatal("expected Linux to document at least one member")
	}
	script := linuxRustdocScript(args)

	for _, want := range []string{
		"RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links",
		"-A rustdoc::private_intra_doc_links'",
		"cargo 'doc' '--no-deps' '--all-features' '--document-private-items' '--locked'",
		"'-p' 'cmdr'",
	} {
		if !strings.Contains(script, want) {
			t.Errorf("the Linux rustdoc command must contain %q, got:\n%s", want, script)
		}
	}
	// macOS-only and vendored members don't document on Linux, same as the host lane's rule.
	if strings.Contains(script, "'cmdr-fsevent-stream'") {
		t.Errorf("the macOS-only vendored fork must stay out of the Linux doc build:\n%s", script)
	}
}

// A red lint has to name the file and line; the progress around it is noise.
func TestTrimCargoProgressKeepsTheDiagnostic(t *testing.T) {
	input := `    Checking cmdr-fs v0.1.0 (/repo/crates/cmdr-fs)
   Compiling cmdr v0.1.0 (/repo/apps/desktop/src-tauri)
    Checking cmdr v0.1.0 (/repo/apps/desktop/src-tauri)
error: redundant closure
   --> apps/desktop/src-tauri/src/volumes_linux/mounts.rs:342:21
    |
342 |         .filter_map(|path| path_volume_id(path))
    |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: replace the closure with the function itself: ` + "`path_volume_id`" + `
    |
    = note: ` + "`-D clippy::redundant-closure`" + ` implied by ` + "`-D warnings`" + `
    Checking cmdr-git v0.1.0 (/repo/crates/cmdr-git)
error: could not compile ` + "`cmdr`" + ` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
`
	out := trimCargoProgress(input)

	for _, want := range []string{
		"error: redundant closure",
		"--> apps/desktop/src-tauri/src/volumes_linux/mounts.rs:342:21",
		"help: replace the closure with the function itself",
		"error: could not compile `cmdr` (lib test)",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected %q to survive, got:\n%s", want, out)
		}
	}
	for _, drop := range []string{"Checking cmdr-fs", "Compiling cmdr v", "Checking cmdr-git"} {
		if strings.Contains(out, drop) {
			t.Errorf("expected progress line %q to be dropped, got:\n%s", drop, out)
		}
	}
}

// A diagnostic whose text starts with one of cargo's status words is still a diagnostic.
func TestTrimCargoProgressKeepsDiagnosticsThatQuoteAStatusWord(t *testing.T) {
	input := "error: Checking this invariant failed\n  --> src/lib.rs:1:1\n"
	if got := trimCargoProgress(input); got != strings.TrimSpace(input) {
		t.Errorf("expected the diagnostic unchanged, got:\n%s", got)
	}
}

func TestClippySuccessCountsCrates(t *testing.T) {
	cases := []struct {
		name, output, where, want string
	}{
		{"summary line", "Compiling 12 crates\n", "", "Checked 12 crates, no warnings"},
		{"checking lines", "    Checking a v0.1.0\n    Checking b v0.1.0\n", " on Linux", "Checked 2 crates on Linux, no warnings"},
		{"one crate", "    Checking a v0.1.0\n", " on Linux", "Checked 1 crate on Linux, no warnings"},
		{"warm, nothing rechecked", "    Finished `dev` profile\n", " on Linux", "No warnings on Linux"},
	}
	for _, c := range cases {
		if got := clippySuccess(c.output, c.where).Message; got != c.want {
			t.Errorf("%s: clippySuccess = %q, want %q", c.name, got, c.want)
		}
	}
}
