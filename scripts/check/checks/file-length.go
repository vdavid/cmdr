package checks

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

const (
	fileLengthWarnLines     = 800
	fileLengthCriticalLines = 1200

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

var fileLengthSkipDirs = map[string]bool{
	"_ignored":     true,
	"build":        true,
	"dist":         true,
	"node_modules": true,
	"target":       true,
}

type longFile struct {
	relPath   string
	lines     int
	sizeBytes int64
}

// fileLengthAllowlist is the on-disk shape of file-length-allowlist.json.
// `Files` maps relative paths to accepted line counts (the contract a file may
// not silently grow past). `Exempt` maps relative paths to a reason for files
// whose length is not actionable at all (generated files): they never warn and
// never get ratcheted.
type fileLengthAllowlist struct {
	Comment string            `json:"$comment,omitempty"`
	Exempt  map[string]string `json:"exempt,omitempty"`
	Files   map[string]int    `json:"files"`
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
	for _, path := range sortedKeys(list.Files) {
		allowed := list.Files[path]
		lineCount, err := countLines(filepath.Join(rootDir, path))
		switch {
		case err != nil:
			delete(list.Files, path)
			changes = append(changes, fmt.Sprintf("removed %s (file no longer exists)", path))
		case lineCount < fileLengthWarnLines:
			delete(list.Files, path)
			changes = append(changes, fmt.Sprintf("removed %s (now %d lines, under the %d threshold)", path, lineCount, fileLengthWarnLines))
		case lineCount <= allowed*(100-fileLengthAllowlistBufferPct)/100:
			list.Files[path] = lineCount
			changes = append(changes, fmt.Sprintf("ratcheted %s: %d → %d lines", path, allowed, lineCount))
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
	relPaths, ok := gitTrackedFiles(rootDir)
	if !ok {
		var err error
		relPaths, err = walkSourceFiles(rootDir)
		if err != nil {
			return fileLengthScanResult{}, err
		}
	}

	var result fileLengthScanResult
	for _, relPath := range relPaths {
		if !fileLengthSourceExtensions[filepath.Ext(relPath)] {
			continue
		}
		absPath := filepath.Join(rootDir, relPath)
		lineCount, err := countLines(absPath)
		if err != nil || lineCount < fileLengthWarnLines {
			continue
		}
		if _, exempt := allowlist.Exempt[relPath]; exempt {
			result.allowlistedCount++
			continue
		}
		if allowedLines, ok := allowlist.Files[relPath]; ok && lineCount <= allowedLines*(100+fileLengthAllowlistBufferPct)/100 {
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

// gitTrackedFiles returns every tracked file as a repo-relative, forward-slashed
// path. Returns (nil, false) when rootDir isn't a git work tree, so the caller
// can fall back to a filesystem walk. Tracked-only (no `--others`) is the whole
// point: gitignored and untracked generated output never reaches the scanner.
func gitTrackedFiles(rootDir string) ([]string, bool) {
	cmd := exec.Command("git", "-C", rootDir, "ls-files", "-z")
	out, err := cmd.Output()
	if err != nil {
		return nil, false
	}
	var files []string
	for rel := range strings.SplitSeq(string(out), "\x00") {
		if rel != "" {
			files = append(files, rel)
		}
	}
	return files, true
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
		if allowedLines, ok := allowlist.Files[f.relPath]; ok {
			growthPct := (f.lines - allowedLines) * 100 / allowedLines
			detail = fmt.Sprintf("(%d lines, allowlist: %d, %d kB, ~%s tokens, +%d%% growth)", f.lines, allowedLines, sizeKB, tokenStr, growthPct)
		}
		color := ansiYellow
		if f.lines >= fileLengthCriticalLines {
			color = ansiRed
		}
		sb.WriteString(fmt.Sprintf("  - %s %s%s%s\n", f.relPath, color, detail, ansiReset))
	}

	suffix := ""
	if allowlistedCount > 0 {
		suffix = fmt.Sprintf(" (%d allowlisted)", allowlistedCount)
	}
	return fmt.Sprintf("%d new %s over %d lines%s:\n%s",
		len(files), Pluralize(len(files), "file", "files"),
		fileLengthWarnLines, suffix, strings.TrimRight(sb.String(), "\n"))
}

// RunFileLength scans the repo for source files exceeding the line count threshold.
// Files in the allowlist are suppressed if at or below their allowlisted line count;
// files in the exempt section are always suppressed. Stale allowlist entries are
// shrink-wrapped: outside CI the check removes dead/satisfied entries and ratchets
// slack ones down to the current count; in CI it only reports them.
// Always succeeds: reports long files (and CI-mode staleness) as a warning, never fails.
func RunFileLength(ctx *CheckContext) (CheckResult, error) {
	allowlist := loadFileLengthAllowlist(ctx.RootDir)

	staleChanges := shrinkwrapFileLengthAllowlist(ctx.RootDir, &allowlist)
	madeChanges := false
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(fileLengthAllowlistPath(ctx.RootDir), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/file-length-allowlist.json")
		madeChanges = true
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

	if len(result.longFiles) == 0 {
		okMsg := "All files under threshold"
		if result.allowlistedCount > 0 {
			okMsg = fmt.Sprintf("No new long files (%d allowlisted)", result.allowlistedCount)
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
	return CheckResult{Code: ResultWarning, Message: msg, MadeChanges: madeChanges, Total: -1, Issues: -1, Changes: -1}, nil
}

func countLines(path string) (int, error) {
	f, err := os.Open(path)
	if err != nil {
		return 0, err
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	count := 0
	for scanner.Scan() {
		count++
	}
	return count, scanner.Err()
}

func formatTokenCount(tokens int64) string {
	if tokens >= 1000 {
		return fmt.Sprintf("%dk", tokens/1000)
	}
	return fmt.Sprintf("%d", tokens)
}
