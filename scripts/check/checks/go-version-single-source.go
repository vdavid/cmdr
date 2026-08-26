package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

// `.mise.toml`'s `go` entry is the ONE place this repo names a Go toolchain
// version. Renovate bumps it on its weekly run, every CI job installs from it
// via `mise install`, and `desktop-rust-tests-linux.go` reads it to provision
// the Linux container. Nothing else may name a Go version.
//
// The bug this guards against: `desktop-rust-tests-linux.go` used to carry
// `const goVersion = "1.25.7"` with a comment saying it must match
// `.mise.toml`. Nothing enforced that, so a Renovate toolchain bump moved mise
// to 1.27 and left the container two minors behind, silently testing against a
// Go the repo no longer used.
//
// Three separate invariants, because "the Go version" means three different
// things depending on where you look:
//
//  1. No file outside `.mise.toml` names a toolchain version. That's `scanned`
//     below, and it's what makes the drift unrepresentable rather than merely
//     discouraged.
//  2. Every `go.mod` `go` directive agrees with every other. These are language
//     floors, not toolchain pins, so they legitimately lag mise, but 13 modules
//     disagreeing among themselves is drift with no upside. Compared as versions,
//     so `1.25` and `1.25.0` count as the same floor.
//  3. Every floor is <= mise's toolchain. A floor ABOVE the installed toolchain
//     is the one genuinely broken state: `GOTOOLCHAIN=auto` (the default) makes
//     the go command silently download and run a toolchain mise never pinned,
//     which is exactly the single-source property this check exists to defend.
//
// Note that (3) is one-directional on purpose. Raising mise only widens the
// headroom, so a Renovate toolchain bump can never turn this check red. Making
// the floors track mise exactly would do the opposite: it would break automerge
// on every Go bump, which is the failure mode that cost a CI-red afternoon on
// PR #52 when the pinned Go analyzers had to move in lockstep with a bump.
//
// Deliberately out of scope: `apps/desktop/test/e2e-linux/docker/Dockerfile.base`
// installs Debian's `golang-go` apt package. That container needs *a* Go to run
// the llama-server placeholder generator in `beforeBuildCommand`, and nothing
// about the placeholder is version-sensitive, so it stays on whatever apt ships.
// It names no version, so it doesn't trip (1). See checks/DETAILS.md.
//
// Opt-out for (1): append `# allowed-go-version-pin: <reason>` to the line.
// Empty reasons are rejected.
func RunGoVersionSingleSource(ctx *CheckContext) (CheckResult, error) {
	miseVersion, err := MiseGoVersion(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	files, err := repoFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to enumerate repo files: %w", err)
	}
	sort.Strings(files)

	var (
		violations []string
		orphans    []orphanDirective
		modFiles   []goModFloor
		scanned    int
	)
	for _, rel := range files {
		abs := filepath.Join(ctx.RootDir, filepath.FromSlash(rel))
		if filepath.Base(rel) == "go.mod" {
			floor, ok, err := readGoModFloor(abs, rel)
			if err != nil {
				return CheckResult{}, err
			}
			if ok {
				modFiles = append(modFiles, floor)
			}
			continue
		}
		if !goVersionScannable(rel) {
			continue
		}
		scanned++
		v, o, err := scanForGoVersionPins(abs, rel)
		if err != nil {
			return CheckResult{}, err
		}
		violations = append(violations, v...)
		orphans = append(orphans, o...)
	}

	var parts []string
	if len(violations) > 0 {
		parts = append(parts, fmt.Sprintf(
			"Go toolchain version named outside `.mise.toml`; read it with `MiseGoVersion(rootDir)` instead\n%s",
			indentOutput(strings.Join(violations, "\n"))))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives("# allowed-go-version-pin:", orphans))
	}
	if msg := checkGoModFloors(modFiles, miseVersion); msg != "" {
		parts = append(parts, msg)
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	floor := "none"
	if len(modFiles) > 0 {
		floor = modFiles[0].version
	}
	result := Success(fmt.Sprintf("Go %s from .mise.toml; %d %s scanned, %d go.mod on floor %s",
		miseVersion, scanned, Pluralize(scanned, "file", "files"), len(modFiles), floor))
	result.Total = scanned
	return result, nil
}

