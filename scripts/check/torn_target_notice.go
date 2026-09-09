package main

import (
	"fmt"
	"sort"
	"strings"

	"cmdr/scripts/check/checks"
)

// The inconsistent-`target/` notice.
//
// `target/` can end up holding fingerprints from one moment and `.rlib`s from
// another. Cargo then trusts a fingerprint whose artefact no longer matches, and
// rustc reports that it cannot find, or cannot reconcile, a crate nobody touched.
// The usual cause is a fresh worktree: `cp -Rc` walks `target/` for ~80 s, and
// anything compiling in the main clone during that walk lands in the copy twice,
// at two different versions. An interrupted build, a killed `cargo-sweep`, or a
// disk-full write produce the same shape.
//
// The failure is loud and accuses the wrong suspect. It arrives as a wall of
// compiler output right after whatever the author last edited, so the obvious
// next move is to revert a change that was fine. That cost a session ~15 minutes
// of diagnosis on 2026-09-09 and nearly cost a correct dependency change.
//
// So the run ends by naming it. Recovery is a profile-scoped `cargo clean`, which
// is safe by construction here: `target/` is regenerable, which is the whole
// reason cloning it was reasonable in the first place.
//
// ❌ Not auto-recovered on purpose. Lanes run concurrently, so cleaning mid-run
// would pull `target/` out from under the lanes still building; and cleaning
// after the run means silently spending a cold rebuild (12 minutes, measured) on
// a run the reader thinks is finishing. Naming the cause is the whole win.

// registryMarker is the path segment every vendored dependency's source sits
// under. It is the discriminator for this notice: a crate rustc cannot reconcile
// UNDER the registry was built from source the author never edited, so their
// change cannot be the cause. The same error against a path in the checkout is an
// ordinary mistake and must stay reported as one.
const registryMarker = "/registry/src/"

// unreconcilableCrateCodes are the two rustc codes that mean "the artefact I was
// handed is not the one this fingerprint promised": E0463 can't find the crate at
// all, E0460 found one that disagrees with what a dependent was built against.
//
// Matched by CODE, never by sentence: a rustc error code is a stable identifier
// (the errno of the compiler), while the prose around it is free to be reworded
// upstream at any time. Same reasoning as the repo's error-string-match rule.
var unreconcilableCrateCodes = []string{"error[E0460]", "error[E0463]"}

// detectTornTarget reports the registry packages implicated in a lane's failure,
// sorted and deduped, or nil when the failure is anything else.
//
// Both halves are required. The error code alone fires on an author's own missing
// crate; a registry path alone appears in ordinary output all the time (a
// deprecation warning pointing into a dependency, a backtrace frame).
func detectTornTarget(output string) []string {
	hasCode := false
	for _, code := range unreconcilableCrateCodes {
		if strings.Contains(output, code) {
			hasCode = true
			break
		}
	}
	if !hasCode {
		return nil
	}

	seen := map[string]bool{}
	for _, line := range strings.Split(output, "\n") {
		if pkg := registryPackage(line); pkg != "" {
			seen[pkg] = true
		}
	}
	if len(seen) == 0 {
		return nil
	}

	pkgs := make([]string, 0, len(seen))
	for pkg := range seen {
		pkgs = append(pkgs, pkg)
	}
	sort.Strings(pkgs)
	return pkgs
}

// registryPackage pulls the `<name>-<version>` directory out of a registry path,
// or "" when the line holds none. The layout is
// `…/registry/src/<index host>/<name>-<version>/…`, so the package is the segment
// after the index host. Read from the PATH rather than the message text: the path
// shape is cargo's on-disk layout, which the wording is not.
func registryPackage(line string) string {
	idx := strings.Index(line, registryMarker)
	if idx < 0 {
		return ""
	}
	rest := strings.Split(line[idx+len(registryMarker):], "/")
	if len(rest) < 2 {
		return ""
	}
	return rest[1]
}

// maxListedPackages caps the notice's package list. A long list pushes the one
// line that matters, the recovery command, off the reader's screen.
const maxListedPackages = 5

// formatTornTargetNotice renders the closing warning. inWorktree adds the cause
// that explains nearly every real instance; in the main clone that sentence would
// send the reader looking for a clone that never happened.
func formatTornTargetNotice(pkgs []string, inWorktree bool) string {
	listed := pkgs
	suffix := ""
	if len(pkgs) > maxListedPackages {
		listed = pkgs[:maxListedPackages]
		suffix = fmt.Sprintf(", and %d more", len(pkgs)-maxListedPackages)
	}

	var sb strings.Builder
	fmt.Fprintf(&sb, "%s🧊 Rust couldn't reconcile %d prebuilt %s in target/: %s%s.%s\n",
		colorYellow, len(pkgs), checks.Pluralize(len(pkgs), "crate", "crates"),
		strings.Join(listed, ", "), suffix, colorReset)
	fmt.Fprintf(&sb, "     Every one of them is a third-party crate, so this is not from your change: the build cache itself is inconsistent.\n")
	fmt.Fprintf(&sb, "     Recover with: %scargo clean --profile dev%s, then re-run.\n", colorYellow, colorReset)
	if inWorktree {
		fmt.Fprintf(&sb, "     A worktree's target/ lands this way when the clone runs while the main clone is building. See worktree-warm-start.md.\n")
	}
	return sb.String()
}

// printTornTargetNotice prints the notice when any failed lane carries the
// signature. It goes last, after the summary, because the summary is where a
// reader stops.
func printTornTargetNotice(states []*CheckState, rootDir string) {
	seen := map[string]bool{}
	for _, state := range states {
		if state.Status != StatusFailed || state.Error == nil {
			continue
		}
		for _, pkg := range detectTornTarget(state.Error.Error()) {
			seen[pkg] = true
		}
	}
	if len(seen) == 0 {
		return
	}

	pkgs := make([]string, 0, len(seen))
	for pkg := range seen {
		pkgs = append(pkgs, pkg)
	}
	sort.Strings(pkgs)
	fmt.Print(formatTornTargetNotice(pkgs, !isMainWorkingTree(rootDir)))
}
