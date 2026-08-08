package checks

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// AllowTestSleepComment opts a single sleep site out of the test-sleep check.
// Place it on the line immediately above the flagged line, or as a trailing
// comment on the flagged line, with a reason that says WHY the sleep is the
// subject rather than a guess (fake latency in a stub, a canceller thread whose
// delay IS the scenario, a negative assertion over a window, a debounce / TTL /
// throttle window the test exists to exercise).
//
//	// allowed-test-sleep: fake USB latency; the delay is the thing under test
//	std::thread::sleep(Duration::from_millis(5));
const AllowTestSleepComment = "// allowed-test-sleep:"

// testSleepRegex matches a blocking sleep call in its qualified forms. The tree
// uses only `std::thread::sleep(`, `thread::sleep(`, and `tokio::time::sleep(`
// (no bare `sleep(` imports), so anchoring on the `::sleep(` tail avoids matching
// an unrelated local named `sleep`.
var testSleepRegex = regexp.MustCompile(`\b(?:std::)?(?:thread|tokio::time|time)::sleep\s*\(`)

type testSleepSite struct {
	relPath string
	line    int
	text    string
}

// RunTestSleep fails the build if any Rust TEST code sleeps a fixed span instead
// of waiting on a condition. A fixed sleep is either too short (a flake under
// load) or too long (a slow suite); the sanctioned wait is
// `crate::test_support::wait_until` / `wait_until_async`, which panic on timeout.
// A sleep that genuinely IS the subject (fake latency, a debounce window, a
// canceller's head start) opts out with an `// allowed-test-sleep: <reason>`
// directive. The convention is documented in `docs/testing.md`.
//
// "Test code" is every line of a dedicated test file (see isRustTestPath) plus
// the body of a `#[cfg(test)] mod { ... }` inside a production file. Production
// sleeps are out of jurisdiction and never flagged.
func RunTestSleep(ctx *CheckContext) (CheckResult, error) {
	// Every first-party tree. The vendored fork is out of jurisdiction: its tests
	// drive real FSEvents against upstream's own timing assumptions.
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-test-sleep")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []testSleepSite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		rootViolations, rootOrphans, rootScanned, scanErr := scanForTestSleep(ctx.RootDir, root)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
		}
		violations = append(violations, rootViolations...)
		orphans = append(orphans, rootOrphans...)
		scanned += rootScanned
	}

	var parts []string
	if len(violations) > 0 {
		sort.Slice(violations, func(i, j int) bool {
			if violations[i].relPath == violations[j].relPath {
				return violations[i].line < violations[j].line
			}
			return violations[i].relPath < violations[j].relPath
		})
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", v.relPath, v.line, v.text))
		}
		parts = append(parts, fmt.Sprintf(
			"found %d fixed sleep %s in test code (a fixed sleep flakes under load and "+
				"slows the suite: wait on a condition with `crate::test_support::wait_until` / "+
				"`wait_until_async`, which panic on timeout). If the sleep IS the subject "+
				"(fake latency, a debounce window, a canceller head start), add "+
				"`%s <reason>` on the line above:\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowTestSleepComment, strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowTestSleepComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, no fixed sleeps in test code",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForTestSleep(rootDir, srcDir string) ([]testSleepSite, []orphanDirective, int, error) {
	var violations []testSleepSite
	var orphans []orphanDirective
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		scanned++

		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}

		fileViolations, fileOrphans, scanErr := scanRustFileForTestSleep(path, relPath, isRustTestPath(relPath, d.Name()))
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
		return nil
	})

	return violations, orphans, scanned, err
}

// isRustTestPath reports whether every line of a file is test code. That's the
// conventional test-file names (isRustTestFile), plus any file under a `/tests/`
// module directory (e.g. `indexing/tests/external_drive_fixture.rs`,
// `mcp/tests/mod.rs`), plus a `*test_support*.rs` or `*test_fixtures*.rs` helper
// module. A production file that merely carries an inline `#[cfg(test)] mod tests`
// is NOT one of these; its test lines are found by the region tracker instead.
//
// The helper-module clauses match on the FILE name because such a module is
// gated at its declaration site (`#[cfg(test)] mod test_fixtures;`), leaving
// nothing inside the file for the region tracker to find.
func isRustTestPath(relPath, baseName string) bool {
	if isRustTestFile(baseName) {
		return true
	}
	if strings.Contains(relPath, "/tests/") {
		return true
	}
	if strings.Contains(baseName, "test_support") || strings.Contains(baseName, "test_fixtures") {
		return true
	}
	return false
}

