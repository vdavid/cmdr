package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// residentDocBudgetWords is the word cap for the unconditionally-resident
// agent-doc bundle: the repo-root CLAUDE.md, every file it transitively
// @-imports, and every project rule in .claude/rules/*.md. This bundle loads in
// EVERY agent session, worktree, and subagent, so each word is paid on every
// turn of every session. The cap is seeded at the measured total at creation
// time; it must only ever ratchet DOWN as we trim the docs, never up. Raising it
// needs explicit user consent (same discipline as the other allowlists). Word
// counting matches `wc -w`. Ratcheted from the original 9472 after the doc diet
// re-homed desktop ops out of the root and moved area rules to colocated docs.
const residentDocBudgetWords = 1982

// claudeImportRe captures @-import tokens in a CLAUDE.md. Claude Code treats a
// leading-@ token as a file import; we resolve each against the filesystem and
// keep only the ones that name a real file, which naturally drops npm package
// names (`@iconify-json/lucide`), JSDoc tags (`@param`), and emails
// (`@example.com`) that share the @-prefix shape but aren't imports.
var claudeImportRe = regexp.MustCompile(`@([A-Za-z0-9._][A-Za-z0-9._/-]*)`)

type residentDocEntry struct {
	relPath string
	words   int
}

// collectResidentDocs returns the resident bundle's files (repo-relative) in a
// stable order: the root CLAUDE.md, then its transitive @-imports in
// breadth-first order, then the sorted .claude/rules/*.md set. Each file appears
// once. A missing root CLAUDE.md yields an empty list (the check then reports 0).
func collectResidentDocs(rootDir string) ([]string, error) {
	var ordered []string
	seen := map[string]bool{}

	add := func(rel string) {
		if !seen[rel] {
			seen[rel] = true
			ordered = append(ordered, rel)
		}
	}

	root := "CLAUDE.md"
	if !fileExists(filepath.Join(rootDir, root)) {
		return nil, nil
	}
	add(root)

	// Breadth-first over @-imports. Each import path is resolved relative to the
	// importing file's directory first, then relative to the repo root; only an
	// import that resolves to an existing file is followed.
	for i := 0; i < len(ordered); i++ {
		cur := ordered[i]
		data, err := os.ReadFile(filepath.Join(rootDir, cur))
		if err != nil {
			return nil, err
		}
		for _, m := range claudeImportRe.FindAllStringSubmatch(string(data), -1) {
			importPath := m[1]
			resolved := resolveImport(rootDir, cur, importPath)
			if resolved != "" {
				add(resolved)
			}
		}
	}

	// The project rules are unconditionally resident regardless of imports.
	rulesDir := filepath.Join(rootDir, ".claude", "rules")
	rulePaths, err := rulesMarkdownFiles(rulesDir)
	if err != nil {
		return nil, err
	}
	for _, p := range rulePaths {
		rel, _ := filepath.Rel(rootDir, p)
		add(filepath.ToSlash(rel))
	}

	return ordered, nil
}

// resolveImport resolves an @-import path (relative to the importing file's
// directory first, then to the repo root) and returns the repo-relative path if
// it names an existing file, else "".
func resolveImport(rootDir, importer, importPath string) string {
	importerDir := filepath.Dir(importer)
	candidates := []string{
		filepath.Join(importerDir, filepath.FromSlash(importPath)),
		filepath.FromSlash(importPath),
	}
	for _, c := range candidates {
		clean := filepath.Clean(c)
		if strings.HasPrefix(clean, "..") {
			continue // never escape the repo root
		}
		if fileExists(filepath.Join(rootDir, clean)) {
			return filepath.ToSlash(clean)
		}
	}
	return ""
}

// rulesMarkdownFiles returns the sorted list of *.md files directly under the
// rules dir. A missing dir is not an error (yields no files).
func rulesMarkdownFiles(rulesDir string) ([]string, error) {
	entries, err := os.ReadDir(rulesDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var paths []string
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".md") {
			continue
		}
		paths = append(paths, filepath.Join(rulesDir, e.Name()))
	}
	sort.Strings(paths)
	return paths, nil
}

// RunResidentDocBudget sums the word counts of the unconditionally-resident
// agent-doc bundle (root CLAUDE.md + its transitive @-imports + .claude/rules/*.md)
// and warns when the total exceeds residentDocBudgetWords. Warn-only: this is a
// metric that guards against silent regrowth of the per-session token cost, like
// claude-md-length. Always succeeds (warn, never fails).
func RunResidentDocBudget(ctx *CheckContext) (CheckResult, error) {
	docs, err := collectResidentDocs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to collect resident docs: %w", err)
	}

	var entries []residentDocEntry
	total := 0
	for _, rel := range docs {
		words, wErr := countWords(filepath.Join(ctx.RootDir, rel))
		if wErr != nil {
			return CheckResult{}, fmt.Errorf("failed to count words in %s: %w", rel, wErr)
		}
		entries = append(entries, residentDocEntry{relPath: rel, words: words})
		total += words
	}

	if total <= residentDocBudgetWords {
		return Success(fmt.Sprintf("Resident agent-doc bundle is %d words (cap %d), across %d %s",
			total, residentDocBudgetWords, len(entries), Pluralize(len(entries), "file", "files"))), nil
	}

	return CheckResult{
		Code:    ResultWarning,
		Message: formatResidentDocBudget(entries, total),
		Total:   -1,
		Issues:  -1,
		Changes: -1,
	}, nil
}

// formatResidentDocBudget builds the over-budget warning: the total, the cap,
// the overage, and the per-file breakdown (largest first) so the reader sees
// where to trim.
func formatResidentDocBudget(entries []residentDocEntry, total int) string {
	sorted := make([]residentDocEntry, len(entries))
	copy(sorted, entries)
	sort.Slice(sorted, func(i, j int) bool {
		if sorted[i].words != sorted[j].words {
			return sorted[i].words > sorted[j].words
		}
		return sorted[i].relPath < sorted[j].relPath
	})

	var sb strings.Builder
	for _, e := range sorted {
		sb.WriteString(fmt.Sprintf("  - %s (%d words)\n", e.relPath, e.words))
	}
	overage := total - residentDocBudgetWords
	return fmt.Sprintf(
		"Resident agent-doc bundle is %s%d words%s, over the %d-word cap by %d "+
			"(this bundle loads in EVERY session; trim it, don't raise the cap — playbook: docs/doc-system.md):\n%s",
		ansiYellow, total, ansiReset, residentDocBudgetWords, overage,
		strings.TrimRight(sb.String(), "\n"))
}
