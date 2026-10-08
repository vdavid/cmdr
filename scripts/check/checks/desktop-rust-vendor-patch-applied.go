package checks

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// RunVendorPatchApplied fails unless every crate the root `Cargo.toml` swaps in
// through `[patch.crates-io] <name> = { path = ... }` actually resolves to that
// path in `Cargo.lock`.
//
// A `[patch]` applies only while the version a dependent asks for matches the
// patched copy's. A bump past it (Renovate would do it) makes cargo resolve
// crates.io again, park the patch under `[[patch.unused]]`, and say so in one
// build warning nobody reads, and the bug the fork exists to fix quietly ships
// again.
//
// The lockfile is the whole answer, so this reads it rather than asking cargo: a
// path package carries no `source`, a crates.io one does. Three ways it fails:
// the crate resolves from a registry (the patch is dead or shared with an
// unpatched copy), the patch sits in `[[patch.unused]]`, or nothing depends on
// the crate any more (the fork is dead weight and should go).
func RunVendorPatchApplied(ctx *CheckContext) (CheckResult, error) {
	patched, err := readPathPatches(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	if len(patched) == 0 {
		return Skipped("no `[patch.crates-io]` path entries to guard"), nil
	}

	var lock cargoLock
	if _, err := toml.DecodeFile(filepath.Join(ctx.RootDir, "Cargo.lock"), &lock); err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read Cargo.lock: %w", err)
	}

	names := make([]string, 0, len(patched))
	for name := range patched {
		names = append(names, name)
	}
	sort.Strings(names)

	var problems []string
	var applied []string
	for _, name := range names {
		problem, versions := judgePatch(name, patched[name], &lock)
		if problem != "" {
			problems = append(problems, problem)
		} else {
			applied = append(applied, fmt.Sprintf("%s %s", name, versions))
		}
	}

	if len(problems) > 0 {
		return CheckResult{}, fmt.Errorf("%s", indentOutput(strings.Join(problems, "\n")))
	}
	return Success(fmt.Sprintf("%s resolved from %s", strings.Join(applied, "; "),
		Pluralize(len(applied), "its vendored copy", "their vendored copies"))), nil
}

type lockedPackage struct {
	Name    string `toml:"name"`
	Version string `toml:"version"`
	Source  string `toml:"source"`
}

type cargoLock struct {
	Package []lockedPackage `toml:"package"`
	Patch   struct {
		Unused []lockedPackage `toml:"unused"`
	} `toml:"patch"`
}

// readPathPatches returns the root manifest's `[patch.crates-io]` entries that
// point at a path, by crate name.
func readPathPatches(rootDir string) (map[string]string, error) {
	var manifest struct {
		Patch map[string]map[string]struct {
			Path string `toml:"path"`
		} `toml:"patch"`
	}
	if _, err := toml.DecodeFile(filepath.Join(rootDir, "Cargo.toml"), &manifest); err != nil {
		return nil, fmt.Errorf("couldn't read the root Cargo.toml: %w", err)
	}
	patched := map[string]string{}
	for name, spec := range manifest.Patch["crates-io"] {
		if spec.Path != "" {
			patched[name] = spec.Path
		}
	}
	return patched, nil
}

// judgePatch says what's wrong with one patch, or "" plus the vendored versions
// when it applies.
func judgePatch(name, path string, lock *cargoLock) (problem, vendoredVersions string) {
	var vendored, fromRegistry []string
	for _, pkg := range lock.Package {
		switch {
		case pkg.Name != name:
		case pkg.Source == "":
			vendored = append(vendored, pkg.Version)
		default:
			fromRegistry = append(fromRegistry, pkg.Version)
		}
	}
	unused := false
	for _, pkg := range lock.Patch.Unused {
		unused = unused || pkg.Name == name
	}

	const remedy = "A dependent asked for a version the vendored copy isn't: pin it back to the vendored " +
		"version, or move the fork to the new one (or drop it, if upstream released the fix)"
	switch {
	case len(fromRegistry) > 0:
		return fmt.Sprintf("%s %s resolves from crates.io instead of %s, so the vendored fix doesn't ship. %s",
			name, strings.Join(fromRegistry, ", "), path, remedy), ""
	case unused:
		return fmt.Sprintf("cargo lists the %s patch to %s as unused, so the vendored fix doesn't ship. %s",
			name, path, remedy), ""
	case len(vendored) == 0:
		return fmt.Sprintf("nothing depends on %s any more, so the patch to %s guards nothing. Drop the fork: "+
			"delete %s, its `[patch.crates-io]` line, and its `exclude` entry", name, path, path), ""
	default:
		return "", strings.Join(vendored, ", ")
	}
}
