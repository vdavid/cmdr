package checks

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
)

const (
	fileLengthWarnLines = 800
	// fileLengthTestWarnLines is the threshold for test files (see isTestFile):
	// splitting a test file scatters shared mocks and fixtures across siblings
	// rather than improving architecture, so tests get more room before warning.
	fileLengthTestWarnLines = 1200
	fileLengthCriticalLines = 1200
	// fileLengthTestCriticalLines is a test file's red threshold, the same 1.5x
	// ratio above its warn line as fileLengthCriticalLines is above
	// fileLengthWarnLines — so a test file gets a yellow phase too, rather than
	// going straight to red the moment it crosses its (already generous) warn
	// line.
	fileLengthTestCriticalLines = 1800

	// Tolerate this much growth above each allowlisted file's recorded line count before warning,
	// so small incremental edits don't trigger a warning until growth becomes meaningful.
	fileLengthAllowlistBufferPct = 10

	ansiYellow = "\033[33m"
	ansiRed    = "\033[31m"
	ansiReset  = "\033[0m"
)

var fileLengthSourceExtensions = map[string]bool{
	".astro":  true,
	".css":    true,
	".go":     true,
	".html":   true,
	".js":     true,
	".rs":     true,
	".sh":     true,
	".svelte": true,
	".ts":     true,
}

// fileLengthSkipDirs are directory names this check never looks inside, matched at
// any depth. Most are build output that git wouldn't list anyway; `vendor` is the
// one that earns its keep. A vendored third-party crate (`vendor/<crate>`) is kept
// byte-identical to its upstream release, so its length is upstream's call, not
// ours, and allowlisting it would only churn on the next refresh.
var fileLengthSkipDirs = map[string]bool{
	"_ignored":     true,
	"build":        true,
	"dist":         true,
	"node_modules": true,
	"target":       true,
	"vendor":       true,
}

// inSkippedDir reports whether any segment of a repo-relative, forward-slashed
// path names a skipped directory. The filesystem walk prunes those directories as
// it goes; the git listing hands over finished paths, so it needs this.
func inSkippedDir(relPath string) bool {
	for segment := range strings.SplitSeq(relPath, "/") {
		if fileLengthSkipDirs[segment] {
			return true
		}
	}
	return false
}

type longFile struct {
	relPath   string
	lines     int
	sizeBytes int64
}

// isTestFile reports whether relPath (forward-slashed, repo-relative) is a test
// file under this repo's per-language naming conventions, so it gets
// fileLengthTestWarnLines instead of fileLengthWarnLines. An inline
// `#[cfg(test)] mod tests` block inside an ordinary .rs file does NOT count:
// that file is still source, matching its own name and location.
func isTestFile(relPath string) bool {
	base := path.Base(relPath)
	switch path.Ext(relPath) {
	case ".rs":
		return base == "tests.rs" || strings.HasSuffix(base, "_test.rs") ||
			strings.HasSuffix(base, "_tests.rs") || hasDirSegment(relPath, "tests")
	case ".go":
		return strings.HasSuffix(base, "_test.go")
	case ".ts", ".js":
		return strings.HasSuffix(base, ".test.ts") || strings.HasSuffix(base, ".test.js") ||
			strings.HasSuffix(base, ".spec.ts") || strings.HasSuffix(base, ".spec.js") ||
			underAppsTestDir(relPath)
	default:
		return false
	}
}

// hasDirSegment reports whether dir is one of relPath's directory components
// (the file name itself excluded).
func hasDirSegment(relPath, dir string) bool {
	for _, seg := range strings.Split(path.Dir(relPath), "/") {
		if seg == dir {
			return true
		}
	}
	return false
}

// underAppsTestDir reports whether relPath sits under an `apps/<app>/test/`
// directory (e2e specs and their helpers, staged under a non-test filename).
func underAppsTestDir(relPath string) bool {
	parts := strings.Split(relPath, "/")
	return len(parts) >= 3 && parts[0] == "apps" && parts[2] == "test"
}

// fileLengthThreshold returns the warn threshold that applies to relPath.
func fileLengthThreshold(relPath string) int {
	if isTestFile(relPath) {
		return fileLengthTestWarnLines
	}
	return fileLengthWarnLines
}

// fileLengthCriticalThreshold returns the red threshold that applies to
// relPath, picked the same way fileLengthThreshold picks the warn one.
func fileLengthCriticalThreshold(relPath string) int {
	if isTestFile(relPath) {
		return fileLengthTestCriticalLines
	}
	return fileLengthCriticalLines
}

