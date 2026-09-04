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

// AllowHandRolledFixtureComment opts a single hand-built fixture out. The reason
// has to say why this test needs a shape production never emits — the honest
// case is a test pinning what the code does when an INCOHERENT entry reaches it
// anyway, which is exactly what a named constructor refuses to build.
//
//	// allowed-hand-rolled-fixture: the point is the shape the constructors reject
//	let result = CachedScanResult { … };
const AllowHandRolledFixtureComment = "// allowed-hand-rolled-fixture:"

// handRolledFixtureTypes are the cross-boundary types a test may not hand-build.
// Each one carries facts about the filesystem from one subsystem to another, and
// a hand-written literal reproduces the test author's assumptions instead of a
// shape production actually emits.
//
// `WrittenFile` is what a transfer's rollback ledger claims it put on disk, and
// its constructors are the whole point: each one names a kind of write (a local
// entry it stats itself, a volume file with the bytes it piped, a partial that
// was never complete), and none can produce a local entry missing its node id. A
// literal picks those fields by hand and certifies whatever the author assumed.
var handRolledFixtureTypes = []string{"CachedScanResult", "SourceHint", "VolumePreflight", "WrittenFile"}

// handRolledFixtureRegex matches a struct-literal construction of one of those
// types. A literal NAMES ITS TYPE at the construction site, so no type inference
// is needed.
//
// The leading boundary keeps it off a longer identifier ending in the same word,
// and the negative-lookalike handling is done in the scanner: a `-> Type {`
// return position and a `struct Type {` / `impl Type {` definition all end in the
// same two tokens without constructing anything.
var handRolledFixtureRegex = regexp.MustCompile(
	`\b(` + strings.Join(handRolledFixtureTypes, "|") + `)\s*\{`)

// fixtureDeclarationRegex matches the positions where `Type {` is a declaration
// or a return type rather than a construction.
var fixtureDeclarationRegex = regexp.MustCompile(`(->|\bstruct\b|\bimpl\b|\benum\b|\btrait\b)`)

type handRolledFixtureSite struct {
	relPath string
	line    int
	text    string
}

// RunNoHandRolledFixture fails the build if TEST code hand-builds one of the
// cross-boundary scan types instead of going through its named constructor.
//
// This is a REGRESSION FENCE, not a finder, and it's meant to ship with zero
// findings. Every fixture in the tree already goes through
// `CachedScanResult::from_local_walk` / `::from_volume_batch`; the check exists
// so the next test author can't undo that by copy-pasting an old literal, which
// is precisely how the original bug survived three months of green tests.
//
// The reason it matters: a hand-built `CachedScanResult` reproduces the
// implementer's mental model, not a shape any production walk emits. Every test
// that touched the scan cache seeded a fully-populated `per_path`, a shape the
// LOCAL preview path has never once produced — so the fixtures certified the
// bug. A named constructor can only build the shapes that exist.
//
// `SourceHint` and `VolumePreflight` match nothing today and by design will keep
// matching nothing: their only literals live in production `preflight.rs`, which
// the test-code scoping excludes. They're in the list so a future test can't
// start hand-building them either.
func RunNoHandRolledFixture(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-no-hand-rolled-fixture")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []handRolledFixtureSite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		rootViolations, rootOrphans, rootScanned, scanErr := scanForHandRolledFixture(ctx.RootDir, root)
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
			"found %d hand-built cross-boundary %s in test code (a literal reproduces the author's assumptions, "+
				"not a shape production emits — that's how a scan cache full of invented `per_path` entries "+
				"certified a data-loss bug for three months). Build through the named constructor "+
				"(`::from_local_walk` / `::from_volume_batch`), or say why this test needs a shape they refuse "+
				"with `%s <why>`:\n%s",
			len(violations), Pluralize(len(violations), "fixture", "fixtures"), AllowHandRolledFixtureComment,
			strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowHandRolledFixtureComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, every scan-cache fixture builds through a named constructor",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForHandRolledFixture(rootDir, srcDir string) ([]handRolledFixtureSite, []orphanDirective, int, error) {
	var violations []handRolledFixtureSite
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

		fileViolations, fileOrphans, scanErr := scanRustFileForHandRolledFixture(
			path, relPath, isRustTestPath(filepath.ToSlash(relPath), d.Name()))
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
		return nil
	})

	return violations, orphans, scanned, err
}

// scanRustFileForHandRolledFixture scans one file. When wholeFileIsTest is true
// every line is test code; otherwise only the body of an inline
// `#[cfg(test)] mod` counts — the same jurisdiction test-sleep and fixed-temp-dir
// use, so a production constructor's own body is never flagged.
func scanRustFileForHandRolledFixture(
	path, relPath string, wholeFileIsTest bool,
) ([]handRolledFixtureSite, []orphanDirective, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []handRolledFixtureSite
	var state rustTestModState
	tracker := newDirectiveTracker(AllowHandRolledFixtureComment, "//")
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

		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "//") || strings.HasPrefix(trimmed, "*") {
			if strings.Contains(line, AllowHandRolledFixtureComment) {
				blockDirectiveLine = lineNum
			}
			continue
		}

		loc := handRolledFixtureRegex.FindStringIndex(line)
		if loc == nil || fixtureDeclarationRegex.MatchString(line[:loc[0]]) {
			blockDirectiveLine = 0
			continue
		}

		if strings.Contains(line, AllowHandRolledFixtureComment) {
			tracker.markLineUsed(lineNum)
		} else if blockDirectiveLine > 0 {
			tracker.markLineUsed(blockDirectiveLine)
		} else {
			violations = append(violations, handRolledFixtureSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		blockDirectiveLine = 0
	}

	return violations, tracker.orphans(relPath), scanner.Err()
}
