package checks

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// AllowStaleSelectorComment opts one line's selectors out of the check. It goes at the
// end of the flagged line or on a comment-only line directly above, with a reason:
//
//	// allowed-stale-selector: the class comes from a third-party widget's own markup
//	await page.click('.vendor-row')
const AllowStaleSelectorComment = "// allowed-stale-selector:"

// e2eSelScanDirs are the test trees that write selectors against the app's DOM. The
// Linux E2E lane has no specs of its own: it runs these same ones under Docker.
var e2eSelScanDirs = []string{
	"apps/desktop/test/e2e-playwright",
	"apps/desktop/test/e2e-shared",
}

// e2eSelSourceDir holds every class and data-* attribute the app renders.
const e2eSelSourceDir = "apps/desktop/src"

type e2eStaleSelectorHit struct {
	relPath    string
	line       int
	token      e2eSelToken
	literal    string
	reasonless bool // an opt-out covers it but gives no reason
}

type e2eStaleSelectorReport struct {
	hits      []e2eStaleSelectorHit
	orphans   []orphanDirective
	files     int
	selectors int
}

// RunE2EStaleSelector fails when a Playwright selector names a class or data-* attribute
// that exists nowhere in the frontend source. UI changes break selectors silently in
// code nothing runs routinely (the i18n capture waited five days on a removed row), and
// a missing token is visible statically the day the UI changes. The lexer and selector
// parser live in `e2e-stale-selector-parse.go`.
func RunE2EStaleSelector(ctx *CheckContext) (CheckResult, error) {
	vocab, err := loadE2ESelVocabulary(filepath.Join(ctx.RootDir, filepath.FromSlash(e2eSelSourceDir)))
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to read the frontend source: %w", err)
	}
	report, err := scanE2EStaleSelectors(ctx.RootDir, vocab)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan the E2E tests: %w", err)
	}

	var parts []string
	if len(report.hits) > 0 {
		parts = append(parts, formatE2EStaleSelectorHits(report.hits))
	}
	if len(report.orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowStaleSelectorComment, report.orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}
	return Success(fmt.Sprintf(
		"%d %s in %d test %s name only classes and data-* attributes the app source has",
		report.selectors, Pluralize(report.selectors, "selector", "selectors"),
		report.files, Pluralize(report.files, "file", "files"),
	)), nil
}

func formatE2EStaleSelectorHits(hits []e2eStaleSelectorHit) string {
	sort.Slice(hits, func(i, j int) bool {
		if hits[i].relPath != hits[j].relPath {
			return hits[i].relPath < hits[j].relPath
		}
		if hits[i].line != hits[j].line {
			return hits[i].line < hits[j].line
		}
		return hits[i].token.name < hits[j].token.name
	})
	var sb strings.Builder
	sb.WriteString(fmt.Sprintf(
		"found %d selector %s naming a class or data-* attribute that appears nowhere in %s, so the UI "+
			"likely changed under the test. Point each at what the app renders now. A token the source can't "+
			"spell (a third-party widget's own markup) opts out with `%s <reason>` at the end of the line or "+
			"on the line above:\n",
		len(hits), Pluralize(len(hits), "token", "tokens"), e2eSelSourceDir, AllowStaleSelectorComment,
	))
	for _, h := range hits {
		sb.WriteString(fmt.Sprintf("  %s:%d: %s `%s` in `%s`", h.relPath, h.line, h.token.kind, h.token.name, h.literal))
		if h.reasonless {
			sb.WriteString(" (its opt-out needs a reason)")
		}
		sb.WriteString("\n")
	}
	return strings.TrimRight(sb.String(), "\n")
}

// ---- vocabulary ----

// e2eSelDatasetRegex finds `dataset.fooBar`, which renders as `data-foo-bar`.
var e2eSelDatasetRegex = regexp.MustCompile(`\bdataset\.([A-Za-z][A-Za-z0-9]*)`)

// loadE2ESelVocabulary collects every whole word in the frontend source, where a word
// may contain `-` and `_`. Classes reach the DOM through `class="…"`, `class:foo`,
// `classList`, template strings, and CSS, and all of them spell the class as one such
// word, so presence is the bar rather than a model of every way to set one.
func loadE2ESelVocabulary(srcDir string) (map[string]bool, error) {
	vocab := make(map[string]bool)
	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			if d.Name() == "node_modules" {
				return filepath.SkipDir
			}
			return nil
		}
		if !isE2ESelVocabularyFile(d.Name()) {
			return nil
		}
		body, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		addE2ESelVocabulary(vocab, string(body))
		return nil
	})
	return vocab, err
}