// fileLengthAllowlist is the on-disk shape of file-length-allowlist.json.
// `Files` maps relative paths to accepted line counts (the contract a file may
// not silently grow past) plus the reason each one is accepted. `Exempt` maps
// relative paths to a reason for files whose length is not actionable at all
// (generated files): they never fail and never get ratcheted.
type fileLengthAllowlist struct {
	Comment string                     `json:"$comment,omitempty"`
	Exempt  map[string]string          `json:"exempt,omitempty"`
	Files   map[string]fileLengthLimit `json:"files"`
}

// fileLengthLimit is one `files` entry: the accepted line count, and why this
// file gets to be that long. The reason is mandatory (`RunFileLength` fails on
// an empty one) so an allowlisted file carries the thinking that put it there
// and nobody has to re-derive it; a reason starting with `fileLengthTodoPrefix`
// means the opposite, that the file SHOULD be split and here's how.
//
// The shrink-wrap rewrites the number and leaves the reason alone, which is why
// the two ride on one entry rather than in a parallel map: a parallel map drifts
// the moment an entry is removed from one side only.
type fileLengthLimit struct {
	Lines  int
	Reason string
}

// fileLengthTodoPrefix marks a reason that admits the file should be split.
// These are counted on the green line so the backlog stays visible instead of
// reading like an accepted decision.
const fileLengthTodoPrefix = "TODO:"

// fileLengthLimitJSON is the serialized form: always an object, so a hand-added
// entry that a reason is missing from reads as obviously incomplete rather than
// as a number somebody chose.
type fileLengthLimitJSON struct {
	Lines  int    `json:"lines"`
	Reason string `json:"reason"`
}

func (l fileLengthLimit) MarshalJSON() ([]byte, error) {
	return json.Marshal(fileLengthLimitJSON(l))
}

// UnmarshalJSON also accepts a bare number (`"path/to/x.rs": 903`). That form
// carries no reason, so the check reports it as incomplete rather than silently
// accepting it; taking it here means the error says which entry needs a reason,
// instead of the whole allowlist failing to parse and every long file in the
// repo lighting up at once.
func (l *fileLengthLimit) UnmarshalJSON(data []byte) error {
	var lines int
	if err := json.Unmarshal(data, &lines); err == nil {
		*l = fileLengthLimit{Lines: lines}
		return nil
	}
	var obj fileLengthLimitJSON
	if err := json.Unmarshal(data, &obj); err != nil {
		return fmt.Errorf("a `files` value is a line count or {\"lines\": N, \"reason\": \"…\"}: %w", err)
	}
	*l = fileLengthLimit(obj)
	return nil
}

// fileLengthAllowlistPath returns the allowlist location (next to the check
// source files).
func fileLengthAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", "file-length-allowlist.json")
}

// loadFileLengthAllowlist reads the allowlist JSON from the checks directory.
// A missing or unparsable file yields an empty allowlist (all long files get
// reported).
func loadFileLengthAllowlist(rootDir string) fileLengthAllowlist {
	var list fileLengthAllowlist
	data, err := os.ReadFile(fileLengthAllowlistPath(rootDir))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return fileLengthAllowlist{}
	}
	return list
}

// shrinkwrapFileLengthAllowlist computes the stale-entry verdicts: dead
// entries (file gone), satisfied entries (file under the warn threshold), and
// slack entries (file more than the growth buffer below its allowed count,
// which get ratcheted down to the current count). It mutates list in place and
// returns one human-readable line per change.
func shrinkwrapFileLengthAllowlist(rootDir string, list *fileLengthAllowlist) []string {
	var changes []string
	for _, relPath := range sortedKeys(list.Files) {
		allowed := list.Files[relPath]
		threshold := fileLengthThreshold(relPath)
		lineCount, err := countLines(filepath.Join(rootDir, relPath))
		switch {
		case err != nil:
			delete(list.Files, relPath)
			changes = append(changes, fmt.Sprintf("removed %s (file no longer exists)", relPath))
		case lineCount < threshold:
			delete(list.Files, relPath)
			changes = append(changes, fmt.Sprintf("removed %s (now %d lines, under the %d threshold)", relPath, lineCount, threshold))
		case lineCount <= allowed.Lines*(100-fileLengthAllowlistBufferPct)/100:
			// The number ratchets, the reason rides along: it explains the
			// file's shape, which shrinking by a few lines doesn't change.
			list.Files[relPath] = fileLengthLimit{Lines: lineCount, Reason: allowed.Reason}
			changes = append(changes, fmt.Sprintf("ratcheted %s: %d → %d lines", relPath, allowed.Lines, lineCount))
		}
	}
	for _, path := range sortedKeys(list.Exempt) {
		if !fileExists(filepath.Join(rootDir, path)) {
			delete(list.Exempt, path)
			changes = append(changes, fmt.Sprintf("removed exempt %s (file no longer exists)", path))
		}
	}
	return changes
}

