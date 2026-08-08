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

// DefaultOKComment justifies one `#[derive(..., Default, ...)]` in the
// filesystem trees. The reason answers one question: is this type's ZERO VALUE a
// truthful claim, or a confident answer nobody made?
//
//	// DEFAULT-OK: zero really is "nothing enumerated yet", the state before a walk starts
//	#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
//	pub struct ListingProgress {
//
// If the sentence is hard to write, that derive is the next bug. `SourceHint`'s
// `Default` claimed `is_directory: false` about a filesystem nobody had looked
// at, and that claim cost a user's folder.
const DefaultOKComment = "// DEFAULT-OK:"

// deriveDefaultRegex matches a `#[derive(...)]` attribute listing `Default`. The
// word boundary keeps it off a `DefaultFoo` or `MyDefault` in the same list.
var deriveDefaultRegex = regexp.MustCompile(`#\[derive\([^)]*\bDefault\b`)

type deriveDefaultSite struct {
	relPath string
	line    int
	text    string
}

// RunDeriveDefaultJustified fails the build if a `Default` is derived in the
// filesystem trees without a written reason its zero value is honest.
//
// A `Default` on a fact-carrying type is a free wrong answer: it looks like "no
// information" and is actually a confident claim about the filesystem that
// nobody made. Removing one derive fixes one type; this makes it a rule, so the
// next type carrying a filesystem fact has to argue for its zero value at the
// moment someone writes it.
//
// Jurisdiction is PRODUCTION code under the app's `file_system/` and all of
// `cmdr-fs` — the two trees where a zero value is a claim about a disk. Test
// code is exempt in both senses the word has: a dedicated test file
// (isRustTestPath), AND the body of an inline test module inside a production
// file. A test double's zero value is a test's problem, and demanding an
// annotation there buys churn instead of safety. The `cmdr-fs` host stubs sit in
// `#[cfg(any(test, feature = "testing"))] mod` blocks, so the module tracker
// arms on that form too, not just on the bare `#[cfg(test)]`.
func RunDeriveDefaultJustified(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-derive-default-justified")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []deriveDefaultSite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		for _, tree := range deriveDefaultTrees(root) {
			if _, statErr := os.Stat(tree); statErr != nil {
				continue
			}
			treeViolations, treeOrphans, treeScanned, scanErr := scanForDeriveDefault(ctx.RootDir, tree)
			if scanErr != nil {
				return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
			}
			violations = append(violations, treeViolations...)
			orphans = append(orphans, treeOrphans...)
			scanned += treeScanned
		}
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
			"found %d unjustified `Default` %s in the filesystem trees (a zero value on a fact-carrying type "+
				"isn't \"no information\", it's a confident claim about the disk that nobody made). "+
				"Add `%s <why the zero value is truthful>` in the comment block above, or drop the derive:\n%s",
			len(violations), Pluralize(len(violations), "derive", "derives"), DefaultOKComment,
			strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(DefaultOKComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, every derived `Default` says why its zero value is truthful",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

// deriveDefaultTrees narrows a member's `src/` to the subtrees where a zero
// value is a claim about a disk. In the app that's `file_system/` alone; a
// member whose whole reason to exist is filesystem vocabulary (`cmdr-fs`) is in
// scope end to end. Everything else in the workspace stays out: widening to
// every member would put ~120 more derives under the rule and buy nothing, since
// the fault class is specifically "a type that carries a fact about a file".
func deriveDefaultTrees(srcDir string) []string {
	if strings.HasSuffix(filepath.ToSlash(srcDir), "crates/cmdr-fs/src") {
		return []string{srcDir}
	}
	return []string{filepath.Join(srcDir, "file_system")}
}

func scanForDeriveDefault(rootDir, srcDir string) ([]deriveDefaultSite, []orphanDirective, int, error) {
	var violations []deriveDefaultSite
	var orphans []orphanDirective
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}

		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}
		if isRustTestPath(filepath.ToSlash(relPath), d.Name()) {
			return nil
		}
		scanned++

		fileViolations, fileOrphans, scanErr := scanRustFileForDeriveDefault(path, relPath)
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
		return nil
	})

	return violations, orphans, scanned, err
}

// scanRustFileForDeriveDefault scans one production file, skipping inline test
// modules.
//
// The directive may sit anywhere in the contiguous run of comments AND
// attributes immediately above the derive, because a derive rarely touches its
// doc comment directly: `#[repr(C)]` and `#[cfg(target_os = "macos")]`
// legitimately sit between them.
func scanRustFileForDeriveDefault(path, relPath string) ([]deriveDefaultSite, []orphanDirective, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []deriveDefaultSite
	var state rustTestModState
	tracker := newDirectiveTracker(DefaultOKComment, "//")
	blockDirectiveLine := 0
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		// Inverted polarity: the inline test mod is the carve-out, not the target.
		if advanceTestModRegion(line, &state) {
			blockDirectiveLine = 0
			continue
		}

		tracker.observe(lineNum, line)

		trimmed := strings.TrimLeft(line, " \t")
		isComment := strings.HasPrefix(trimmed, "//") || strings.HasPrefix(trimmed, "*")
		if isComment {
			if strings.Contains(line, DefaultOKComment) {
				blockDirectiveLine = lineNum
			}
			continue
		}

		if !deriveDefaultRegex.MatchString(line) {
			// Another attribute keeps the block above alive; anything else ends it.
			if !strings.HasPrefix(trimmed, "#[") {
				blockDirectiveLine = 0
			}
			continue
		}

		if strings.Contains(line, DefaultOKComment) {
			tracker.markLineUsed(lineNum)
		} else if blockDirectiveLine > 0 {
			tracker.markLineUsed(blockDirectiveLine)
		} else {
			violations = append(violations, deriveDefaultSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		blockDirectiveLine = 0
	}

	return violations, tracker.orphans(relPath), scanner.Err()
}
