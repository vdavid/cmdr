package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// AllowLogPrivateComment opts one line out of the log-private-values check, for a
// value that only LOOKS like the user's (a setting's name, a thread's name, a
// fixed path of our own). Place it on the line above the flagged line, or as a
// trailing comment on it, with a reason.
//
//	// allowed-log-private: the setting's key, not a user value
//	log::debug!("setting {} changed", name);
const AllowLogPrivateComment = "// allowed-log-private:"

// logPrivateSite is one unmarked value inside a log macro.
type logPrivateSite struct {
	relPath string
	line    int
	what    string // why it reads as a user value: `display`, `name: src_path`, `capture`
	text    string
}

// markedStringSite is a marked value turned into a `String` outside a log macro.
type markedStringSite struct {
	relPath string
	line    int
	text    string
}

type logPrivateFileFindings struct {
	sites       []logPrivateSite
	conversions []markedStringSite
	orphans     []orphanDirective
}

// RunLogPrivateValues holds every log macro in non-test Rust code to the marker
// convention (`crates/cmdr-fs/src/log_private.rs`): a path, name, volume, host,
// or account goes into a log line through `LogPrivate` (or `LogDetail` /
// `LoggedText` / `LogError` for text), so the error-report bundle hashes the
// exact value instead of guessing where it ends. It fails, inside a log macro, on:
//
//   - `.display()` and `.to_string_lossy()`, which print a path bare;
//   - an argument or inline capture whose name reads as a path or identity
//     (`readsAsIdentity`), unless its chain ends in a scalar (`paths.len()`).
//
// Strict: there is no allowlist, only the per-line opt-out for a value that isn't
// the user's. Outside log macros it fails on a `LogPrivate` turned into a
// `String` (`.to_string()`, `format!`), since a marked string must never become a
// path, a stored value, or UI copy.
func RunLogPrivateValues(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-log-private-values")
	if err != nil {
		return CheckResult{}, err
	}
	all, scanned, err := scanLogPrivateRoots(ctx.RootDir, roots)
	if err != nil {
		return CheckResult{}, err
	}
	if hardParts := logPrivateFailures(all); len(hardParts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(hardParts, "\n"))
	}
	return Success(fmt.Sprintf("%d Rust %s scanned, every log macro marks its user values",
		scanned, Pluralize(scanned, "file", "files"))), nil
}

// scanLogPrivateRoots reads every non-test Rust file under the roots, indexes the
// path-carrying errors across all of them (a field's type is declared in one file
// and logged in another), then scans each file.
func scanLogPrivateRoots(rootDir string, roots []string) (logPrivateFileFindings, int, error) {
	var sources []rustSourceFile
	for _, root := range roots {
		rootSources, err := readLogPrivateSources(rootDir, root)
		if err != nil {
			return logPrivateFileFindings{}, 0, fmt.Errorf("failed to scan Rust files: %w", err)
		}
		sources = append(sources, rootSources...)
	}
	index := indexPathErrors(sources)
	var all logPrivateFileFindings
	for _, src := range sources {
		findings := scanRustSourceForLogPrivate(src, index)
		all.sites = append(all.sites, findings.sites...)
		all.conversions = append(all.conversions, findings.conversions...)
		all.orphans = append(all.orphans, findings.orphans...)
	}
	return all, len(sources), nil
}

// rustSourceFile is one scanned file: its repo-relative path, its source, and
// its masked twin (`maskRustSource`) with the string literals it found.
type rustSourceFile struct {
	relPath  string
	source   string
	masked   string
	literals []rustLiteral
}

// logPrivateFailures returns the report sections that fail the check: unmarked
// values, marked strings, and orphan directives.
func logPrivateFailures(all logPrivateFileFindings) []string {
	var hardParts []string
	if len(all.sites) > 0 {
		hardParts = append(hardParts, formatLogPrivateSites(all.sites))
	}
	if len(all.conversions) > 0 {
		hardParts = append(hardParts, formatMarkedStringSites(all.conversions))
	}
	if len(all.orphans) > 0 {
		hardParts = append(hardParts, formatOrphanDirectives(AllowLogPrivateComment, all.orphans))
	}
	return hardParts
}

// formatLogPrivateSites lists every unmarked value, grouped by file.
func formatLogPrivateSites(sites []logPrivateSite) string {
	byFile := map[string][]logPrivateSite{}
	for _, s := range sites {
		byFile[s.relPath] = append(byFile[s.relPath], s)
	}
	var sb strings.Builder
	for _, relPath := range sortedKeys(byFile) {
		fileSites := byFile[relPath]
		sort.SliceStable(fileSites, func(i, j int) bool { return fileSites[i].line < fileSites[j].line })
		for _, s := range fileSites {
			sb.WriteString(fmt.Sprintf("  %s:%d [%s] %s\n", relPath, s.line, s.what, s.text))
		}
	}
	return fmt.Sprintf(
		"found %d unmarked user %s in log macros (wrap each value in "+
			"`cmdr_fs::log_private::LogPrivate::<kind>(…)`, keeping the site's `key=` and format spec, or "+
			"`LogDetail(text).marking(&[…])` for foreign text; `%s <reason>` opts out a value that isn't the user's):\n%s",
		len(sites), Pluralize(len(sites), "value", "values"), AllowLogPrivateComment,
		strings.TrimRight(sb.String(), "\n"))
}

func formatMarkedStringSites(sites []markedStringSite) string {
	sort.Slice(sites, func(i, j int) bool {
		if sites[i].relPath == sites[j].relPath {
			return sites[i].line < sites[j].line
		}
		return sites[i].relPath < sites[j].relPath
	})
	var sb strings.Builder
	for _, s := range sites {
		sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", s.relPath, s.line, s.text))
	}
	return fmt.Sprintf(
		"found %d marked %s turned into a `String` outside a log macro (a marked string is log text, never "+
			"a path, a stored value, or UI copy: pass the wrapper to the log macro itself):\n%s",
		len(sites), Pluralize(len(sites), "value", "values"), strings.TrimRight(sb.String(), "\n"))
}

func readLogPrivateSources(rootDir, srcDir string) ([]rustSourceFile, error) {
	var sources []rustSourceFile
	err := corpus.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
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
		relPath = filepath.ToSlash(relPath)
		// Dedicated test files never reach a bundle (isRustTestPath lives with the
		// test-sleep check, in the same package).
		if isRustTestPath(relPath, d.Name()) {
			return nil
		}
		source, readErr := corpus.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		masked, literals := maskRustSource(source)
		sources = append(sources, rustSourceFile{relPath: relPath, source: source, masked: masked, literals: literals})
		return nil
	})
	return sources, err
}
