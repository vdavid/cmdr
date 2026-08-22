package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// A Docker-gated SMB cell in the app crate that CI won't run, made impossible to
// ship quietly.
//
// `desktop-rust-integration-tests` selects the app crate's Docker cells by NAME
// (`test(smb_integration_)`), because that's the only signal on this side of the
// crate boundary: `smb_soak_copy_loop` and the concurrency bench are `#[ignore]`d
// there too, and neither belongs in a gating lane. The cost of that design is
// silent: a cell named anything else compiles, passes review, sits in the tree
// looking like coverage, and never executes anywhere but by hand. One did, for its
// whole life, and it was the sole caller of the single public-surface widening the
// `cmdr-smb` extraction sanctioned — so a sanctioned widening rested on a test
// nothing ran.
//
// A comment saying "keep the prefix" is what was there before, and it's what this
// replaces. The gate itself is the evidence: an `#[ignore]` reason that names the
// Docker fixture says the cell needs the containers, and from there the lane either
// selects it or it doesn't.
//
// The `cmdr-smb` half of the filter needs no equivalent. It selects the whole
// package, so a new cell there is picked up whatever it's called, and an
// `#[ignore]`d test that ISN'T a Docker cell would run in the lane and go red
// rather than go quiet.

// smbLanePrefix is the name fragment the integration lane selects on.
const smbLanePrefix = "smb_integration_"

// AllowOutOfLaneSmbCellComment marks a gated cell that belongs OUTSIDE the lane (a
// soak loop, a measurement harness). It carries a reason after the colon, or it
// isn't an opt-out: a bare marker would be a way to silence this check without
// saying anything. An orphaned one fails, like every other opt-out here.
const AllowOutOfLaneSmbCellComment = "// allowed-out-of-lane-smb-cell:"

// smbFixtureMarkers are the substrings that identify an `#[ignore]` reason as
// naming the Docker SMB fixture. Both are INFRASTRUCTURE identifiers — the start
// script's path and the compose project's service prefix — rather than prose, so
// rewording a reason can't quietly move a cell out of this check's view.
var smbFixtureMarkers = []string{"smb-servers/start.sh", "smb-consumer"}

// smbCellFnPattern finds the test a gate applies to.
var smbCellFnPattern = regexp.MustCompile(`\bfn\s+([A-Za-z0-9_]+)`)

// outOfLaneSmbCell is one Docker-gated test the lane's filter won't select.
type outOfLaneSmbCell struct {
	// file is repo-relative for the app crate's `src`, so a finding reads as a
	// place to go.
	file string
	// line is where the test sits, 1-based: the line a rename edits. Falls back
	// to the gate's own line when there's no test under it.
	line int
	// name is the test function's name, or "" when the gate has no test under it.
	name string
}

func (c outOfLaneSmbCell) String() string {
	return fmt.Sprintf("%s:%d %s", c.file, c.line, c.name)
}

// smbLaneScan is one pass over the sources: the cells CI won't run, and the
// opt-outs that no longer excuse anything.
type smbLaneScan struct {
	stranded []outOfLaneSmbCell
	orphans  []orphanDirective
}

// scanSmbLaneCoverage reports every Docker-gated SMB test in `files` that
// `test(smb_integration_)` won't select, in file then line order, plus every
// orphaned opt-out.
//
// `files` maps a path to its contents. The path matters: nextest matches the
// prefix anywhere in a test's path, module segments included, so a cell inside a
// module that carries it is already selected and isn't a finding.
func scanSmbLaneCoverage(files map[string]string) smbLaneScan {
	var scan smbLaneScan
	for _, path := range sortedKeys(files) {
		tracker := newDirectiveTracker(AllowOutOfLaneSmbCellComment, "//")
		lines := strings.Split(files[path], "\n")
		inModuleThatCarriesThePrefix := strings.Contains(filepath.ToSlash(path), smbLanePrefix)
		for i, line := range lines {
			tracker.observe(i+1, line)
			trimmed := strings.TrimSpace(line)
			if !strings.HasPrefix(trimmed, "#[ignore") || !namesTheSmbFixture(trimmed) {
				continue
			}
			name, at := testUnderGate(lines, i)
			// Whether the lane selects it is decided FIRST, so an opt-out on a cell
			// that IS selected stays unmarked and gets reported as the orphan it is.
			if inModuleThatCarriesThePrefix || strings.Contains(name, smbLanePrefix) {
				continue
			}
			if optOut, found := smbLaneOptOutAbove(lines, i); found {
				tracker.markLineUsed(optOut + 1)
				continue
			}
			scan.stranded = append(scan.stranded, outOfLaneSmbCell{file: path, line: at + 1, name: name})
		}
		scan.orphans = append(scan.orphans, tracker.orphans(path)...)
	}
	return scan
}