// miseGoRE matches `.mise.toml`'s `go = "1.27.0"` entry, tolerating either quote
// style and whitespace. Anchored to the line start so a `go` substring inside
// another key's value can't match.
var miseGoRE = regexp.MustCompile(`(?m)^\s*go\s*=\s*["']([^"']+)["']`)

// MiseGoVersion returns the Go toolchain version `.mise.toml` pins, the single
// source of truth for every Go version in this repo. Callers that need a Go
// version (the Linux container provisioner, this check) read it from here
// rather than repeating the literal, which is what keeps them from drifting.
func MiseGoVersion(rootDir string) (string, error) {
	path := filepath.Join(rootDir, ".mise.toml")
	data, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("failed to read .mise.toml: %w", err)
	}
	m := miseGoRE.FindSubmatch(data)
	if m == nil {
		return "", fmt.Errorf(".mise.toml has no `go = \"...\"` entry; it is the single source for the Go version")
	}
	return string(m[1]), nil
}

// goVersionNamedPinRE matches an identifier that reads as "go version" bound to
// a version-shaped literal, which covers every realistic way a pin gets
// reintroduced: a Go `const goVersion = "1.27.0"`, a workflow's
// `go-version: '1.27'`, a Dockerfile `ARG GO_VERSION=1.27.0`, and a shell
// `GO_VERSION=1.27`. Requiring the name AND the literal keeps the false
// positives near zero on a repo full of unrelated version strings.
var goVersionNamedPinRE = regexp.MustCompile(`(?i)\bgo[-_ ]?version\b\s*[:=]+\s*["']?v?\d+\.\d+`)

// goVersionArtifactRE matches a Go version baked into a download URL or a
// container image tag: `go1.27.0.linux-amd64.tar.gz` and `golang:1.27`. A
// format placeholder (`go%[1]s.linux-...`) deliberately does NOT match, since
// that's the shape a caller reading `MiseGoVersion` produces.
var goVersionArtifactRE = regexp.MustCompile(`\bgo\d+\.\d+(\.\d+)?\.(linux|darwin|windows)-|\bgolang:\d+\.\d+`)

// goVersionPinAllowedRE is the opt-out. Must carry a non-empty reason.
var goVersionPinAllowedRE = regexp.MustCompile(`#\s*allowed-go-version-pin:\s*(\S.*)`)

// goVersionScanExtensions are the file types a Go toolchain pin could plausibly
// hide in. Everything else in the tree (Rust, Svelte, images, locale JSON) is
// skipped so the scan stays cheap on a repo this size.
var goVersionScanExtensions = map[string]bool{
	".go": true, ".sh": true, ".yml": true, ".yaml": true,
	".ts": true, ".json": true, ".toml": true,
}

// goVersionScanExempt are paths that legitimately contain the patterns above.
// `.mise.toml` is the source of truth itself, and this check's test carries
// pin-shaped fixtures in string literals, which comment-skipping can't excuse.
// This check's own source needs no entry: its patterns live in doc comments and
// in regex literals that spell digits as `\d`, so neither self-matches.
var goVersionScanExempt = map[string]bool{
	".mise.toml": true,
	"scripts/check/checks/go-version-single-source_test.go": true,
}

// goVersionScannable says whether a repo-relative path takes part in the pin
// scan. Dockerfiles have no extension, so they're matched by name prefix.
func goVersionScannable(rel string) bool {
	if goVersionScanExempt[rel] {
		return false
	}
	if strings.HasPrefix(filepath.Base(rel), "Dockerfile") {
		return true
	}
	return goVersionScanExtensions[filepath.Ext(rel)]
}

// scanForGoVersionPins reports every line naming a Go toolchain version.
func scanForGoVersionPins(abs, rel string) ([]string, []orphanDirective, error) {
	data, err := os.ReadFile(abs)
	if err != nil {
		return nil, nil, fmt.Errorf("open %s: %w", rel, err)
	}

	var violations []string
	tracker := newDirectiveTracker("# allowed-go-version-pin:", "#")
	for i, line := range strings.Split(string(data), "\n") {
		lineNo := i + 1
		tracker.observe(lineNo, line)
		// A whole-line comment can name a version in prose ("the old
		// GO_VERSION=1.25.7 const lived here") without being a pin, and the
		// doc comments explaining this very check would trip it. The opt-out
		// only works as a TRAILING comment, so a pure-comment directive line
		// excuses nothing and surfaces as an orphan.
		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "#") || strings.HasPrefix(trimmed, "//") {
			continue
		}
		if !goVersionNamedPinRE.MatchString(line) && !goVersionArtifactRE.MatchString(line) {
			continue
		}
		if m := goVersionPinAllowedRE.FindStringSubmatch(line); m != nil && strings.TrimSpace(m[1]) != "" {
			tracker.markUsed(lineNo, line, "")
			continue
		}
		violations = append(violations, fmt.Sprintf(
			"%s:%d: %s\n    read the version from `MiseGoVersion(rootDir)`, OR add `# allowed-go-version-pin: <reason>`",
			rel, lineNo, strings.TrimSpace(line)))
	}
	return violations, tracker.orphans(rel), nil
}

