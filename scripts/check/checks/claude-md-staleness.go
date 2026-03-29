package checks

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

const (
	// staleDays is the number of days after which a CLAUDE.md is considered
	// potentially stale relative to source files in its directory.
	staleDays = 30
)

// sourceExtensions lists the file extensions considered as "source code"
// when checking for staleness. Test files and generated files are excluded
// separately by name pattern.
var sourceExtensions = map[string]bool{
	".rs":     true,
	".ts":     true,
	".svelte": true,
	".css":    true,
	".go":     true,
	".js":     true,
}

// skipDirs lists directories that should never be scanned for source files.
var stalenessSkipDirs = map[string]bool{
	"vendor":        true,
	"node_modules":  true,
	".cargo-docker": true,
	"target":        true,
	"build":         true,
	"dist":          true,
}

type staleEntry struct {
	dir      string // relative path to the directory containing CLAUDE.md
	daysDiff int    // how many days newer the most recent source file is
}

// RunClaudeMdStaleness checks whether CLAUDE.md files might be stale relative
// to the source files in their directory. Always succeeds — emits warnings,
// never fails.
func RunClaudeMdStaleness(ctx *CheckContext) (CheckResult, error) {
	// Step 1: Find all CLAUDE.md files in the repo
	claudeFiles, err := findClaudeMdFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find CLAUDE.md files: %w", err)
	}

	if len(claudeFiles) == 0 {
		return Success("No CLAUDE.md files found"), nil
	}

	// Build a set of directories that have their own CLAUDE.md (for scoping)
	claudeDirs := make(map[string]bool)
	for _, f := range claudeFiles {
		claudeDirs[filepath.Dir(f)] = true
	}

	// Step 2: Build a bulk map of file path → last modified timestamp
	// using a single git log command instead of one per file.
	timestamps, err := gitLastModifiedBulk(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to get git timestamps: %w", err)
	}

	// Step 3: For each CLAUDE.md, check staleness
	var staleEntries []staleEntry

	for _, claudePath := range claudeFiles {
		dir := filepath.Dir(claudePath)

		claudeTime := timestamps[claudePath]
		if claudeTime == 0 {
			continue // file not tracked by git or no history
		}

		newestSource := findNewestSourceTimeBulk(ctx.RootDir, dir, claudeDirs, timestamps)
		if newestSource == 0 {
			continue // no source files found
		}

		diffDays := int((newestSource - claudeTime) / (60 * 60 * 24))
		if diffDays >= staleDays {
			relDir, _ := filepath.Rel(ctx.RootDir, filepath.Join(ctx.RootDir, dir))
			staleEntries = append(staleEntries, staleEntry{
				dir:      relDir,
				daysDiff: diffDays,
			})
		}
	}

	if len(staleEntries) == 0 {
		return Success(fmt.Sprintf("%d CLAUDE.md %s checked, all up to date",
			len(claudeFiles), Pluralize(len(claudeFiles), "file", "files"))), nil
	}

	sort.Slice(staleEntries, func(i, j int) bool {
		return staleEntries[i].dir < staleEntries[j].dir
	})

	var sb strings.Builder
	for _, e := range staleEntries {
		sb.WriteString(fmt.Sprintf("  - %s/ — source files modified %d days after the doc\n", e.dir, e.daysDiff))
	}

	msg := fmt.Sprintf("%d of %d CLAUDE.md %s may be stale (>%d days behind source):\n%s"+
		"Please verify these docs still match the code.",
		len(staleEntries),
		len(claudeFiles),
		Pluralize(len(claudeFiles), "file", "files"),
		staleDays,
		sb.String(),
	)

	return CheckResult{
		Code:    ResultWarning,
		Message: msg,
		Total:   len(claudeFiles),
		Issues:  len(staleEntries),
		Changes: -1,
	}, nil
}