func namesTheSmbFixture(reason string) bool {
	for _, marker := range smbFixtureMarkers {
		if strings.Contains(reason, marker) {
			return true
		}
	}
	return false
}

// smbLaneOptOutAbove finds the reasoned opt-out for the gate at `at`, looked for
// on the gate's own line and the three above it, where the other attributes and
// the comment block sit. Returns the opt-out's 0-based line.
func smbLaneOptOutAbove(lines []string, at int) (int, bool) {
	for i := max(0, at-3); i <= at; i++ {
		marker := strings.Index(lines[i], AllowOutOfLaneSmbCellComment)
		if marker < 0 {
			continue
		}
		if strings.TrimSpace(lines[i][marker+len(AllowOutOfLaneSmbCellComment):]) != "" {
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
		if match := smbCellFnPattern.FindStringSubmatch(lines[i]); match != nil {
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

// countGatedSmbCells is how many Docker-gated cells the app crate holds, for the
// success line: the number this check is standing over.
func countGatedSmbCells(files map[string]string) int {
	count := 0
	for _, contents := range files {
		for _, line := range strings.Split(contents, "\n") {
			trimmed := strings.TrimSpace(line)
			if strings.HasPrefix(trimmed, "#[ignore") && namesTheSmbFixture(trimmed) {
				count++
			}
		}
	}
	return count
}

// RunSmbLaneCoverage fails when a Docker-gated SMB cell in the app crate sits
// outside what the integration lane runs.
func RunSmbLaneCoverage(ctx *CheckContext) (CheckResult, error) {
	if !strings.Contains(smbIntegrationFilter, smbLanePrefix) {
		return CheckResult{}, fmt.Errorf(
			"the integration lane filters on `%s`, which no longer carries `%s`; re-key this check to match it",
			smbIntegrationFilter, smbLanePrefix)
	}

	roots, err := ScannerRoots(ctx.RootDir, "desktop-smb-lane-coverage")
	if err != nil {
		return CheckResult{}, err
	}
	files, err := scanRustSources(ctx.RootDir, roots)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read the app crate's sources: %w", err)
	}

	scan := scanSmbLaneCoverage(files)
	if len(scan.stranded) > 0 {
		lines := make([]string, len(scan.stranded))
		for i, cell := range scan.stranded {
			lines[i] = cell.String()
		}
		sort.Strings(lines)
		return CheckResult{}, fmt.Errorf(
			"%d Docker-gated SMB %s the integration lane won't run:\n  %s\n\n"+
				"`desktop-rust-integration-tests` selects the app crate's cells with `%s`, so rename each one to `%s…`. "+
				"A cell that belongs outside the lane says so with an `%s <why>` comment above its `#[ignore]`",
			len(scan.stranded), Pluralize(len(scan.stranded), "cell", "cells"), strings.Join(lines, "\n  "),
			smbIntegrationFilter, smbLanePrefix, AllowOutOfLaneSmbCellComment)
	}
	if len(scan.orphans) > 0 {
		return CheckResult{}, fmt.Errorf("%s", formatOrphanDirectives(AllowOutOfLaneSmbCellComment, scan.orphans))
	}

	gated := countGatedSmbCells(files)
	return Success(fmt.Sprintf("%s Docker-gated SMB %s in the app crate, all reachable by `%s`",
		formatThousands(gated), Pluralize(gated, "cell", "cells"), smbLanePrefix)), nil
}
