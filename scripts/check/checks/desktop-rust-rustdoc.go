package checks

import (
	"fmt"
	"os/exec"
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

// rustdocErrorDiagnostics keeps the `error:` diagnostics and drops the `warning:`
// ones, whole blocks at a time.
//
// Structural, never length-based: rustdoc prints one blank-line-separated block per
// diagnostic, so this keeps every line of every error and none of any warning. It
// matters because the warnings are the LOUD half — a public module doc naming the
// internal it delegates to is `private_intra_doc_links`, which is good writing, and
// ~70 of them would bury the one broken link the check exists to show.
func rustdocErrorDiagnostics(output string) string {
	blocks := strings.Split(output, "\n\n")
	var kept []string
	for _, block := range blocks {
		trimmed := strings.TrimSpace(block)
		if trimmed == "" {
			continue
		}
		if strings.HasPrefix(trimmed, "error") {
			kept = append(kept, strings.TrimRight(block, "\n"))
		}
	}
	if len(kept) == 0 {
		// Something failed that isn't a diagnostic we recognize (a compile error, a
		// missing toolchain). Never swallow it.
		return output
	}
	return strings.Join(kept, "\n\n")
}