// findClaudeMdFiles walks the repo and returns paths to all CLAUDE.md files,
// relative to rootDir. Skips vendor, node_modules, .cargo-docker, and hidden dirs.
func findClaudeMdFiles(rootDir string) ([]string, error) {
	var files []string

	err := filepath.WalkDir(rootDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			name := d.Name()
			if strings.HasPrefix(name, ".") || stalenessSkipDirs[name] {
				return filepath.SkipDir
			}
			return nil
		}
		if d.Name() == "CLAUDE.md" {
			rel, _ := filepath.Rel(rootDir, path)
			files = append(files, rel)
		}
		return nil
	})

	return files, err
}

// gitLastModifiedBulk returns a map of relative file path → unix timestamp of
// the last git commit that touched each file. Runs a single git log command
// instead of one per file, which is dramatically faster on large repos.
func gitLastModifiedBulk(rootDir string) (map[string]int64, error) {
	// git log --format="%ct" --name-only traverses the full history and emits
	// each commit's timestamp followed by the files it touched. We scan through
	// and keep only the first (most recent) timestamp per file.
	cmd := exec.Command("git", "log", "--format=%ct", "--name-only")
	cmd.Dir = rootDir

	var stdout, stderr strings.Builder
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("git log failed: %w\n%s", err, stderr.String())
	}

	result := make(map[string]int64, 2048)
	var currentTS int64

	scanner := bufio.NewScanner(strings.NewReader(stdout.String()))
	for scanner.Scan() {
		line := scanner.Text()
		if line == "" {
			continue
		}
		// Lines that are purely numeric are timestamps
		if ts, err := strconv.ParseInt(line, 10, 64); err == nil && !strings.Contains(line, "/") && !strings.Contains(line, ".") {
			currentTS = ts
			continue
		}
		// Otherwise it's a filename — record only the first (newest) timestamp
		if currentTS > 0 {
			if _, exists := result[line]; !exists {
				result[line] = currentTS
			}
		}
	}

	return result, nil
}

// findNewestSourceTimeBulk finds the most recent git-committed timestamp among
// source files in dir and its subdirectories (relative to rootDir), but stops
// recursing into any subdirectory that has its own CLAUDE.md. Uses the
// pre-built timestamps map instead of spawning git subprocesses.
func findNewestSourceTimeBulk(rootDir, dir string, claudeDirs map[string]bool, timestamps map[string]int64) int64 {
	var newest int64
	absDir := filepath.Join(rootDir, dir)

	_ = filepath.WalkDir(absDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			rel, _ := filepath.Rel(rootDir, path)
			// Skip subdirectories that have their own CLAUDE.md (but not the
			// root dir itself — that's the one we're checking).
			if rel != dir && claudeDirs[rel] {
				return filepath.SkipDir
			}
			name := d.Name()
			if strings.HasPrefix(name, ".") || stalenessSkipDirs[name] {
				return filepath.SkipDir
			}
			return nil
		}

		// Skip non-source files
		ext := filepath.Ext(d.Name())
		if !sourceExtensions[ext] {
			return nil
		}

		// Skip test files and generated files
		name := d.Name()
		if isTestOrGenerated(name) {
			return nil
		}

		// Skip CLAUDE.md files themselves
		if name == "CLAUDE.md" {
			return nil
		}

		rel, _ := filepath.Rel(rootDir, path)
		ts := timestamps[rel]
		if ts > newest {
			newest = ts
		}
		return nil
	})

	return newest
}

// isTestOrGenerated returns true if the filename looks like a test file or
// generated file that shouldn't count toward staleness.
func isTestOrGenerated(name string) bool {
	lower := strings.ToLower(name)

	// Test files
	if strings.HasSuffix(lower, "_test.go") ||
		strings.HasSuffix(lower, ".test.ts") ||
		strings.HasSuffix(lower, ".test.js") ||
		strings.HasSuffix(lower, ".spec.ts") ||
		strings.HasSuffix(lower, ".spec.js") {
		return true
	}

	// Generated files
	if strings.HasSuffix(lower, ".generated.ts") ||
		strings.HasSuffix(lower, ".generated.go") ||
		strings.HasSuffix(lower, ".gen.go") ||
		strings.HasSuffix(lower, ".pb.go") {
		return true
	}

	return false
}
