package main

import (
	"strings"
	"testing"
)

// A trimmed copy of the real failure (2026-09-09): a worktree whose target/ was
// cloned while the main clone was mid-build. Every implicated crate is a
// third-party one under the registry, which is what makes it diagnosable.
const tornTargetOutput = `clippy found unfixable issues
   Compiling cargo_metadata v0.19.2
error[E0463]: can't find crate for ` + "`camino`" + `
  --> /Users/x/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cargo_metadata-0.19.2/src/lib.rs:81:5
   |
81 | use camino::Utf8PathBuf;
   |     ^^^^^^ can't find crate
error[E0460]: found possibly newer version of crate ` + "`serde_core`" + ` which ` + "`erased_serde`" + ` depends on
 --> /Users/x/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde-untagged-0.1.9/src/seed.rs:8:31
error: could not compile ` + "`cargo_metadata`" + ` (lib) due to 1 previous error`

func TestDetectTornTargetNamesTheImplicatedRegistryPackages(t *testing.T) {
	got := detectTornTarget(tornTargetOutput)

	if len(got) != 2 || got[0] != "cargo_metadata-0.19.2" || got[1] != "serde-untagged-0.1.9" {
		t.Fatalf("detectTornTarget = %v, want the two registry packages, sorted and deduped", got)
	}
}

// The registry path is the whole discriminator. A missing crate in OUR source is
// an ordinary mistake the author should see reported as their own, so an E0463
// that points at the checkout must not be dressed up as a build-cache problem.
func TestDetectTornTargetIgnoresErrorsInOurOwnSource(t *testing.T) {
	output := `error[E0463]: can't find crate for ` + "`nope`" + `
  --> apps/desktop/src-tauri/src/main.rs:3:5
   |
 3 | use nope::Thing;
   |     ^^^^ can't find crate`

	if got := detectTornTarget(output); got != nil {
		t.Fatalf("detectTornTarget = %v, want nil for a failure in our own source", got)
	}
}

// Registry paths show up in ordinary compiler output all the time (a deprecation
// pointing into a dependency, a backtrace frame). Without one of the two unwind-
// the-cache error codes, that is not this problem.
func TestDetectTornTargetNeedsTheErrorCodeNotJustARegistryPath(t *testing.T) {
	output := `warning: use of deprecated function
  --> /Users/x/.cargo/registry/src/index.crates.io-1949cf8c/tokio-1.53.1/src/lib.rs:12:9`

	if got := detectTornTarget(output); got != nil {
		t.Fatalf("detectTornTarget = %v, want nil without E0460/E0463", got)
	}
}

func TestDetectTornTargetIsQuietOnAnOrdinaryFailure(t *testing.T) {
	if got := detectTornTarget("thread 'x' panicked at src/lib.rs:1:1:\nassertion failed"); got != nil {
		t.Fatalf("detectTornTarget = %v, want nil", got)
	}
}

func TestFormatTornTargetNoticeSaysItIsNotTheAuthorsChangeAndGivesTheCommand(t *testing.T) {
	notice := formatTornTargetNotice([]string{"cargo_metadata-0.19.2", "serde-untagged-0.1.9"}, true)

	// The whole point of the notice: stop the reader reverting a good change.
	if !strings.Contains(notice, "not from your change") {
		t.Fatalf("notice must absolve the author's change, got:\n%s", notice)
	}
	if !strings.Contains(notice, "cargo clean --profile dev") {
		t.Fatalf("notice must give the recovery command, got:\n%s", notice)
	}
	if !strings.Contains(notice, "cargo_metadata-0.19.2") {
		t.Fatalf("notice must name the implicated packages, got:\n%s", notice)
	}
	// In a worktree the cause is nearly always the clone racing a live build, and
	// naming it is what stops the next one.
	if !strings.Contains(notice, "worktree-warm-start.md") {
		t.Fatalf("notice in a worktree must point at the warm-start doc, got:\n%s", notice)
	}
}

func TestFormatTornTargetNoticeDropsTheWorktreeCauseInTheMainClone(t *testing.T) {
	notice := formatTornTargetNotice([]string{"cargo_metadata-0.19.2"}, false)

	if strings.Contains(notice, "worktree-warm-start.md") {
		t.Fatalf("main-clone notice must not blame a worktree clone, got:\n%s", notice)
	}
	if !strings.Contains(notice, "cargo clean --profile dev") {
		t.Fatalf("notice must still give the recovery command, got:\n%s", notice)
	}
}

// A long list buries the command, which is the one line that matters.
func TestFormatTornTargetNoticeCapsThePackageList(t *testing.T) {
	many := []string{"a-1.0", "b-1.0", "c-1.0", "d-1.0", "e-1.0", "f-1.0", "g-1.0"}

	notice := formatTornTargetNotice(many, false)

	if strings.Contains(notice, "g-1.0") {
		t.Fatalf("notice should not list every package, got:\n%s", notice)
	}
	if !strings.Contains(notice, "2 more") {
		t.Fatalf("notice should count the packages it elided, got:\n%s", notice)
	}
}