func isE2ESelVocabularyFile(name string) bool {
	// Component tests count on purpose: they mount real components, Ark primitives
	// included, so they're where library-rendered markup like `data-value` gets spelled,
	// and they fail in `svelte-tests` if that markup goes away.
	switch filepath.Ext(name) {
	case ".svelte", ".ts", ".js", ".css", ".html":
		return true
	}
	return false
}

func addE2ESelVocabulary(vocab map[string]bool, body string) {
	for _, word := range strings.FieldsFunc(body, func(r rune) bool { return !isE2ESelWordRune(r) }) {
		vocab[word] = true
	}
	for _, m := range e2eSelDatasetRegex.FindAllStringSubmatch(body, -1) {
		var sb strings.Builder
		sb.WriteString("data")
		for _, r := range m[1] {
			if r >= 'A' && r <= 'Z' {
				sb.WriteByte('-')
				r += 'a' - 'A'
			} else if sb.Len() == len("data") {
				sb.WriteByte('-')
			}
			sb.WriteRune(r)
		}
		vocab[sb.String()] = true
	}
}

func isE2ESelWordRune(r rune) bool {
	return r == '-' || r == '_' || (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')
}

// ---- scan ----

func scanE2EStaleSelectors(rootDir string, vocab map[string]bool) (e2eStaleSelectorReport, error) {
	var report e2eStaleSelectorReport
	found := 0
	for _, dir := range e2eSelScanDirs {
		abs := filepath.Join(rootDir, filepath.FromSlash(dir))
		if _, err := os.Stat(abs); errors.Is(err, fs.ErrNotExist) {
			continue
		}
		found++
		err := filepath.WalkDir(abs, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				switch d.Name() {
				case "node_modules", "test-results", "playwright-report":
					return filepath.SkipDir
				}
				return nil
			}
			// A Vitest `*.test.ts` builds its own happy-dom tree, so its classes owe
			// nothing to the app's source.
			if !strings.HasSuffix(d.Name(), ".ts") || strings.HasSuffix(d.Name(), ".test.ts") {
				return nil
			}
			body, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			relPath, relErr := filepath.Rel(rootDir, path)
			if relErr != nil {
				relPath = path
			}
			report.files++
			scanE2ESelFile(&report, filepath.ToSlash(relPath), string(body), vocab)
			return nil
		})
		if err != nil {
			return report, err
		}
	}
	if found == 0 {
		return report, fmt.Errorf("none of %s exists", strings.Join(e2eSelScanDirs, ", "))
	}
	return report, nil
}

func scanE2ESelFile(report *e2eStaleSelectorReport, relPath, body string, vocab map[string]bool) {
	lines := strings.Split(body, "\n")
	tracker := newDirectiveTracker(AllowStaleSelectorComment, "//")
	for i, line := range lines {
		tracker.observe(i+1, line)
	}
	seen := make(map[string]bool)
	for _, lit := range lexE2ESelLiterals(body) {
		tokens := e2eSelTokens(lit.text)
		if tokens == nil {
			continue
		}
		report.selectors++
		for _, tok := range tokens {
			key := fmt.Sprintf("%d %s %s", lit.line, tok.kind, tok.name)
			if vocab[tok.name] || seen[key] {
				continue
			}
			seen[key] = true
			hit := e2eStaleSelectorHit{
				relPath: relPath,
				line:    lit.line,
				token:   tok,
				literal: strings.ReplaceAll(lit.text, string(e2eSelInterpolation), "${…}"),
			}
			if directiveLine, reason := e2eSelOptOut(lines, lit.line); directiveLine > 0 {
				tracker.markLineUsed(directiveLine)
				if reason != "" {
					continue
				}
				hit.reasonless = true
			}
			report.hits = append(report.hits, hit)
		}
	}
	report.orphans = append(report.orphans, tracker.orphans(relPath)...)
}

// e2eSelOptOut finds the opt-out covering lineNum: a trailing comment on the line
// itself, or a comment-only line directly above. A trailing opt-out on the line above
// belongs to that line's own selectors. It returns the directive's line (0 when none)
// and its reason.
func e2eSelOptOut(lines []string, lineNum int) (int, string) {
	if lineNum >= 1 && lineNum <= len(lines) {
		line := lines[lineNum-1]
		if idx := strings.Index(line, AllowStaleSelectorComment); idx >= 0 {
			return lineNum, strings.TrimSpace(line[idx+len(AllowStaleSelectorComment):])
		}
	}
	if lineNum >= 2 && lineNum-1 <= len(lines) {
		prev := strings.TrimSpace(lines[lineNum-2])
		if strings.HasPrefix(prev, AllowStaleSelectorComment) {
			return lineNum - 1, strings.TrimSpace(prev[len(AllowStaleSelectorComment):])
		}
	}
	return 0, ""
}
