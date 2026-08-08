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

// AllowProbeUnwrapComment opts a single `is_directory(...).await.unwrap_or(...)`
// site out. The reason has to answer one question and only that question: what
// does a WRONG answer cost here? A truthful default reaches no destructive
// branch and no lying progress count; anything else needs the probe's error
// propagated instead of swallowed.
//
//	// allowed-probe-unwrap: labels an undo-history row, reaches no destructive branch
//	Some(v) => v.is_directory(&from).await.unwrap_or(false),
const AllowProbeUnwrapComment = "// allowed-probe-unwrap:"

// probeUnwrapRegex matches a directory probe whose failure is swallowed into a
// guess. Scoped on the METHOD name rather than on the receiver: the method is
// right there at the call site, so no type inference is needed, and a variable
// rename can't make the rule stop firing.
//
// Deliberately NOT widened to `exists()` / `get_metadata()`. `is_directory` is
// the probe whose wrong answer picks a branch that deletes; the other two would
// double the finding set with mostly-truthful sites and turn the directive into
// noise.
var probeUnwrapRegex = regexp.MustCompile(`\.is_directory\(.*\)\.await\.unwrap_or\(`)

type probeUnwrapSite struct {
	relPath string
	line    int
	text    string
}

// RunProbeUnwrapJustified fails the build if production code under
// `file_system/` turns a failed "is this a directory?" probe into a confident
// answer without saying why that guess is truthful.
//
// A probe that CAN'T answer is not the same as a probe that answered "no", and
// collapsing the two is the exact shape of the bug this rule exists for: a
// directory guessed to be a file gets streamed as one, and the cleanup guard
// keyed on that same flag goes the wrong way and recurses into the user's
// merged destination folder. The compiler can't see any of it — the shape is
// hand-written and type-free, so removing a `Default` from a type catches none
// of these — which is why a scanner is the only mechanism there is.
//
// Jurisdiction is PRODUCTION code under `file_system/`: a dedicated test file
// (isRustTestPath) and the body of an inline `#[cfg(test)] mod` inside a
// production file are both exempt, because every such site reads a final state
// in an assertion rather than driving a branch.
func RunProbeUnwrapJustified(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-probe-unwrap-justified")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []probeUnwrapSite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		fsRoot := filepath.Join(root, "file_system")
		if _, statErr := os.Stat(fsRoot); statErr != nil {
			continue
		}
		rootViolations, rootOrphans, rootScanned, scanErr := scanForProbeUnwrap(ctx.RootDir, fsRoot)
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
			"found %d swallowed directory %s (a probe that COULDN'T answer is not a probe that answered \"no\"; "+
				"a guessed `false` streams a directory as a file and picks the branch that deletes). "+
				"Propagate the error and fail the item, or say why the guess is truthful here with "+
				"`%s <why>` on the line above:\n%s",
			len(violations), Pluralize(len(violations), "probe", "probes"), AllowProbeUnwrapComment,
			strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowProbeUnwrapComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, every directory probe either propagates or says why its guess is truthful",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForProbeUnwrap(rootDir, srcDir string) ([]probeUnwrapSite, []orphanDirective, int, error) {
	var violations []probeUnwrapSite
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
		// A whole test file is out of jurisdiction; don't even count it.
		if isRustTestPath(filepath.ToSlash(relPath), d.Name()) {
			return nil
		}
		scanned++

		fileViolations, fileOrphans, scanErr := scanRustFileForProbeUnwrap(path, relPath)
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
		return nil
	})

	return violations, orphans, scanned, err
}

// scanRustFileForProbeUnwrap scans one PRODUCTION file, skipping the body of any
// inline test module.
func scanRustFileForProbeUnwrap(path, relPath string) ([]probeUnwrapSite, []orphanDirective, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []probeUnwrapSite
	var state rustTestModState
	tracker := newDirectiveTracker(AllowProbeUnwrapComment, "//")
	blockDirectiveLine := 0
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		// INVERTED polarity against test-sleep's use of the same helper: there the
		// test mod is the jurisdiction, here it's the carve-out.
		if advanceTestModRegion(line, &state) {
			blockDirectiveLine = 0
			continue
		}

		tracker.observe(lineNum, line)

		trimmed := strings.TrimLeft(line, " \t")
		if strings.HasPrefix(trimmed, "//") || strings.HasPrefix(trimmed, "*") {
			if strings.Contains(line, AllowProbeUnwrapComment) {
				blockDirectiveLine = lineNum
			}
			continue
		}

		if !probeUnwrapRegex.MatchString(line) {
			blockDirectiveLine = 0
			continue
		}

		if strings.Contains(line, AllowProbeUnwrapComment) {
			tracker.markLineUsed(lineNum)
		} else if blockDirectiveLine > 0 {
			tracker.markLineUsed(blockDirectiveLine)
		} else {
			violations = append(violations, probeUnwrapSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		blockDirectiveLine = 0
	}

	return violations, tracker.orphans(relPath), scanner.Err()
}
