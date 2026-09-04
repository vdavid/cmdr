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

// AllowErrorStringMatchComment is the magic comment that opts a single line out
// of the error-string-match check. Place it on the line immediately above the
// flagged line, with a short reason.
//
//	// allowed-error-string-match: parsing structured smbutil output, see classify_smbutil_stderr
//	if stderr.contains("Authentication error") { ... }
const AllowErrorStringMatchComment = "// allowed-error-string-match:"

// errorStringMatchPatterns flags substring-matching against error/state semantics.
// We catch the common shapes; rarely-used variants fall through. False positives
// can be silenced with the AllowErrorStringMatchComment.
var errorStringMatchPatterns = []*regexp.Regexp{
	// Substring match on `message` (the field name on most VolumeError variants).
	regexp.MustCompile(`\bmessage\.contains\(`),
	regexp.MustCompile(`\bmessage\.starts_with\(`),
	// Substring match on subprocess output captured into `stderr` / `stdout`.
	regexp.MustCompile(`\bstderr\.contains\(`),
	regexp.MustCompile(`\bstderr\.starts_with\(`),
	regexp.MustCompile(`\bstdout\.contains\(`),
	regexp.MustCompile(`\bstdout\.starts_with\(`),
	// `err.to_string().contains(...)`: classifying an error by its Display impl.
	regexp.MustCompile(`\.to_string\(\)\.contains\(`),
	regexp.MustCompile(`\.to_string\(\)\.starts_with\(`),
	// `.to_lowercase().contains(...)` and `.to_lowercase().starts_with(...)`
	// are the canonical "classify by case-insensitive substring" anti-pattern.
	// Catches the inline chain even before the lowered String gets bound.
	regexp.MustCompile(`\.to_lowercase\(\)\.contains\(`),
	regexp.MustCompile(`\.to_lowercase\(\)\.starts_with\(`),
	// `let lower = msg.to_lowercase(); lower.contains(...)` is the same anti-
	// pattern split across two lines. We flag the canonical local-binding
	// names (`error`, `err`, `msg`, `err_msg`, `errmsg`, `lower`, `lowered`).
	// Any classification dressed up as one of these names trips the rule;
	// genuinely-content checks on unrelated locals (UI copy assertions, log
	// line routing) either pick a different variable name or carry the
	// `allowed-error-string-match:` opt-out with a reason.
	regexp.MustCompile(`\b(error|err|msg|err_msg|errmsg|lower|lowered)\.contains\(`),
	regexp.MustCompile(`\b(error|err|msg|err_msg|errmsg|lower|lowered)\.starts_with\(`),
}

type errorStringMatchSite struct {
	relPath string
	line    int
	text    string
}

// RunErrorStringMatch fails the build if any non-test Rust file matches an
// error/state value by substring. The convention is documented in
// `AGENTS.md` § "No string-matching error or state classification".
func RunErrorStringMatch(ctx *CheckContext) (CheckResult, error) {
	// Every first-party tree. Typed errors are the crate boundary's whole point, so
	// a crate is the last place a substring match should go unnoticed. The vendored
	// fork is out of jurisdiction.
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-error-string-match")
	if err != nil {
		return CheckResult{}, err
	}

	var violations []errorStringMatchSite
	var orphans []orphanDirective
	scanned := 0
	for _, root := range roots {
		rootViolations, rootOrphans, rootScanned, scanErr := scanForErrorStringMatch(ctx.RootDir, root)
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
			"found %d %s of substring-matching error/state values "+
				"(use a typed enum variant, errno code, or explicit flag instead: "+
				"add `%s <reason>` on the line above to opt a specific site out):\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowErrorStringMatchComment, strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowErrorStringMatchComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, no string-matching of error/state values",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForErrorStringMatch(rootDir, srcDir string) ([]errorStringMatchSite, []orphanDirective, int, error) {
	var violations []errorStringMatchSite
	var orphans []orphanDirective
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		// Skip dedicated test files. In-file `#[cfg(test)] mod tests {}` blocks are
		// still scanned: test assertions like `err.message.contains("...")` are
		// exactly the kind of stringly-typed check we want to flag.
		if isRustTestFile(path) {
			return nil
		}
		scanned++

		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}

		f, openErr := os.Open(path)
		if openErr != nil {
			return openErr
		}
		defer f.Close()

		scanner := bufio.NewScanner(f)
		scanner.Buffer(make([]byte, 64*1024), 1024*1024)
		tracker := newDirectiveTracker(AllowErrorStringMatchComment, "//")
		var prev string
		lineNum := 0
		for scanner.Scan() {
			lineNum++
			line := scanner.Text()
			tracker.observe(lineNum, line)

			trimmed := strings.TrimLeft(line, " \t")
			if strings.HasPrefix(trimmed, "//") {
				prev = line
				continue
			}

			if !lineMatchesErrorStringPattern(line) {
				prev = line
				continue
			}

			// Opt-out: `// allowed-error-string-match: <reason>` on the
			// previous line OR as a trailing comment on the same line.
			if hasAllowErrorStringMatchComment(prev) || hasAllowErrorStringMatchComment(line) {
				tracker.markUsed(lineNum, line, prev)
				prev = line
				continue
			}

			violations = append(violations, errorStringMatchSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
			prev = line
		}
		orphans = append(orphans, tracker.orphans(relPath)...)
		return scanner.Err()
	})

	return violations, orphans, scanned, err
}

func lineMatchesErrorStringPattern(line string) bool {
	for _, re := range errorStringMatchPatterns {
		if re.MatchString(line) {
			return true
		}
	}
	return false
}

func hasAllowErrorStringMatchComment(line string) bool {
	return strings.Contains(line, AllowErrorStringMatchComment)
}

// isRustTestFile recognizes the conventional Rust test-file layouts, by PATH rather than by
// base name: a module that outgrows one file becomes a directory, and every file in one is as
// much a dedicated test file as the `tests.rs` it was split out of. Two spellings of that
// directory count, and at ANY depth rather than only as the immediate parent: a bare `tests/`
// (`agent/store/proposals/tests/`, and `mcp/tests/tool_registry_tests/` once it grows submodule
// dirs of its own), and the same convention with the subject kept in the name, where a suite
// that outgrows `copy_tests.rs` becomes `copy_tests/progress.rs`. Splitting a long test file
// must not enroll its halves in a production-code scanner.
func isRustTestFile(path string) bool {
	for _, segment := range strings.Split(filepath.ToSlash(filepath.Dir(path)), "/") {
		if segment == "tests" || strings.HasSuffix(segment, "_tests") || strings.HasSuffix(segment, "_test") {
			return true
		}
	}
	name := filepath.Base(path)
	if name == "tests.rs" {
		return true
	}
	for _, suffix := range []string{"_test.rs", "_tests.rs"} {
		if strings.HasSuffix(name, suffix) {
			return true
		}
	}
	return false
}