// rustTestModState tracks the inline test-gated `mod { ... }` region inside a
// production file. It arms on a test-gated `cfg` attribute, enters on the next
// `mod ... {`, and leaves when brace depth returns to where the mod opened. It
// mirrors lock-poison's tracker but INVERTS the verdict: lock-poison skips the
// test mod, we scan only inside it.
//
// Shared by every scanner that has to tell test code from production code —
// test-sleep and fixed-temp-dir scan ONLY inside the region, derive-default and
// probe-unwrap scan only OUTSIDE it — so the region rules can't drift apart
// between them.
type rustTestModState struct {
	inTestMod      bool
	testModDepth   int
	pendingCfgTest bool
}

// testGatedCfgRegex matches a `cfg` attribute whose predicate turns a module
// into test-only code. Both spellings count: the bare `#[cfg(test)]`, and the
// `#[cfg(any(test, feature = "testing"))]` the `cmdr-fs` host stubs use, which
// exists because `cfg(test)` is OFF in a consumer's test build (see that
// crate's `DETAILS.md`). Matching only the literal form would leave six test
// doubles looking like production code to every scanner here.
//
// `\btest\b` deliberately misses `feature = "testing"` on its own — that gate
// alone doesn't make a module test-only — and `not(` is excluded so
// `#[cfg(not(test))]`, which marks the PRODUCTION arm, never arms the tracker.
var testGatedCfgRegex = regexp.MustCompile(`#\[cfg\([^]]*\btest\b`)

// isTestGatedCfg reports whether a line carries a cfg attribute that makes what
// follows test-only.
func isTestGatedCfg(line string) bool {
	if strings.Contains(line, "not(") {
		return false
	}
	return testGatedCfgRegex.MatchString(line)
}

// scanRustFileForTestSleep scans one file. When wholeFileIsTest is true every
// line is test code; otherwise only the body of an inline `#[cfg(test)] mod`
// counts, and the region tracker decides membership line by line.
func scanRustFileForTestSleep(path, relPath string, wholeFileIsTest bool) ([]testSleepSite, []orphanDirective, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []testSleepSite
	var state rustTestModState
	tracker := newDirectiveTracker(AllowTestSleepComment, "//")
	// The directive can sit anywhere in the contiguous comment block immediately
	// above the sleep, because a reason often wraps across two comment lines. We
	// remember the block's directive line and clear it on any non-comment line.
	blockDirectiveLine := 0
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		inTest := wholeFileIsTest
		if !wholeFileIsTest {
			inTest = advanceTestModRegion(line, &state)
		}
		if !inTest {
			blockDirectiveLine = 0
			continue
		}

		// Inside test code: this line's directives are in jurisdiction, so record
		// them for the orphan report.
		tracker.observe(lineNum, line)

		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "//") || strings.HasPrefix(trimmed, "*") {
			if strings.Contains(line, AllowTestSleepComment) {
				blockDirectiveLine = lineNum
			}
			continue
		}

		if !testSleepRegex.MatchString(line) {
			// A non-comment line ends the comment block above.
			blockDirectiveLine = 0
			continue
		}

		// Opt-out: a trailing directive on the sleep line, or a directive anywhere
		// in the contiguous comment block immediately above it.
		if strings.Contains(line, AllowTestSleepComment) {
			tracker.markLineUsed(lineNum)
		} else if blockDirectiveLine > 0 {
			tracker.markLineUsed(blockDirectiveLine)
		} else {
			violations = append(violations, testSleepSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		blockDirectiveLine = 0
	}

	return violations, tracker.orphans(relPath), scanner.Err()
}

// advanceTestModRegion advances the inline-test-mod state for one line and
// reports whether that line is inside the `#[cfg(test)] mod { ... }` body. The
// opening `mod ... {` line and the attribute line themselves return false: they
// carry no sleep worth flagging, and keeping them out matches how the directive
// tracker should see only real test-body lines.
func advanceTestModRegion(line string, state *rustTestModState) bool {
	if state.inTestMod {
		state.testModDepth += strings.Count(line, "{") - strings.Count(line, "}")
		if state.testModDepth <= 0 {
			state.inTestMod = false
		}
		return true
	}

	if isTestGatedCfg(line) {
		state.pendingCfgTest = true
	}
	if state.pendingCfgTest && strings.Contains(line, "mod ") && strings.Contains(line, "{") {
		state.pendingCfgTest = false
		state.testModDepth = strings.Count(line, "{") - strings.Count(line, "}")
		state.inTestMod = state.testModDepth > 0
		return false
	}
	return false
}