type fileLengthScanResult struct {
	longFiles        []longFile
	allowlistedCount int
}

// scanFileLengths collects source files exceeding the threshold. It enumerates
// git-tracked files (so gitignored/untracked generated output is excluded for
// free), falling back to a filesystem walk outside a git work tree (e.g. tests
// against a throwaway dir). Each candidate is filtered by extension, line count,
// and the allowlist; a tracked file that's locally deleted is skipped silently.
func scanFileLengths(rootDir string, allowlist fileLengthAllowlist) (fileLengthScanResult, error) {
	relPaths, err := repoFiles(rootDir)
	if err != nil {
		return fileLengthScanResult{}, err
	}

	var result fileLengthScanResult
	for _, relPath := range relPaths {
		if !fileLengthSourceExtensions[filepath.Ext(relPath)] || inSkippedDir(relPath) {
			continue
		}
		absPath := filepath.Join(rootDir, relPath)
		lineCount, err := countLines(absPath)
		if err != nil || lineCount < fileLengthThreshold(relPath) {
			continue
		}
		if _, exempt := allowlist.Exempt[relPath]; exempt {
			result.allowlistedCount++
			continue
		}
		if allowed, ok := allowlist.Files[relPath]; ok && lineCount <= allowed.Lines*(100+fileLengthAllowlistBufferPct)/100 {
			result.allowlistedCount++
			continue
		}
		info, err := os.Stat(absPath)
		if err != nil {
			continue
		}
		result.longFiles = append(result.longFiles, longFile{relPath: relPath, lines: lineCount, sizeBytes: info.Size()})
	}
	return result, nil
}

// walkSourceFiles is the non-git fallback: a filesystem walk returning every
// source file as a repo-relative path, skipping hidden and vendored/generated
// dirs by name. Less precise than the git enumeration (exact-name skip set, not
// gitignore-aware), but it only runs outside a git work tree.
func walkSourceFiles(rootDir string) ([]string, error) {
	var files []string
	err := filepath.WalkDir(rootDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			name := d.Name()
			if path != rootDir && (strings.HasPrefix(name, ".") || fileLengthSkipDirs[name]) {
				return filepath.SkipDir
			}
			return nil
		}
		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			return nil
		}
		files = append(files, filepath.ToSlash(relPath))
		return nil
	})
	return files, err
}

// formatLongFiles builds the warning message listing long files.
func formatLongFiles(files []longFile, allowlist fileLengthAllowlist, allowlistedCount int) string {
	sort.Slice(files, func(i, j int) bool { return files[i].relPath < files[j].relPath })

	var sb strings.Builder
	for _, f := range files {
		sizeKB := f.sizeBytes / 1000
		tokenStr := formatTokenCount(f.sizeBytes / 4)
		detail := fmt.Sprintf("(%d lines, %d kB, ~%s tokens)", f.lines, sizeKB, tokenStr)
		if allowed, ok := allowlist.Files[f.relPath]; ok {
			// A zero ceiling has no growth to express, and dividing by it used to panic
			// the whole check instead of reporting the file. `0` is what a placeholder
			// entry carries while its real count is still being measured.
			if allowed.Lines > 0 {
				growthPct := (f.lines - allowed.Lines) * 100 / allowed.Lines
				detail = fmt.Sprintf("(%d lines, allowlist: %d, %d kB, ~%s tokens, +%d%% growth)", f.lines, allowed.Lines, sizeKB, tokenStr, growthPct)
			} else {
				detail = fmt.Sprintf("(%d lines, allowlist: %d, %d kB, ~%s tokens)", f.lines, allowed.Lines, sizeKB, tokenStr)
			}
		}
		color := ansiYellow
		if f.lines >= fileLengthCriticalThreshold(f.relPath) {
			color = ansiRed
		}
		sb.WriteString(fmt.Sprintf("  - %s %s%s%s\n", f.relPath, color, detail, ansiReset))
	}

	suffix := ""
	if allowlistedCount > 0 {
		suffix = fmt.Sprintf(" (%d allowlisted)", allowlistedCount)
	}
	return fmt.Sprintf("%d new %s over the length limit (%s lines, %s for tests)%s:\n%s\nsplit it if that's a genuine architectural win, otherwise add it to scripts/check/checks/file-length-allowlist.json WITH a reason",
		len(files), Pluralize(len(files), "file", "files"),
		FormatThousands(fileLengthWarnLines), FormatThousands(fileLengthTestWarnLines),
		suffix, strings.TrimRight(sb.String(), "\n"))
}

