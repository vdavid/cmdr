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
	// Catches an audit-finding-style introduction even before the lowered
	// String gets bound to a `let lower = ...`. The May 2026 audit hit this
	// shape three times (installer / keychain / write_operations).
	//
	// Known gap: `let lower = msg.to_lowercase(); lower.contains(...)` is the
	// same bug but split across two lines. Widening to `\blower\.contains\(`
	// would also flag a handful of pre-existing, documented sites (Linux
	// mount-CLI output parsing, MTP USB-permission detection); fixing those
	// is out of scope here and review/CLAUDE.md guidance remains the second
	// line of defense.
	regexp.MustCompile(`\.to_lowercase\(\)\.contains\(`),
	regexp.MustCompile(`\.to_lowercase\(\)\.starts_with\(`),
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
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")

	violations, scanned, err := scanForErrorStringMatch(ctx.RootDir, rustSrcDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", err)
	}

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
		return CheckResult{}, fmt.Errorf(
			"found %d %s of substring-matching error/state values "+
				"(use a typed enum variant, errno code, or explicit flag instead: "+
				"add `%s <reason>` on the line above to opt a specific site out):\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowErrorStringMatchComment, sb.String(),
		)
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, no string-matching of error/state values",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForErrorStringMatch(rootDir, srcDir string) ([]errorStringMatchSite, int, error) {
	var violations []errorStringMatchSite
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
		if isRustTestFile(d.Name()) {
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
		var prev string
		lineNum := 0
		for scanner.Scan() {
			lineNum++
			line := scanner.Text()

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
		return scanner.Err()
	})

	return violations, scanned, err
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

// isRustTestFile recognizes the conventional Rust test-file names.
func isRustTestFile(name string) bool {
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
