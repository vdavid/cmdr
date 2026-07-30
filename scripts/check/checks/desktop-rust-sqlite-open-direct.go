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

// sqliteOpenFactoryFile is the one file allowed to call `rusqlite::Connection::open*`:
// it IS the factory, and it installs the process-wide shared page cache
// (`sqlite3_config(SQLITE_CONFIG_PAGECACHE, ...)`) first. SQLite only accepts that
// slab BEFORE it initializes itself, and the first connection opened anywhere in the
// process initializes it — so one direct open that wins the race silently drops the
// whole app back to per-connection page-cache budgets that scale with the (thread-local,
// unbounded) connection count.
//
// Path is relative to the repo root.
const sqliteOpenFactoryFile = "apps/desktop/src-tauri/src/sqlite_util.rs"

// rawSqliteOpenPattern matches a direct rusqlite connection open: `Connection::open`,
// `Connection::open_with_flags`, `Connection::open_in_memory`, with or without the
// `rusqlite::` prefix.
var rawSqliteOpenPattern = regexp.MustCompile(`\bConnection::open\w*\s*\(`)

type sqliteOpenSite struct {
	relPath string
	line    int
	text    string
}

// RunSqliteOpenDirect greps the desktop Rust crate for direct
// `rusqlite::Connection::open*` calls outside the `sqlite_util` factories. The
// convention and why it's load-bearing are documented in
// `apps/desktop/src-tauri/src/indexing/store/DETAILS.md` § "SQLite page memory is one
// process-wide slab".
func RunSqliteOpenDirect(ctx *CheckContext) (CheckResult, error) {
	// KindApp only. The slab is process-wide, so only code linked into the Cmdr
	// process can lose the race; a standalone developer CLI opens the first
	// connection in its OWN process and has nothing to protect. The index store is
	// the biggest consumer of all this and is headed for a crate, so scanning the
	// app tree alone would leave the check watching the wrong half.
	roots, err := RustSrcRoots(ctx.RootDir, KindApp)
	if err != nil {
		return CheckResult{}, err
	}

	var violations []sqliteOpenSite
	scanned := 0
	for _, root := range roots {
		rootViolations, rootScanned, scanErr := scanForDirectSqliteOpen(ctx.RootDir, root)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
		}
		violations = append(violations, rootViolations...)
		scanned += rootScanned
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
			"found %d direct `Connection::open*` %s outside %s (use `crate::sqlite_util::{open, open_read_only, open_in_memory}`; the first connection in the process initializes SQLite and locks out the shared page-cache slab):\n%s",
			len(violations), Pluralize(len(violations), "call", "calls"), sqliteOpenFactoryFile,
			strings.TrimRight(sb.String(), "\n"),
		)
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, every SQLite connection opens through `sqlite_util`",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

// scanForDirectSqliteOpen walks the given source dir and returns every direct
// `Connection::open*` call site outside the factory file, plus the count of files
// scanned for reporting.
func scanForDirectSqliteOpen(rootDir, srcDir string) ([]sqliteOpenSite, int, error) {
	var violations []sqliteOpenSite
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
		if filepath.ToSlash(relPath) == sqliteOpenFactoryFile {
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
			if !rawSqliteOpenPattern.MatchString(line) {
				continue
			}
			// Skip lines that are clearly comments or doc comments.
			trimmed := strings.TrimLeft(line, " \t")
			if strings.HasPrefix(trimmed, "//") {
				continue
			}
			violations = append(violations, sqliteOpenSite{
				relPath: relPath,
				line:    lineNum,
				text:    strings.TrimSpace(line),
			})
		}
		return scanner.Err()
	})
	if err != nil {
		return nil, 0, err
	}

	return violations, scanned, nil
}
