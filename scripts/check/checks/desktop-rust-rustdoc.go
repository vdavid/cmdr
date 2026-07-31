package checks

import (
	"fmt"
	"os/exec"
	"regexp"
	"runtime"
	"strings"
)

// The index crate's docs are a deliverable, so they need a gate.
//
// `#![deny(missing_docs)]` is a rustc lint and fires under clippy, so "is it
// documented?" was already covered. "Does the documentation point anywhere?" was
// not: a `[`Foo`]` intra-doc link that resolves to nothing is invisible to every
// other lane, because only `cargo doc` resolves them. Splitting the index into its
// own crate broke 27 of them at once — every link naming an item the surface audit
// had narrowed to `pub(crate)`, plus a couple pointing at app modules the crate
// never had.
//
// `--all-features` on purpose: the `testing` and `tooling` surfaces are exactly
// where a link rots unnoticed, since nothing else compiles them by default.
//
// Only `broken_intra_doc_links` is denied. `private_intra_doc_links` stays a
// warning: a public module's doc explaining the internal it delegates to is good
// writing, and rustdoc rendering it as plain text is the right outcome, not a
// failure.

// RunRustdoc builds the workspace's documentation with broken intra-doc links
// denied.
func RunRustdoc(ctx *CheckContext) (CheckResult, error) {
	members, err := WorkspaceMembers(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	targetOS := cargoOSName(runtime.GOOS)
	args := []string{"doc", "--no-deps", "--all-features", "--locked"}
	documented := 0
	for _, m := range members {
		// A vendored fork's docs aren't ours to hold to this, and `--all-features`
		// doesn't even build one: `cmdr-fsevent-stream` carries mutually exclusive
		// `tokio` / `async-std` feature arms, so turning both on fails to compile.
		if m.Kind == KindVendored || !m.BuildsOn(targetOS) {
			continue
		}
		args = append(args, "-p", m.Name)
		documented++
	}
	if documented == 0 {
		return Skipped("no first-party members build on this platform"), nil
	}

	cmd := exec.Command("cargo", args...)
	cmd.Dir = ctx.RootDir
	cmd.Env = append(cmd.Environ(), "RUSTDOCFLAGS=-D rustdoc::broken_intra_doc_links")
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("cargo doc found broken intra-doc links\n%s",
			indentOutput(rustdocErrorDiagnostics(output)))
	}
	return Success(fmt.Sprintf("%d %s documented, no broken intra-doc links",
		documented, Pluralize(documented, "crate", "crates"))), nil
}

// diagnosticHeader matches the opening line of a rustc / rustdoc diagnostic: a
// column-zero `error` or `warning`, an optional `[E0124]` code, then a colon.
// Every continuation line (the `-->` locator, the source excerpt, `= note`,
// `= help`, and a trailing `help:` suggestion block) is what follows until the
// next header.
var diagnosticHeader = regexp.MustCompile(`^(error|warning)(\[[^\]]*\])?:`)

// rustdocErrorDiagnostics keeps the `error:` diagnostics and drops the `warning:`
// ones, whole diagnostics at a time.
//
// It matters because the warnings are the LOUD half: a public module doc naming
// the internal it delegates to is `private_intra_doc_links`, which is good
// writing, and ~70 of them bury the one broken link the check exists to show.
//
// Structural, never length-based, and the structure is the LINE rather than the
// paragraph. Cargo runs its own `Documenting` progress line straight into the
// first diagnostic with no blank line between them, and rustdoc glues
// `error: could not document …` to the warning count above it. Splitting on blank
// lines therefore hands a diagnostic whatever preceded it, which drops the error
// whenever it opens or closes the stream — the exact case this check reports.
func rustdocErrorDiagnostics(output string) string {
	var kept []string
	var current []string
	keeping := false
	flush := func() {
		if keeping && len(current) > 0 {
			kept = append(kept, strings.TrimRight(strings.Join(current, "\n"), "\n"))
		}
		current = nil
	}
	for _, line := range strings.Split(output, "\n") {
		if match := diagnosticHeader.FindStringSubmatch(line); match != nil {
			flush()
			keeping = match[1] == "error"
		}
		if keeping {
			current = append(current, line)
		}
	}
	flush()
	if len(kept) == 0 {
		// Something failed that isn't a diagnostic we recognize (a compile error, a
		// missing toolchain). Never swallow it.
		return output
	}
	return strings.Join(kept, "\n\n")
}