// goModFloor is one module's declared language floor.
type goModFloor struct {
	relPath string
	version string
}

// goModDirectiveRE matches a `go.mod` `go` directive line. The `toolchain`
// directive is a different statement and doesn't match.
var goModDirectiveRE = regexp.MustCompile(`(?m)^go\s+(\d+\.\d+(?:\.\d+)?)\s*$`)

// readGoModFloor extracts the `go` directive. A module without one is not an
// error (the directive is optional), it just doesn't take part in the compare.
func readGoModFloor(abs, rel string) (goModFloor, bool, error) {
	data, err := os.ReadFile(abs)
	if err != nil {
		return goModFloor{}, false, fmt.Errorf("open %s: %w", rel, err)
	}
	m := goModDirectiveRE.FindSubmatch(data)
	if m == nil {
		return goModFloor{}, false, nil
	}
	return goModFloor{relPath: rel, version: string(m[1])}, true, nil
}

// checkGoModFloors enforces invariants (2) and (3): every module declares the
// same floor, and that floor doesn't exceed the pinned toolchain. Returns "" when
// both hold.
func checkGoModFloors(floors []goModFloor, miseVersion string) string {
	if len(floors) == 0 {
		return ""
	}

	// Group by VERSION, not by the literal text. `go mod tidy` canonicalizes
	// `go 1.25` to `go 1.25.0` on a module whose dependencies ask for it, so
	// demanding identical strings would put this check in a rewrite war with
	// Go's own tooling over two spellings of the same floor.
	byVersion := map[string][]string{}
	for _, f := range floors {
		key := f.version
		for existing := range byVersion {
			if compareGoVersions(existing, f.version) == 0 {
				key = existing
				break
			}
		}
		byVersion[key] = append(byVersion[key], f.relPath)
	}
	if len(byVersion) > 1 {
		versions := make([]string, 0, len(byVersion))
		for v := range byVersion {
			versions = append(versions, v)
		}
		sort.Slice(versions, func(i, j int) bool { return compareGoVersions(versions[i], versions[j]) < 0 })
		var sb strings.Builder
		for _, v := range versions {
			sb.WriteString(fmt.Sprintf("  go %s: %s\n", v, strings.Join(byVersion[v], ", ")))
		}
		return fmt.Sprintf(
			"`go.mod` files declare %d different `go` floors; every module shares one\n%s",
			len(byVersion), strings.TrimRight(sb.String(), "\n"))
	}

	floor := floors[0].version
	if compareGoVersions(floor, miseVersion) > 0 {
		return fmt.Sprintf(
			"`go.mod` floor `go %s` is above .mise.toml's `go = \"%s\"`\n"+
				"    A floor above the installed toolchain makes GOTOOLCHAIN=auto download a Go that mise never pinned.\n"+
				"    Raise .mise.toml first, or lower the floor.",
			floor, miseVersion)
	}
	return ""
}

// compareGoVersions orders two dotted Go versions numerically, returning -1, 0,
// or 1. A missing patch component counts as 0, so `1.25` == `1.25.0`. String
// comparison would get this wrong the moment a component reaches double digits
// ("1.9" > "1.27" lexically), which Go's minor version already has.
func compareGoVersions(a, b string) int {
	aParts, bParts := strings.Split(a, "."), strings.Split(b, ".")
	for i := 0; i < max(len(aParts), len(bParts)); i++ {
		av, bv := 0, 0
		if i < len(aParts) {
			av, _ = strconv.Atoi(aParts[i])
		}
		if i < len(bParts) {
			bv, _ = strconv.Atoi(bParts[i])
		}
		if av != bv {
			if av < bv {
				return -1
			}
			return 1
		}
	}
	return 0
}
