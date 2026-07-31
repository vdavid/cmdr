package checks

import (
	"strings"
	"testing"
)

func TestRustdocOutputKeepsErrorsWholeAndDropsWarnings(t *testing.T) {
	output := `warning: public documentation for ` + "`config`" + ` links to private item
  --> a.rs:1:1
   |
   = note: chatter

error: unresolved link to ` + "`limit`" + `
  --> b.rs:10:62
   |
10 | //!   run -- <db> [limit]
   |                    ^^^^^ no item named ` + "`limit`" + ` in scope
   |
   = help: escape it

warning: another one
  --> c.rs:2:2

error: could not document ` + "`operation-log-dump`" + `
`
	kept := rustdocErrorDiagnostics(output)
	if strings.Contains(kept, "warning:") {
		t.Fatalf("warnings must be dropped whole:\n%s", kept)
	}
	// Every line of the error block survives, including the source excerpt and the
	// help note — the diagnostic is useless without them.
	for _, line := range []string{"unresolved link to", "b.rs:10:62", "no item named", "= help: escape it", "could not document"} {
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
