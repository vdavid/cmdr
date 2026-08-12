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
// `--document-private-items` because none of these crates are published. Rustdoc
// here is an internal artifact, so a public item's doc naming the internal it
// delegates to (`ArchiveIndex` → `EntryStore`) is good writing that should RESOLVE
// rather than render as plain text. It also puts private items' own docs under the
// same link gate.
//
// Every rustdoc lint the project holds itself to is denied, so nothing lands as a
// warning that a reader has to notice and act on. Anything the toolchain still
// warns about after that is unowned — a new lint class, or a doc problem outside
// this list — and the check fails on it rather than printing it into the void.

// rustdocDeniedLints is the contract: each of these is an `error`, never a
// warning. `unescaped_backticks` is deliberately absent — it's allow-by-default
// upstream and fires on ordinary prose like "the `2` in `x2`", which would cost
// more in false positives than it catches.
var rustdocDeniedLints = []string{
	"broken_intra_doc_links",
	"invalid_codeblock_attributes",
	"invalid_html_tags",
	"invalid_rust_codeblocks",
	"redundant_explicit_links",
	"bare_urls",
}

// rustdocAllowedLints is the one class we opt out of, explicitly rather than by
// dropping it from the output.
//
// `private_intra_doc_links` doesn't go away under `--document-private-items`, it
// changes meaning: the link now resolves, and the lint says it would break for
// someone building docs WITHOUT the flag. Nobody does — these crates are
// unpublished and this check is the only thing that documents them. Denying it
// would force ~75 sentences to either drop a link that helps the reader
// (`ArchiveIndex` explaining that its read handles live behind `EntryStore`) or
// make an internal type public to satisfy a lint. Both are worse than the doc we
// have. `-A` states that decision where the other lints are stated, instead of
// leaving a warning for every reader to re-litigate.
var rustdocAllowedLints = []string{"private_intra_doc_links"}

// RunRustdoc builds the workspace's documentation with every doc lint denied.
func RunRustdoc(ctx *CheckContext) (CheckResult, error) {
	members, err := WorkspaceMembers(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	targetOS := cargoOSName(runtime.GOOS)
	args := []string{"doc", "--no-deps", "--all-features", "--document-private-items", "--locked"}
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

	lintFlags := make([]string, 0, len(rustdocDeniedLints)+len(rustdocAllowedLints))
	for _, lint := range rustdocDeniedLints {
		lintFlags = append(lintFlags, "-D rustdoc::"+lint)
	}
	for _, lint := range rustdocAllowedLints {
		lintFlags = append(lintFlags, "-A rustdoc::"+lint)
	}

	cmd := exec.Command("cargo", args...)
	cmd.Dir = ctx.RootDir
	cmd.Env = append(cmd.Environ(), "RUSTDOCFLAGS="+strings.Join(lintFlags, " "))
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("cargo doc found doc-lint violations\n%s",
			indentOutput(rustdocFailureOutput(output)))
	}
	// A green run that still printed a warning means a lint nobody owns fired.
	// Surfacing it is the point: either it joins `rustdocDeniedLints` or the doc
	// gets fixed, and both need a human to see it.
	if warnings := rustdocDiagnostics(output); warnings != "" {
		return CheckResult{}, fmt.Errorf("cargo doc emitted warnings outside the denied lints\n%s",
			indentOutput(warnings))
	}
	return Success(fmt.Sprintf("%d %s documented, no doc-lint violations",
		documented, Pluralize(documented, "crate", "crates"))), nil
}

// diagnosticHeader matches the opening line of a rustc / rustdoc diagnostic: a
// column-zero `error` or `warning`, an optional `[E0124]` code, then a colon.
// Every continuation line (the `-->` locator, the source excerpt, `= note`,
// `= help`, and a trailing `help:` suggestion block) is what follows until the
// next header.
var diagnosticHeader = regexp.MustCompile(`^(error|warning)(\[[^\]]*\])?:`)

// cargoProgressLine matches cargo's own right-aligned status lines. They end the
// diagnostic above them: cargo prints one straight into (and out of) a diagnostic
// with no blank line, so without this a `Finished` rides along inside the block
// it interrupted. A diagnostic's own continuation lines can't collide, since
// every one of them opens with `-->`, `|`, `=`, a line number, or `help:`.
var cargoProgressLine = regexp.MustCompile(`^\s*(Documenting|Compiling|Checking|Building|Finished|Fresh|Generated|Downloading|Downloaded|Updating|Locking|Blocking)\s`)

// rustdocDiagnostics keeps every diagnostic block whole and drops everything
// else: cargo's `Documenting` / `Finished` progress lines, blank filler, and any
// other stream noise. Errors and warnings both survive, because both are things
// the check refuses to pass on.
//
// Structural, never length-based, and the structure is the LINE rather than the
// paragraph. Cargo runs its own `Documenting` progress line straight into the
// first diagnostic with no blank line between them, and rustdoc glues
// `error: could not document …` to the warning count above it. Splitting on blank
// lines therefore hands a diagnostic whatever preceded it, which drops the error
// whenever it opens or closes the stream: the exact case this check reports.
func rustdocDiagnostics(output string) string {
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
		switch {
		case diagnosticHeader.MatchString(line):
			flush()
			keeping = true
		case cargoProgressLine.MatchString(line):
			flush()
			keeping = false
		}
		if keeping {
			current = append(current, line)
		}
	}
	flush()
	return strings.Join(kept, "\n\n")
}

// rustdocFailureOutput is what a red run reports: the diagnostics if any parsed,
// otherwise the raw stream. A toolchain failure, a compile error with no
// diagnostic header, or an interrupt has nothing diagnostic-shaped in it, and
// swallowing that would report an empty reason for a red check.
func rustdocFailureOutput(output string) string {
	if diagnostics := rustdocDiagnostics(output); diagnostics != "" {
		return diagnostics
	}
	return output
}
