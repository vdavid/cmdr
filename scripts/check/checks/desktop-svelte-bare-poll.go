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

// AllowBarePollComment is the magic comment that opts a single line out of the
// bare-poll check. Place it on the line immediately above the flagged line, or
// at the end of the flagged line, with a short reason.
//
//	// allowed-bare-poll: best-effort cleanup of any lingering modal
//	await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.modal-overlay')), 3000)
const AllowBarePollComment = "// allowed-bare-poll:"

// barePollHelpers is the set of `Promise<boolean>` polling helpers whose return
// value is the success signal. Awaiting them as a bare expression statement
// silently swallows timeouts: the helper returns `false`, no assertion ever
// runs, and the test passes green. The fix is either to use Playwright's
// `expect.poll(...).toBeTruthy()` (preferred) or to wrap the call in
// `expect(await ...).toBe(true)`.
//
// Add new helpers here as they're introduced.
var barePollHelpers = []string{
	"pollUntil",
	"pollFs",
	"pollUntilValue",
	"pollOverlayGone",
	"pollFocusedPane",
	"pollActiveMode",
}

// barePollRegex matches `await <helper>(` at the start of a line (after any
// indent). That's the bare-expression-statement shape: the return value isn't
// assigned, returned, wrapped in `expect(...)`, or guarded by an `if`. Same-
// line lead-ins like `const x = await foo(` or `expect(await foo(` have
// non-whitespace before `await`, so the start-of-line anchor excludes them.
var barePollRegex = regexp.MustCompile(
	`^\s*await\s+(` + strings.Join(barePollHelpers, "|") + `)\s*\(`,
)

type barePollSite struct {
	relPath string
	line    int
	helper  string
	text    string
}

// RunBarePoll fails the build if any test file under `apps/desktop/test/` calls
// one of the known `Promise<boolean>` polling helpers as a bare expression
// statement (return value discarded). The convention is documented in
// `apps/desktop/test/e2e-playwright/CLAUDE.md` § "Polling helpers".
func RunBarePoll(ctx *CheckContext) (CheckResult, error) {
	testDir := filepath.Join(ctx.RootDir, "apps", "desktop", "test")

	violations, scanned, err := scanForBarePoll(ctx.RootDir, testDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan test files: %w", err)
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
			"found %d bare `await <pollHelper>(...)` %s where the return value is "+
				"discarded (the test silently passes if the poll times out). "+
				"Prefer Playwright's `expect.poll(() => ...).toBeTruthy()`, or wrap as "+
				"`expect(await pollUntil(...)).toBe(true)`. "+
				"Add `%s <reason>` on the line above to opt a specific site out (rare):\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowBarePollComment, sb.String(),
		)
	}

	return Success(fmt.Sprintf(
		"%d test %s scanned, no bare polling-helper calls",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForBarePoll(rootDir, testDir string) ([]barePollSite, int, error) {
	var violations []barePollSite
	scanned := 0

	err := filepath.WalkDir(testDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			// Skip generated / heavy directories.
			if d.Name() == "node_modules" || d.Name() == "test-results" || d.Name() == "playwright-report" {
				return filepath.SkipDir
			}
			return nil
		}
		// Scan only TypeScript files; the patterns are TS-specific.
		if !strings.HasSuffix(d.Name(), ".ts") {
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
			if strings.HasPrefix(trimmed, "//") || strings.HasPrefix(trimmed, "*") {
				prev = line
				continue
			}

			m := barePollRegex.FindStringSubmatch(line)
			if m == nil {
				prev = line
				continue
			}

			// Opt-out: `// allowed-bare-poll: <reason>` on the previous line OR as
			// a trailing comment on the same line.
			if hasAllowBarePollComment(prev) || hasAllowBarePollComment(line) {
				prev = line
				continue
			}

			violations = append(violations, barePollSite{
				relPath: relPath,
				line:    lineNum,
				helper:  m[1],
				text:    strings.TrimSpace(line),
			})
			prev = line
		}
		return scanner.Err()
	})

	return violations, scanned, err
}

func hasAllowBarePollComment(line string) bool {
	return strings.Contains(line, AllowBarePollComment)
}