// reasonlessAllowlistEntries returns the `files` entries with no reason, which
// is what a bare-number entry (hand-added, or written before reasons were
// required) unmarshals to. Reported as a failure: an allowlist entry whose
// thinking wasn't written down has to be re-derived by whoever next wonders
// about that file, which is the cost this field exists to remove.
func reasonlessAllowlistEntries(list fileLengthAllowlist) []string {
	var missing []string
	for _, relPath := range sortedKeys(list.Files) {
		if strings.TrimSpace(list.Files[relPath].Reason) == "" {
			missing = append(missing, relPath)
		}
	}
	return missing
}

// countTodoAllowlistEntries counts the entries whose reason admits the file
// should be split. Printed on the green line so the number stays in view: these
// are a backlog, not settled decisions, and nothing else would ever surface
// them.
func countTodoAllowlistEntries(list fileLengthAllowlist) int {
	n := 0
	for _, limit := range list.Files {
		if strings.HasPrefix(strings.TrimSpace(limit.Reason), fileLengthTodoPrefix) {
			n++
		}
	}
	return n
}

// RunFileLength scans the repo for source files exceeding the line count threshold.
// Files in the allowlist are suppressed if at or below their allowlisted line count;
// files in the exempt section are always suppressed. Stale allowlist entries are
// shrink-wrapped: outside CI the check removes dead/satisfied entries and ratchets
// slack ones down to the current count; in CI it only reports them.
//
// FAILS on a file over the limit that isn't allowlisted, and on an allowlist
// entry with no reason. A warn had no owner: it survived the run that caused it
// and landed on David later as a separate triage pass, which is the work this
// check exists to keep off his desk. The agent whose run goes red splits the
// file, or allowlists it with a reason and says so in the commit.
//
// The shrink-wrap path stays advisory (a local auto-fix, a CI warn): that's
// bookkeeping the check does to itself, and turning it red would be new noise
// rather than someone's unfinished work.
func RunFileLength(ctx *CheckContext) (CheckResult, error) {
	allowlist := loadFileLengthAllowlist(ctx.RootDir)

	staleChanges := shrinkwrapFileLengthAllowlist(ctx.RootDir, &allowlist)
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(fileLengthAllowlistPath(ctx.RootDir), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/file-length-allowlist.json")
	}

	result, err := scanFileLengths(ctx.RootDir, allowlist)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan files: %w", err)
	}

	var staleMsg string
	if len(staleChanges) > 0 {
		verb := "Shrink-wrapped allowlist"
		if ctx.CI {
			verb = "Stale allowlist entries (a local run shrink-wraps them)"
		}
		staleMsg = fmt.Sprintf("%s:\n  - %s", verb, strings.Join(staleChanges, "\n  - "))
	}

	if missing := reasonlessAllowlistEntries(allowlist); len(missing) > 0 {
		return CheckResult{}, fmt.Errorf(
			"%d allowlist %s no reason:\n  - %s\nevery `files` entry is {\"lines\": N, \"reason\": \"…\"}: say why this file gets to be long, or `TODO: <why it should be split, and how>`",
			len(missing), Pluralize(len(missing), "entry has", "entries have"), strings.Join(missing, "\n  - "),
		)
	}

	if len(result.longFiles) == 0 {
		okMsg := "All files under threshold"
		if result.allowlistedCount > 0 {
			okMsg = fmt.Sprintf("No new long files (%d allowlisted", result.allowlistedCount)
			if todos := countTodoAllowlistEntries(allowlist); todos > 0 {
				okMsg += fmt.Sprintf(", %d marked TODO", todos)
			}
			okMsg += ")"
		}
		if staleMsg != "" {
			if ctx.CI {
				return CheckResult{Code: ResultWarning, Message: okMsg + "; " + staleMsg, Total: -1, Issues: -1, Changes: -1}, nil
			}
			res := SuccessWithChanges(okMsg + "; " + staleMsg)
			return res, nil
		}
		return Success(okMsg), nil
	}

	msg := formatLongFiles(result.longFiles, allowlist, result.allowlistedCount)
	if staleMsg != "" {
		msg += "\n" + staleMsg
	}
	// The shrink-wrap's own rewrite rides along in `msg`: an error result carries
	// no MadeChanges flag, and a failing check prints its message either way.
	return CheckResult{}, fmt.Errorf("%s", msg)
}

// countLines returns the file's line count, counting a final unterminated line.
// Counts newline bytes rather than scanning lines, so a file with one very long
// line (a generated bundle, a minified asset) is measured instead of erroring out
// and silently dropping out of every scan that calls this.
func countLines(path string) (int, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return 0, err
	}
	count := bytes.Count(data, []byte{'\n'})
	if len(data) > 0 && data[len(data)-1] != '\n' {
		count++
	}
	return count, nil
}

func formatTokenCount(tokens int64) string {
	if tokens >= 1000 {
		return fmt.Sprintf("%dk", tokens/1000)
	}
	return fmt.Sprintf("%d", tokens)
}
