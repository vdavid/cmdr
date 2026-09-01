package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// A Docker-gated fixture cell in the app crate that CI won't run, made
// impossible to ship quietly.
//
// `desktop-rust-integration-tests` selects the app crate's Docker cells by NAME,
// because that's the only signal on this side of the crate boundary:
// `smb_soak_copy_loop` and the concurrency bench are `#[ignore]`d there too, and
// neither belongs in a gating lane. The cost of that design is silent: a cell
// named anything else compiles, passes review, sits in the tree looking like
// coverage, and never executes anywhere but by hand. One did, for its whole
// life, and it was the sole caller of the single public-surface widening the
// `cmdr-smb` extraction sanctioned — so a sanctioned widening rested on a test
// nothing ran.
//
// A comment saying "keep the prefix" is what was there before, and it's what
// this replaces. The gate itself is the evidence: an `#[ignore]` reason that
// names a Docker fixture says the cell needs those containers, and from there
// the lane either selects it or it doesn't.
//
// The backend crates need no equivalent. The lane selects each by package, so a
// new cell there is picked up whatever it's called, and an `#[ignore]`d test
// that ISN'T a Docker cell would run in the lane and go red rather than go
// quiet.

// laneFixture is one Docker fixture the integration lane serves: how an
// `#[ignore]` reason names it, the test-name prefix the lane selects its
// app-crate cells by, and the backend crate whose whole ignored surface is
// Docker cells.
type laneFixture struct {
	// name is how a finding refers to the fixture.
	name string
	// lanePrefix is the name fragment the lane selects this fixture's app-crate
	// cells on.
	lanePrefix string
	// markers identify an `#[ignore]` reason as naming this fixture. They're
	// INFRASTRUCTURE identifiers — the start script's path and the compose
	// project's service prefix — rather than prose, so rewording a reason can't
	// quietly move a cell out of this check's view.
	markers []string
	// backendPackage is the crate where every `#[ignore]`d test is a Docker cell
	// by construction: there's no other reason to ignore one in a crate with no
	// app around it. `package`, not `binary`: nextest matches a lib test target
	// by binary id, which is not the crate name.
	backendPackage string
}

// laneFixtures are the Docker fixtures the integration lane covers.
var laneFixtures = []laneFixture{
	{
		name:           "SMB",
		lanePrefix:     "smb_integration_",
		markers:        []string{"smb-servers/start.sh", "smb-consumer"},
		backendPackage: "cmdr-smb",
	},
	{
		name:           "SFTP",
		lanePrefix:     "sftp_integration_",
		markers:        []string{"sftp-servers/start.sh", "sftp-fixture"},
		backendPackage: "cmdr-sftp",
	},
	{
		name:           "WebDAV",
		lanePrefix:     "webdav_integration_",
		markers:        []string{"webdav-servers/start.sh", "webdav-fixture"},
		backendPackage: "cmdr-webdav",
	},
}

// AllowOutOfLaneFixtureCellComment marks a gated cell that belongs OUTSIDE the
// lane (a soak loop, a measurement harness). It carries a reason after the
// colon, or it isn't an opt-out: a bare marker would be a way to silence this
// check without saying anything. An orphaned one fails, like every other opt-out
// here.
const AllowOutOfLaneFixtureCellComment = "// allowed-out-of-lane-fixture-cell:"

// fixtureIntegrationFilter builds the nextest expression that selects every
// Docker-backed fixture cell and nothing else.
//
// Two halves, because the suites live on two sides of a crate boundary. In the
// APP crate the name prefix is the only signal, and it has to stay one, which
// `desktop-fixture-lane-coverage` enforces. In a backend crate the whole package
// qualifies, so a cell there can be named for what it asserts.
//
// A backend crate's clause is included only once the crate exists: nextest fails
// to PARSE `package(x)` for an unknown package, so naming one ahead of its crate
// takes the whole lane down rather than selecting nothing. rootDir "" skips the
// package half entirely, which is what the prefix-contract test wants.
func fixtureIntegrationFilter(rootDir string) string {
	clauses := make([]string, 0, 2*len(laneFixtures))
	for _, f := range laneFixtures {
		clauses = append(clauses, "test("+f.lanePrefix+")")
	}
	for _, f := range laneFixtures {
		if rootDir != "" && backendPackageExists(rootDir, f.backendPackage) {
			clauses = append(clauses, "package("+f.backendPackage+")")
		}
	}
	return strings.Join(clauses, " + ")
}

