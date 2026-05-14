package checks

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// allowlistedLogErrorSites lists files where raw `log::error!` calls are intentional
// (the macro definition itself). Anything outside this list must use `log_error!` so
// the auto-dispatcher (Flow B) sees user-visible errors.
//
// Paths are relative to the repo root.
var allowlistedLogErrorSites = map[string]bool{
	"apps/desktop/src-tauri/src/error_reporter/mod.rs": true,
}

type logErrorSite struct {
	relPath string
	line    int
	text    string
}

// RunLogErrorMacro greps the desktop Rust crate for `log::error!` calls and fails on
// any site outside the allowlist. The convention is documented in
// `apps/desktop/src-tauri/src/error_reporter/CLAUDE.md` § Convention.
func RunLogErrorMacro(ctx *CheckContext) (CheckResult, error) {
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")

	violations, scanned, err := scanForRawLogError(ctx.RootDir, rustSrcDir)
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
			"found %d raw `log::error!` %s outside the allowlist (use `crate::log_error!` instead; see error_reporter/CLAUDE.md):\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), sb.String(),
		)
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, no raw `log::error!` outside the allowlist",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

// scanForRawLogError walks the given source dir and returns every `log::error!` call
// site that isn't in the allowlist. Returns the count of files scanned for reporting.
func scanForRawLogError(rootDir, srcDir string) ([]logErrorSite, int, error) {
	var violations []logErrorSite
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
		if allowlistedLogErrorSites[relPath] {
			return nil
		}

		f, openErr := os.Open(path)
		if openErr != nil {
			return openErr
		}
		defer f.Close()

		scanner := bufio.NewScanner(f)
		// Allow long lines (default is 64 KB; some generated/test files exceed it).
		scanner.Buffer(make([]byte, 64*1024), 1024*1024)
		lineNum := 0
		for scanner.Scan() {
			lineNum++
			line := scanner.Text()
			if !strings.Contains(line, "log::error!") {
				continue
			}
			// Skip lines that are clearly comments: `///`, `//`, `//!`.
			trimmed := strings.TrimLeft(line, " \t")
			if strings.HasPrefix(trimmed, "//") {
				continue
			}
			violations = append(violations, logErrorSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		return scanner.Err()
	})

	return violations, scanned, err
}
