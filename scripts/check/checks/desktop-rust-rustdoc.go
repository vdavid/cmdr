package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
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

// rustdocTargetDir is this lane's OWN cargo build directory, nested inside the
// shared one so `cargo clean` and `rm -rf target` still take it with them.
func rustdocTargetDir(rootDir string) string {
	return filepath.Join(cargoTargetDir(rootDir), "rustdoc")
}

// rustdocEnv is the environment the doc build runs under: the lint contract,
// plus a private build directory when running locally.
//
// Locally the private directory wins twice, measured on an M3 Max (2026-08-12,
// one sample each, `/usr/bin/time -l` around `pnpm check rustdoc`):
//
//  1. Cargo's build-directory lock is exclusive for a whole command, so sharing
//     `target/` serialized this lane against `clippy` and the test lanes. It
//     burns only ~1.7 cores, so it fits beside them once it stops queueing.
//  2. Warm runs dropped 27.5 s → 17.0 s. Living beside the other lanes meant
//     their fingerprint churn kept invalidating doc units already built here.
//
// It's affordable because `cargo doc` builds dependencies metadata-only, with no
// codegen: 2.0 GB against an 82 GB `target/`, and a 75 s cold build after a
// `Cargo.lock` bump.
//
// ❌ Never extend it to CI. There it's all cost and no benefit: the workflow runs
// each check as its own sequential step, so no two cargo commands overlap and
// there's no lock to dodge. Meanwhile the runner ships ~14 GB free and has
// already hit "No space left on device" linking `libcmdr_lib.a` (see ci.yml's
// "Free disk space" step), and a second directory would push `rust-cache` toward
// GitHub's 10 GB ceiling for artifacts a fresh runner can't reuse anyway.
func rustdocEnv(base []string, rootDir, rustdocFlags string, ci bool) []string {
	env := append(base, "RUSTDOCFLAGS="+rustdocFlags)
	if ci {
		return env
	}
	return append(env, "CARGO_TARGET_DIR="+rustdocTargetDir(rootDir))
}

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
	cmd.Env = rustdocEnv(cmd.Environ(), ctx.RootDir, strings.Join(lintFlags, " "), ctx.CI)
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
	diagnostics := rustdocDiagnostics(output)
	if diagnostics == "" {
		return output
	}
	if hint := mergedFragmentHint(diagnostics); hint != "" {
		return diagnostics + "\n\n" + hint
	}
	return diagnostics
}

// unresolvedLinkHeader matches the opening line of the one diagnostic this hint
// is about.
var unresolvedLinkHeader = regexp.MustCompile(`^error: unresolved link to `)

// mergedFragmentHint explains a spanless unresolved link, and stays quiet
// otherwise.
//
// A missing `-->` locator is rustdoc's signature for merged doc fragments: an
// outer `///` on a `mod foo;` declaration concatenates with `foo`'s own `//!`
// header, and rustdoc then resolves the WHOLE merged doc in the parent's scope.
// Every link the child file wrote against its own items stops resolving, and
// because the fragments came from two files rustdoc can't point at either one.
// So the reader gets "no item named X in scope" naming an item sitting a few
// lines below the link, with nothing to go on.
//
// Left unexplained it costs a real debugging session, which is why the hint
// lives here rather than in a doc: the check is where someone meets the problem.
func mergedFragmentHint(diagnostics string) string {
	located := true
	for _, line := range strings.Split(diagnostics, "\n") {
		switch {
		case unresolvedLinkHeader.MatchString(line):
			located = false
		case diagnosticHeader.MatchString(line):
			located = true
		case !located && strings.HasPrefix(strings.TrimSpace(line), "-->"):
			located = true
		}
		if !located && strings.Contains(line, "no item named") {
			return "hint: an unresolved link with no `-->` location usually means merged doc fragments.\n" +
				"      An outer `///` on a `mod foo;` declaration concatenates with `foo`'s own `//!`\n" +
				"      header, and the merged doc resolves in the PARENT's scope, so links the child\n" +
				"      wrote against its own items stop resolving. Drop the outer comment (the file's\n" +
				"      `//!` header already documents the module), or make the link absolute\n" +
				"      (`[`crate::a::b::thing`]`), which is immune either way."
		}
	}
	return ""
}