// backendPackageExists reports whether a workspace member of that name is on
// disk. A crate directory with a manifest is what makes `package(x)` parse.
func backendPackageExists(rootDir, name string) bool {
	_, err := os.Stat(filepath.Join(rootDir, "crates", name, "Cargo.toml"))
	return err == nil
}

// fixtureCellFnPattern finds the test a gate applies to.
var fixtureCellFnPattern = regexp.MustCompile(`\bfn\s+([A-Za-z0-9_]+)`)

// outOfLaneCell is one Docker-gated test the lane's filter won't select.
type outOfLaneCell struct {
	// file is repo-relative for the app crate's `src`, so a finding reads as a
	// place to go.
	file string
	// line is where the test sits, 1-based: the line a rename edits. Falls back
	// to the gate's own line when there's no test under it.
	line int
	// name is the test function's name, or "" when the gate has no test under it.
	name string
	// fixture is the one the gate names, which is what says the prefix a fix uses.
	fixture laneFixture
}

func (c outOfLaneCell) String() string {
	return fmt.Sprintf("%s:%d %s", c.file, c.line, c.name)
}

// fixtureLaneScan is one pass over the sources: the cells CI won't run, and the
// opt-outs that no longer excuse anything.
type fixtureLaneScan struct {
	stranded []outOfLaneCell
	orphans  []orphanDirective
}

// scanFixtureLaneCoverage reports every Docker-gated fixture test in `files` the
// lane's filter won't select, in file then line order, plus every orphaned
// opt-out.
//
// `files` maps a path to its contents. The path matters: nextest matches the
// prefix anywhere in a test's path, module segments included, so a cell inside a
// module that carries its fixture's prefix is already selected and isn't a
// finding.
func scanFixtureLaneCoverage(files map[string]string) fixtureLaneScan {
	var scan fixtureLaneScan
	for _, path := range sortedKeys(files) {
		tracker := newDirectiveTracker(AllowOutOfLaneFixtureCellComment, "//")
		lines := strings.Split(files[path], "\n")
		slashPath := filepath.ToSlash(path)
		for i, line := range lines {
			tracker.observe(i+1, line)
			trimmed := strings.TrimSpace(line)
			if !strings.HasPrefix(trimmed, "#[ignore") {
				continue
			}
			fixture, named := fixtureNamedBy(trimmed)
			if !named {
				continue
			}
			name, at := testUnderGate(lines, i)
			// Whether the lane selects it is decided FIRST, so an opt-out on a cell
			// that IS selected stays unmarked and gets reported as the orphan it is.
			if strings.Contains(slashPath, fixture.lanePrefix) || strings.Contains(name, fixture.lanePrefix) {
				continue
			}
			if optOut, found := fixtureLaneOptOutAbove(lines, i); found {
				tracker.markLineUsed(optOut + 1)
				continue
			}
			scan.stranded = append(scan.stranded, outOfLaneCell{file: path, line: at + 1, name: name, fixture: fixture})
		}
		scan.orphans = append(scan.orphans, tracker.orphans(path)...)
	}
	return scan
}

// fixtureNamedBy says which fixture an `#[ignore]` reason names, if any.
func fixtureNamedBy(reason string) (laneFixture, bool) {
	for _, fixture := range laneFixtures {
		for _, marker := range fixture.markers {
			if strings.Contains(reason, marker) {
				return fixture, true
			}
		}
	}
	return laneFixture{}, false
}

