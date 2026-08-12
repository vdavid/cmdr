package checks

import (
	"strings"
	"testing"
)

// Shaped like real `cargo doc` output, which is the whole point: cargo's own
// progress line runs straight into the first diagnostic with no blank line, and
// the closing `error: could not document` is glued to the warning count above it.
// A fixture with a blank line everywhere lets a paragraph-based filter pass while
// the real thing drops the diagnostic the check exists to show.
const realRustdocOutput = ` Documenting cmdr-index v0.0.0 (/repo/crates/cmdr-index)
warning: unowned lint nobody denied yet
  --> crates/cmdr-index/src/indexing/host/config.rs:16:65
   |
16 | //! [` + "`IndexConfig`" + `] is an INPUT value. [` + "`set_config`" + `] pushes
   |                                                     ^^^^^^^^^^ this item is private
   |
   = note: a note line

error: unresolved link to ` + "`limit`" + `
  --> crates/cmdr-index/src/importance/scheduler/mod.rs:45:26
   |
45 | //!   run -- <db> [limit]
   |                    ^^^^^ no item named ` + "`limit`" + ` in scope
   |
   = help: to escape ` + "`[`" + ` and ` + "`]`" + ` characters, add a backslash before them

 Finished ` + "`dev`" + ` profile in 12.3s
error: could not document ` + "`cmdr-index`" + `
`

func TestRustdocOutputKeepsEveryDiagnosticWhole(t *testing.T) {
	kept := rustdocDiagnostics(realRustdocOutput)
	// Both severities survive: a warning is a lint nobody owns, which the check
	// refuses to pass on just like an error.
	for _, line := range []string{"unowned lint nobody denied yet", "= note: a note line"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the warning block lost %q:\n%s", line, kept)
		}
	}
	// Every line of the error block survives, including the source excerpt and the
	// help note — the diagnostic is useless without them.
	for _, line := range []string{"unresolved link to", "scheduler/mod.rs:45:26", "no item named", "= help: to escape", "could not document"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the error block lost %q:\n%s", line, kept)
		}
	}
	// Cargo's progress lines are noise, wherever they sit. The first one opens the
	// stream and the second is glued between two diagnostics.
	for _, noise := range []string{"Documenting", "Finished"} {
		if strings.Contains(kept, noise) {
			t.Fatalf("cargo's %q progress line is not a diagnostic:\n%s", noise, kept)
		}
	}
}

func TestRustdocOutputKeepsADiagnosticThatOpensTheStream(t *testing.T) {
	// The failure that made a paragraph-based filter useless: the one error was the
	// FIRST diagnostic, so a paragraph split glued it to cargo's progress line and
	// no block started with `error`.
	output := ` Documenting cmdr-index v0.0.0 (/repo/crates/cmdr-index)
error: unresolved link to ` + "`limit`" + `
  --> crates/cmdr-index/src/importance/scheduler/mod.rs:45:26
   |
45 | //!   run -- <db> [limit]
   |                    ^^^^^ no item named ` + "`limit`" + ` in scope
`
	kept := rustdocDiagnostics(output)
	if strings.Contains(kept, "Documenting") {
		t.Fatalf("cargo's progress line came back with the error:\n%s", kept)
	}
	for _, line := range []string{"unresolved link to", "no item named"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the error block lost %q:\n%s", line, kept)
		}
	}
}

func TestRustdocCleanOutputHasNoDiagnostics(t *testing.T) {
	// The green path leans on this: anything left over means a lint fired, so a
	// clean stream must parse to the empty string rather than to its noise.
	clean := " Documenting cmdr v0.38.0 (/repo/apps/desktop/src-tauri)\n Finished `dev` profile in 41.2s\n"
	if got := rustdocDiagnostics(clean); got != "" {
		t.Fatalf("a clean run must parse to nothing, got %q", got)
	}
}

func TestRustdocFailureOutputPassesThroughWhenNothingLooksLikeADiagnostic(t *testing.T) {
	// A toolchain failure, a killed process, or a linker error has no diagnostic
	// block. Swallowing it would report an empty reason for a red check.
	if got := rustdocFailureOutput("some linker noise\nno diagnostics here\n"); !strings.Contains(got, "linker noise") {
		t.Fatalf("unrecognized output must pass through whole, got %q", got)
	}
	if got := rustdocFailureOutput("error[E0124]: field is already declared\n"); !strings.Contains(got, "E0124") {
		t.Fatalf("a compile error is still an error block, got %q", got)
	}
}

func TestRustdocDeniedLintsAreRustdocLints(t *testing.T) {
	// A typo'd lint name is silently accepted by rustdoc (it's an unknown-lint
	// warning at most), so the contract would quietly stop being enforced.
	for _, lint := range rustdocDeniedLints {
		if strings.ContainsAny(lint, ":- ") {
			t.Errorf("%q must be the bare lint name; the `rustdoc::` prefix is added by the check", lint)
		}
	}
	if len(rustdocDeniedLints) == 0 {
		t.Fatal("the deny list is the whole contract; an empty one gates nothing")
	}
}
