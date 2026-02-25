package checks

import (
	"bufio"
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

// RunFileLength scans the repo for source files exceeding the line count threshold.
// Always succeeds — reports long files as a warning, never fails.
func RunFileLength(ctx *CheckContext) (CheckResult, error) {
	var longFiles []longFile

	err := filepath.WalkDir(ctx.RootDir, func(path string, d os.DirEntry, err error) error {
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

		ext := filepath.Ext(d.Name())
		if !fileLengthSourceExtensions[ext] {
			return nil
		}

		lineCount, err := countLines(path)
		if err != nil {
			return nil
		}

		if lineCount >= fileLengthWarnLines {
			info, err := d.Info()
			if err != nil {
				return nil
			}
			relPath, _ := filepath.Rel(ctx.RootDir, path)
			longFiles = append(longFiles, longFile{
				relPath:   relPath,
				lines:     lineCount,
				sizeBytes: info.Size(),
			})
		}

		return nil
	})
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan files: %w", err)
	}

	if len(longFiles) == 0 {
		return Success("All files under threshold"), nil
	}

	sort.Slice(longFiles, func(i, j int) bool {
		return longFiles[i].relPath < longFiles[j].relPath
	})

	var sb strings.Builder
	for _, f := range longFiles {
		sizeKB := f.sizeBytes / 1000
		tokenStr := formatTokenCount(f.sizeBytes / 4)
		detail := fmt.Sprintf("(%d lines, %d kB, ~%s tokens)", f.lines, sizeKB, tokenStr)

		color := ansiYellow
		if f.lines >= fileLengthCriticalLines {
			color = ansiRed
		}

		sb.WriteString(fmt.Sprintf("  - %s %s%s%s\n", f.relPath, color, detail, ansiReset))
	}

	msg := fmt.Sprintf("%d %s over %d lines:\n%s",
		len(longFiles),
		Pluralize(len(longFiles), "file", "files"),
		fileLengthWarnLines,
		strings.TrimRight(sb.String(), "\n"),
	)

	return CheckResult{Code: ResultWarning, Message: msg}, nil
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