// fixtureLaneOptOutAbove finds the reasoned opt-out for the gate at `at`, looked
// for on the gate's own line and the three above it, where the other attributes
// and the comment block sit. Returns the opt-out's 0-based line.
func fixtureLaneOptOutAbove(lines []string, at int) (int, bool) {
	for i := max(0, at-3); i <= at; i++ {
		marker := strings.Index(lines[i], AllowOutOfLaneFixtureCellComment)
		if marker < 0 {
			continue
		}
		if strings.TrimSpace(lines[i][marker+len(AllowOutOfLaneFixtureCellComment):]) != "" {
			return i, true
		}
	}
	return 0, false
}

// testUnderGate names the first test below the gate and says which line it's on,
// skipping the attributes and comments between them. An empty name means there's
// no test under the gate at all, which reads as a finding of its own rather than
// a silent pass.
func testUnderGate(lines []string, gate int) (string, int) {
	for i := gate + 1; i < len(lines) && i <= gate+10; i++ {
		if match := fixtureCellFnPattern.FindStringSubmatch(lines[i]); match != nil {
			return match[1], i
		}
	}
	return "", gate
}

// scanRustSources reads every `.rs` file under `roots`, keyed repo-relative so a
// finding reads as a place to go.
func scanRustSources(rootDir string, roots []string) (map[string]string, error) {
	files := map[string]string{}
	for _, root := range roots {
		err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
			if err != nil || info.IsDir() || !strings.HasSuffix(path, ".rs") {
				return err
			}
			data, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			relative, relErr := filepath.Rel(rootDir, path)
			if relErr != nil {
				return relErr
			}
			files[filepath.ToSlash(relative)] = string(data)
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	return files, nil
}

// countGatedFixtureCells is how many Docker-gated cells the app crate holds, for
// the success line: the number this check is standing over.
func countGatedFixtureCells(files map[string]string) int {
	count := 0
	for _, contents := range files {
		for _, line := range strings.Split(contents, "\n") {
			trimmed := strings.TrimSpace(line)
			if !strings.HasPrefix(trimmed, "#[ignore") {
				continue
			}
			if _, named := fixtureNamedBy(trimmed); named {
				count++
			}
		}
	}
	return count
}

// RunFixtureLaneCoverage fails when a Docker-gated fixture cell in the app crate
// sits outside what the integration lane runs.
func RunFixtureLaneCoverage(ctx *CheckContext) (CheckResult, error) {
	filter := fixtureIntegrationFilter(ctx.RootDir)
	for _, fixture := range laneFixtures {
		if !strings.Contains(filter, fixture.lanePrefix) {
			return CheckResult{}, fmt.Errorf(
				"the integration lane filters on `%s`, which no longer carries `%s`; re-key this check to match it",
				filter, fixture.lanePrefix)
		}
	}

	roots, err := ScannerRoots(ctx.RootDir, "desktop-fixture-lane-coverage")
	if err != nil {
		return CheckResult{}, err
	}
	files, err := scanRustSources(ctx.RootDir, roots)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read the app crate's sources: %w", err)
	}

	scan := scanFixtureLaneCoverage(files)
	if len(scan.stranded) > 0 {
		lines := make([]string, len(scan.stranded))
		for i, cell := range scan.stranded {
			lines[i] = fmt.Sprintf("%s → rename to `%s…`", cell, cell.fixture.lanePrefix)
		}
		sort.Strings(lines)
		return CheckResult{}, fmt.Errorf(
			"%d Docker-gated %s the integration lane won't run:\n  %s\n\n"+
				"`desktop-rust-integration-tests` selects the app crate's cells with `%s`. "+
				"A cell that belongs outside the lane says so with an `%s <why>` comment above its `#[ignore]`",
			len(scan.stranded), Pluralize(len(scan.stranded), "cell", "cells"), strings.Join(lines, "\n  "),
			filter, AllowOutOfLaneFixtureCellComment)
	}
	if len(scan.orphans) > 0 {
		return CheckResult{}, fmt.Errorf("%s", formatOrphanDirectives(AllowOutOfLaneFixtureCellComment, scan.orphans))
	}

	gated := countGatedFixtureCells(files)
	return Success(fmt.Sprintf("%s Docker-gated fixture %s in the app crate, all reachable by the lane's filter",
		formatThousands(gated), Pluralize(gated, "cell", "cells"))), nil
}
