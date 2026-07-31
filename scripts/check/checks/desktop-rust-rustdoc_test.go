package checks

import (
	"strings"
	"testing"
)

// Shaped like real `cargo doc` output, which is the whole point: cargo's own
// progress line runs straight into the first diagnostic with no blank line, and
// the closing `error: could not document` is glued to the warning count above it.
// A fixture with a blank line everywhere lets a paragraph-based filter pass while
// the real thing drops the error the check exists to show.
const realRustdocOutput = ` Documenting cmdr-index v0.0.0 (/repo/crates/cmdr-index)
warning: public documentation for ` + "`config`" + ` links to private item ` + "`set_config`" + `
  --> crates/cmdr-index/src/indexing/host/config.rs:16:65
   |
16 | //! [` + "`IndexConfig`" + `] is an INPUT value. [` + "`set_config`" + `] pushes
   |                                                     ^^^^^^^^^^ this item is private
   |
   = note: this link will resolve properly if you pass ` + "`--document-private-items`" + `

error: unresolved link to ` + "`limit`" + `
  --> crates/cmdr-index/src/importance/scheduler/mod.rs:45:26
   |
45 | //!   run -- <db> [limit]
   |                    ^^^^^ no item named ` + "`limit`" + ` in scope
   |
   = help: to escape ` + "`[`" + ` and ` + "`]`" + ` characters, add a backslash before them

warning: redundant explicit link target
 --> crates/cmdr-index/src/media_index/mod.rs:6:24
  |
6 | //! the [` + "`MediaIndex`" + `](read::MediaIndex) read API surfaced over the
  |          ------------  ^^^^^^^^^^^^^^^^ explicit target is redundant
  |
help: remove explicit link target
  |
6 - //! the [` + "`MediaIndex`" + `](read::MediaIndex) read API surfaced over the
6 + //! the [` + "`MediaIndex`" + `] read API surfaced over the
  |

warning: ` + "`cmdr-index`" + ` (lib doc) generated 69 warnings
error: could not document ` + "`cmdr-index`" + `
`

func TestRustdocOutputKeepsErrorsWholeAndDropsWarnings(t *testing.T) {
	kept := rustdocErrorDiagnostics(realRustdocOutput)
	if strings.Contains(kept, "warning:") {
		t.Fatalf("warnings must be dropped whole:\n%s", kept)
	}
	if strings.Contains(kept, "Documenting") {
		t.Fatalf("cargo's progress line goes with the diagnostic it runs into:\n%s", kept)
	}
	// Every line of the error block survives, including the source excerpt and the
	// help note — the diagnostic is useless without them.
	for _, line := range []string{"unresolved link to", "scheduler/mod.rs:45:26", "no item named", "= help: to escape", "could not document"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the error block lost %q:\n%s", line, kept)
		}
	}
	// A `help:` block is a continuation of the warning above it, so it goes too.
	if strings.Contains(kept, "remove explicit link target") {
		t.Fatalf("a warning's continuation lines must go with it:\n%s", kept)
	}
}

func TestRustdocOutputKeepsAnErrorThatOpensTheStream(t *testing.T) {
	// The failure that made the filter useless: the one error was the FIRST
	// diagnostic, so a paragraph split glued it to cargo's progress line, no block
	// started with `error`, and the fallback dumped every warning instead.
	output := ` Documenting cmdr-index v0.0.0 (/repo/crates/cmdr-index)
error: unresolved link to ` + "`limit`" + `
  --> crates/cmdr-index/src/importance/scheduler/mod.rs:45:26
   |
45 | //!   run -- <db> [limit]
   |                    ^^^^^ no item named ` + "`limit`" + ` in scope

warning: public documentation for ` + "`config`" + ` links to private item ` + "`set_config`" + `
  --> crates/cmdr-index/src/indexing/host/config.rs:16:65
`
	kept := rustdocErrorDiagnostics(output)
	if strings.Contains(kept, "warning:") || strings.Contains(kept, "Documenting") {
		t.Fatalf("the whole output came back instead of the one error:\n%s", kept)
	}
	for _, line := range []string{"unresolved link to", "no item named"} {
		if !strings.Contains(kept, line) {
			t.Fatalf("the error block lost %q:\n%s", line, kept)
		}
	}
}

func TestRustdocOutputPassesThroughWhenNothingLooksLikeADiagnostic(t *testing.T) {
	// A toolchain or compile failure has no `error:` diagnostic block. Swallowing it
	// would report an empty reason for a red check.
	output := "error[E0124]: field is already declared\n"
	if got := rustdocErrorDiagnostics("some linker noise\nno diagnostics here\n"); !strings.Contains(got, "linker noise") {
		t.Fatalf("unrecognized output must pass through whole, got %q", got)
	}
	if got := rustdocErrorDiagnostics(output); !strings.Contains(got, "E0124") {
		t.Fatalf("a compile error is still an error block, got %q", got)
	}
}
