package checks

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

const (
	fileLengthWarnLines     = 800
	fileLengthCriticalLines = 1200

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

// loadFileLengthAllowlist reads the allowlist JSON from the checks directory.
// Returns a map of relative path → allowed line count.
func loadFileLengthAllowlist(rootDir string) map[string]int {
	// The allowlist lives next to the check source files
	allowlistPath := filepath.Join(rootDir, "scripts", "check", "checks", "file-length-allowlist.json")
	data, err := os.ReadFile(allowlistPath)
	if err != nil {
		return nil
	}
	var raw struct {
		Files map[string]int `json:"files"`
	}
	if err := json.Unmarshal(data, &raw); err != nil {
		return nil
	}
	return raw.Files
}

type fileLengthScanResult struct {
	longFiles        []longFile
	allowlistedCount int
}

// scanFileLengths walks the repo and collects files exceeding the threshold.
func scanFileLengths(rootDir string, allowlist map[string]int) (fileLengthScanResult, error) {
	var result fileLengthScanResult

	err := filepath.WalkDir(rootDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			name := d.Name()
			if strings.HasPrefix(name, ".") || fileLengthSkipDirs[name] {
				return filepath.SkipDir
			}
			return nil
		}
		if !fileLengthSourceExtensions[filepath.Ext(d.Name())] {
			return nil
		}
		lineCount, err := countLines(path)
		if err != nil || lineCount < fileLengthWarnLines {
			return nil
		}
		relPath, _ := filepath.Rel(rootDir, path)
		if allowedLines, ok := allowlist[relPath]; ok && lineCount <= allowedLines {
			result.allowlistedCount++
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return nil
		}
		result.longFiles = append(result.longFiles, longFile{relPath: relPath, lines: lineCount, sizeBytes: info.Size()})
		return nil
	})
	return result, err
}

// formatLongFiles builds the warning message listing long files.
func formatLongFiles(files []longFile, allowlist map[string]int, allowlistedCount int) string {
	sort.Slice(files, func(i, j int) bool { return files[i].relPath < files[j].relPath })

	var sb strings.Builder
	for _, f := range files {
		sizeKB := f.sizeBytes / 1000
		tokenStr := formatTokenCount(f.sizeBytes / 4)
		detail := fmt.Sprintf("(%d lines, %d kB, ~%s tokens)", f.lines, sizeKB, tokenStr)
		if allowedLines, ok := allowlist[f.relPath]; ok {
			detail = fmt.Sprintf("(%d lines, allowlist: %d, %d kB, ~%s tokens)", f.lines, allowedLines, sizeKB, tokenStr)
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
// Files in the allowlist are suppressed if at or below their allowlisted line count.
// Always succeeds — reports long files as a warning, never fails.
func RunFileLength(ctx *CheckContext) (CheckResult, error) {
	allowlist := loadFileLengthAllowlist(ctx.RootDir)
	result, err := scanFileLengths(ctx.RootDir, allowlist)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan files: %w", err)
	}

	if len(result.longFiles) == 0 {
		if result.allowlistedCount > 0 {
			return Success(fmt.Sprintf("No new long files (%d allowlisted)", result.allowlistedCount)), nil
		}
		return Success("All files under threshold"), nil
	}

	msg := formatLongFiles(result.longFiles, allowlist, result.allowlistedCount)
	return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: -1, Changes: -1}, nil
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
