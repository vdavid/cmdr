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

// AllowFixedTempDirComment opts a single site out of the fixed-temp-dir check.
// Place it on the line immediately above the flagged line, or as a trailing
// comment on it, with a reason that says why the OS temp root itself is load
// bearing (the assertion is about the temp root, the path is already made unique
// some other way, nothing ever deletes it).
//
//	// allowed-fixed-temp-dir: the OS temp root IS the assertion
//	assert!(dir.starts_with(std::env::temp_dir()));
const AllowFixedTempDirComment = "// allowed-fixed-temp-dir:"

// fixedTempDirRegex matches a use of the OS temp root. Any use in test code is
// suspect, because the interesting mistake is joining a constant onto it; the
// opt-out directive carries the handful of deliberate cases.
var fixedTempDirRegex = regexp.MustCompile(`\b(?:std::)?env::temp_dir\s*\(\s*\)`)

type fixedTempDirSite struct {
	relPath string
	line    int
	text    string
}

// RunFixedTempDir fails the build if any Rust TEST code builds a scratch
// directory from `std::env::temp_dir()`. A path under the OS temp root is shared
// by every process on the machine and by every past run, which gives three
// failure modes: two concurrent suite runs delete each other's live fixtures
// (nextest isolates processes but NOT the filesystem), a fixture survives with
// the previous run's files so a test can pass on leftovers, and teardown placed
// after the assertions never runs on a failure. The sanctioned fixture is
// `crate::test_support::TestDir`, which is process-unique and removes itself on
// drop. The convention is documented in `docs/testing.md`.
//
// "Test code" is every line of a dedicated test file (see isRustTestPath) plus
// the body of a `#[cfg(test)] mod { ... }` inside a production file. Production
// code that stages into the temp dir on purpose (the updater, icon samples,
// archive extraction, SMB auth files) is out of jurisdiction and never flagged.
func RunFixedTempDir(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-fixed-temp-dir")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []fixedTempDirSite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		rootViolations, rootOrphans, rootScanned, scanErr := scanForFixedTempDir(ctx.RootDir, root)
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
			"found %d fixture %s built on the OS temp root (every process on the machine shares it, "+
				"so two suite runs delete each other's live fixtures and a test can pass on the previous "+
				"run's leftovers). Use `crate::test_support::TestDir`, which is process-unique and removes "+
				"itself on drop. If the temp root itself is load bearing, add `%s <reason>` on the line above:\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowFixedTempDirComment, strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowFixedTempDirComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, every test fixture owns its scratch directory",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForFixedTempDir(rootDir, srcDir string) ([]fixedTempDirSite, []orphanDirective, int, error) {
	var violations []fixedTempDirSite
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

		fileViolations, fileOrphans, scanErr := scanRustFileForFixedTempDir(path, relPath, isRustTestPath(relPath, d.Name()))
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
		return nil
	})

	return violations, orphans, scanned, err
}

// scanRustFileForFixedTempDir scans one file. When wholeFileIsTest is true every
// line is test code; otherwise only the body of an inline `#[cfg(test)] mod`
// counts, and the shared region tracker decides membership line by line.
func scanRustFileForFixedTempDir(path, relPath string, wholeFileIsTest bool) ([]fixedTempDirSite, []orphanDirective, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []fixedTempDirSite
	var state rustTestModState
	tracker := newDirectiveTracker(AllowFixedTempDirComment, "//")
	// The directive can sit anywhere in the contiguous comment block immediately
	// above the site, because a reason often wraps across two comment lines.
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

		tracker.observe(lineNum, line)

		// A doc comment naming the anti-pattern (`TestDir`'s own "why not
		// env::temp_dir()" rationale, the module docs that point at it) is prose,
		// not a fixture. Comment lines only carry the directive forward.
		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "//") || strings.HasPrefix(trimmed, "*") {
			if strings.Contains(line, AllowFixedTempDirComment) {
				blockDirectiveLine = lineNum
			}
			continue
		}

		if !fixedTempDirRegex.MatchString(line) {
			blockDirectiveLine = 0
			continue
		}

		if strings.Contains(line, AllowFixedTempDirComment) {
			tracker.markLineUsed(lineNum)
		} else if blockDirectiveLine > 0 {
			tracker.markLineUsed(blockDirectiveLine)
		} else {
			violations = append(violations, fixedTempDirSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		blockDirectiveLine = 0
	}

	return violations, tracker.orphans(relPath), scanner.Err()
}
