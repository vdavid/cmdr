package main

import (
	"path/filepath"
	"strings"
)

type fileStats struct {
	total    int
	rust     int
	rustProd int
	rustTest int
	ts       int
	tsProd   int
	tsTest   int
	svelte   int
	astro    int
	goTotal  int
	css      int
	docs     int
	other    int
	comments []string
}

func (s *fileStats) copyWithoutComments() *fileStats {
	return &fileStats{
		total:    s.total,
		rust:     s.rust,
		rustProd: s.rustProd,
		rustTest: s.rustTest,
		ts:       s.ts,
		tsProd:   s.tsProd,
		tsTest:   s.tsTest,
		svelte:   s.svelte,
		astro:    s.astro,
		goTotal:  s.goTotal,
		css:      s.css,
		docs:     s.docs,
		other:    s.other,
	}
}

// skipPatterns lists filenames and globs to exclude from counting.
// Exact names and filepath.Match-style wildcards are supported.
var skipPatterns = []string{
	// Lock/generated files
	"pnpm-lock.yaml",
	"package-lock.json",
	"yarn.lock",
	"Cargo.lock",
	"go.sum",
	// Binary files (line counting is meaningless)
	"*.png",
	"*.ico",
	"*.icns",
	"*.woff2",
	"*.lottie",
}

func shouldSkip(file string) bool {
	base := filepath.Base(file)
	for _, pattern := range skipPatterns {
		if matched, _ := filepath.Match(pattern, base); matched {
			return true
		}
	}
	return false
}

type category int

const (
	catRustProd category = iota
	catRustTest
	catTSProd
	catTSTest
	catSvelte
	catAstro
	catGo
	catCSS
	catDocs
	catOther
)

// simpleExtToCategory maps extensions that don't need test/prod distinction.
var simpleExtToCategory = map[string]category{
	".svelte": catSvelte,
	".astro":  catAstro,
	".go":     catGo,
	".css":    catCSS,
	".scss":   catCSS,
	".md":     catDocs,
}

// tsExtensions is the set of TypeScript/JavaScript file extensions.
var tsExtensions = map[string]bool{
	".ts": true, ".tsx": true, ".js": true, ".jsx": true, ".mjs": true, ".cjs": true,
}

func categorizeFile(file string) category {
	ext := strings.ToLower(filepath.Ext(file))
	base := filepath.Base(file)

	if cat, ok := simpleExtToCategory[ext]; ok {
		return cat
	}
	if base == "LICENSE" {
		return catDocs
	}
	if ext == ".rs" {
		return categorizeRust(file)
	}
	if tsExtensions[ext] {
		return categorizeTypeScript(file, base)
	}
	return catOther
}

func categorizeRust(file string) category {
	if isTestPath(file) {
		return catRustTest
	}
	return catRustProd
}

func categorizeTypeScript(file, base string) category {
	if isTestFilename(base) || isTestPath(file) {
		return catTSTest
	}
	return catTSProd
}

func isTestFilename(base string) bool {
	return strings.HasSuffix(base, ".test.ts") || strings.HasSuffix(base, ".test.tsx") ||
		strings.HasSuffix(base, ".spec.ts") || strings.HasSuffix(base, ".spec.tsx")
}

// isTestPath checks if a file lives under a test/tests/e2e directory.
func isTestPath(file string) bool {
	for part := range strings.SplitSeq(file, "/") {
		switch part {
		case "test", "tests", "__tests__", "e2e", "testutil", "testdata":
			return true
		}
	}
	return false
}

// countRustTestLines counts lines inside #[cfg(test)] blocks using brace-depth tracking.
func countRustTestLines(content string) int {
	testLines := 0
	depth := 0
	inTestBlock := false

	for line := range strings.SplitSeq(content, "\n") {
		trimmed := strings.TrimSpace(line)

		if !inTestBlock && strings.Contains(trimmed, "#[cfg(test)]") {
			inTestBlock = true
			testLines++
			// The opening brace may be on this same line
			depth += strings.Count(line, "{") - strings.Count(line, "}")
			continue
		}

		if inTestBlock {
			testLines++
			depth += strings.Count(line, "{") - strings.Count(line, "}")
			if depth <= 0 {
				inTestBlock = false
				depth = 0
			}
		}
	}

	return testLines
}

func countLinesForCommit(commitHash string, messages []string) (*fileStats, error) {
	stats := &fileStats{comments: messages}

	files, err := getFilesAtCommit(commitHash)
	if err != nil {
		return nil, err
	}

	// Collect blobs for non-skipped files
	var wanted []fileEntry
	for _, f := range files {
		if !shouldSkip(f.path) {
			wanted = append(wanted, f)
		}
	}

	blobs := make([]string, len(wanted))
	for i, f := range wanted {
		blobs[i] = f.blob
	}

	contents, err := batchGetFileContents(blobs)
	if err != nil {
		return nil, err
	}

	for _, f := range wanted {
		content, ok := contents[f.blob]
		if !ok {
			continue // binary or missing
		}
		accumulateFileStats(stats, f.path, content)
	}

	return stats, nil
}

func accumulateFileStats(stats *fileStats, path, content string) {
	lines := countLines(content)
	stats.total += lines
	cat := categorizeFile(path)

	// For Rust prod files, split inline #[cfg(test)] lines from prod lines.
	if cat == catRustProd {
		testLines := countRustTestLines(content)
		stats.rustProd += lines - testLines
		stats.rustTest += testLines
		stats.rust += lines
		return
	}

	switch cat {
	case catRustTest:
		stats.rustTest += lines
		stats.rust += lines
	case catTSProd:
		stats.tsProd += lines
		stats.ts += lines
	case catTSTest:
		stats.tsTest += lines
		stats.ts += lines
	case catSvelte:
		stats.svelte += lines
	case catAstro:
		stats.astro += lines
	case catGo:
		stats.goTotal += lines
	case catCSS:
		stats.css += lines
	case catDocs:
		stats.docs += lines
	case catOther:
		stats.other += lines
	}
}
