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

// AllowPluralizeNounComment opts a single line out of the pluralize-noun
// check. Place it on the flagged line as a trailing comment, or on the line
// immediately above.
//
//	// allowed-pluralize-noun: structured key=value log fields, not a count + noun
const AllowPluralizeNounComment = "// allowed-pluralize-noun:"

// allowPluralizeNounCommentTs is the TS/Svelte spelling of the opt-out marker.
const allowPluralizeNounCommentTs = "// allowed-pluralize-noun:"

// pluralizeNounSuffixRe matches a `{varname}` interpolation immediately
// followed by a single space and a word ending in `s`. Catches the common
// `"{count} files"` / `format!("{n} bytes ...")` shape that reads as
// `1 files` / `1 bytes` when the count is 1.
//
// We intentionally only match the simple "+s" plural here; irregulars like
// `entries`, `directories`, `branches` slip through, matching the user's
// regex `[a-zA-Z]\} [a-zA-Z][-a-zA-Z]+s\b`. Capture group 1 holds the noun
// so we can filter common non-plural false positives below.
var pluralizeNounSuffixRe = regexp.MustCompile(`[a-zA-Z]\} ([a-zA-Z][-a-zA-Z]+s)\b`)

// pluralizeNounNonPluralWords are English words that end in `s` but aren't
// plural nouns (verbs, copulas, demonstratives). Catches the most common
// false positives so the check stays useful without manual opt-outs.
var pluralizeNounNonPluralWords = map[string]bool{
	"was":      true,
	"is":       true,
	"has":      true,
	"this":     true,
	"his":      true,
	"its":      true,
	"plus":     true,
	"minus":    true,
	"thus":     true,
	"yes":      true,
	"exceeds":  true,
	"starts":   true,
	"ends":     true,
	"prints":   true,
	"returns":  true,
	"yields":   true,
	"contains": true,
	"matches":  true,
	"reads":    true,
	"writes":   true,
	"emits":    true,
	"fires":    true,
	"goes":     true,
	"does":     true,
	"says":     true,
	"shows":    true,
}

// pluralizeNounAllowSubstrings filters out obvious false positives. These
// patterns match against the slice of text starting at the `{...}` and
// extending a few characters past the noun, so the check stays close to the
// user's regex without flagging every CSS / Svelte attribute / debug
// key=value log shape.
//
// The shape we're trying to flag is natural-language "{N} somethings".
// Everything else in the list below is something that *looks* like that to a
// regex but isn't actually a count + noun.
var pluralizeNounAllowSubstrings = []*regexp.Regexp{
	// Svelte attribute bindings: `{onClick} class="..."`, `class:foo={bar}`.
	regexp.MustCompile(`\bclass="`),
	regexp.MustCompile(`\bclass:`),
	regexp.MustCompile(`\bbind:`),
	regexp.MustCompile(`\baria-`),
	regexp.MustCompile(`\bon[A-Za-z]+=`),
	regexp.MustCompile(`\bon[a-z]+=`),
	regexp.MustCompile(`\bitem=`),
	regexp.MustCompile(`\bchecked=`),
	regexp.MustCompile(`\btype=`),
	regexp.MustCompile(`\bhref=`),
	regexp.MustCompile(`\bdata-`),
	regexp.MustCompile(`\bstyle="`),
	regexp.MustCompile(`\b(stash|reflog)@\{`),
}

type pluralizeNounSite struct {
	relPath string
	line    int
	text    string
}

// RunPluralizeNoun fails the build if any source file matches the
// `{var} somethings` shape (a count interpolation followed by a regular
// `+s` plural). Callers should use the shared `pluralize(count, "thing")`
// helper instead so the form reads correctly when the count is 1.
func RunPluralizeNoun(ctx *CheckContext) (CheckResult, error) {
	roots := []struct {
		dir  string
		exts []string
	}{
		{filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src"), []string{".rs"}},
		{filepath.Join(ctx.RootDir, "apps", "desktop", "src"), []string{".ts", ".svelte"}},
		{filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "tests"), []string{".rs"}},
		{filepath.Join(ctx.RootDir, "tools"), []string{".rs"}},
	}

	var allViolations []pluralizeNounSite
	scanned := 0
	for _, root := range roots {
		violations, count, err := scanForPluralizeNoun(ctx.RootDir, root.dir, root.exts)
		if err != nil {
			return CheckResult{}, fmt.Errorf("scan %s: %w", root.dir, err)
		}
		allViolations = append(allViolations, violations...)
		scanned += count
	}

	if len(allViolations) > 0 {
		sort.Slice(allViolations, func(i, j int) bool {
			if allViolations[i].relPath == allViolations[j].relPath {
				return allViolations[i].line < allViolations[j].line
			}
			return allViolations[i].relPath < allViolations[j].relPath
		})
		var sb strings.Builder
		for _, v := range allViolations {
			sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", v.relPath, v.line, v.text))
		}
		return CheckResult{}, fmt.Errorf(
			"found %d %s of `{var} <plural>` patterns that read as `1 files` when the count is 1 "+
				"(use the shared `pluralize(count, \"thing\")` helper from "+
				"`crate::pluralize` / `$lib/utils/pluralize`; add `%s <reason>` on the line if a false positive):\n%s",
			len(allViolations), Pluralize(len(allViolations), "site", "sites"), AllowPluralizeNounComment, sb.String(),
		)
	}

	return Success(fmt.Sprintf(
		"%d %s scanned, no `{var} <plural>` patterns",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForPluralizeNoun(rootDir, srcDir string, exts []string) ([]pluralizeNounSite, int, error) {
	var violations []pluralizeNounSite
	scanned := 0

	// Skip if the root doesn't exist (e.g. tools/ in a fresh worktree).
	if _, err := os.Stat(srcDir); os.IsNotExist(err) {
		return nil, 0, nil
	}

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if skip, action := pluralizeNounWalkAction(d, exts); skip {
			return action
		}
		scanned++

		fileViolations, scanErr := scanPluralizeNounFile(rootDir, path)
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		return nil
	})

	return violations, scanned, err
}

// pluralizeNounWalkAction reports whether to skip the entry. For directories
// to skip recursion, the action is `filepath.SkipDir`; for files that don't
// match the wanted extensions, the action is `nil` (skip this entry only).
func pluralizeNounWalkAction(d os.DirEntry, exts []string) (skip bool, action error) {
	if d.IsDir() {
		name := d.Name()
		if name == "node_modules" || name == "target" || name == ".svelte-kit" {
			return true, filepath.SkipDir
		}
		return true, nil
	}
	for _, ext := range exts {
		if strings.HasSuffix(d.Name(), ext) {
			return false, nil
		}
	}
	return true, nil
}

func scanPluralizeNounFile(rootDir, path string) ([]pluralizeNounSite, error) {
	relPath, relErr := filepath.Rel(rootDir, path)
	if relErr != nil {
		relPath = path
	}

	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, openErr
	}
	defer f.Close()

	var violations []pluralizeNounSite
	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)
	var prev string
	lineNum := 0
	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		if isLikelyCommentLine(line) {
			prev = line
			continue
		}

		matches := pluralizeNounSuffixRe.FindAllStringSubmatchIndex(line, -1)
		if len(matches) == 0 {
			prev = line
			continue
		}

		if hasAllowPluralizeNounComment(prev) || hasAllowPluralizeNounComment(line) {
			prev = line
			continue
		}

		if !anyRealPluralizeNounHit(line, matches) {
			prev = line
			continue
		}

		violations = append(violations, pluralizeNounSite{
			relPath: relPath,
			line:    lineNum,
			text:    strings.TrimSpace(line),
		})
		prev = line
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}
	return violations, nil
}

// isLikelyCommentLine drops single-line `//` comments and JSDoc / Rust-style
// `/** … */` continuation lines so flagged matches are real format strings.
func isLikelyCommentLine(line string) bool {
	trimmed := strings.TrimLeft(line, " \t")
	return strings.HasPrefix(trimmed, "//") ||
		strings.HasPrefix(trimmed, "* ") ||
		strings.HasPrefix(trimmed, "/*") ||
		strings.HasPrefix(trimmed, "*/") ||
		trimmed == "*"
}

// anyRealPluralizeNounHit reports whether any of the regex matches on `line`
// is a real `{count} <plural>` site after filtering for non-plural English
// words, structured `key=value` shapes, and surrounding-context false
// positives like Svelte attribute bindings and the git `stash@{n}` shorthand.
func anyRealPluralizeNounHit(line string, matches [][]int) bool {
	for _, m := range matches {
		if isRealPluralizeNounMatch(line, m) {
			return true
		}
	}
	return false
}

func isRealPluralizeNounMatch(line string, m []int) bool {
	noun := line[m[2]:m[3]]
	// Structured `key=value`: noun is followed by `=`.
	if m[3] < len(line) && line[m[3]] == '=' {
		return false
	}
	if pluralizeNounNonPluralWords[strings.ToLower(noun)] {
		return false
	}
	end := min(m[1]+40, len(line))
	if isPluralizeNounFalsePositive(line[m[0]:end]) {
		return false
	}
	// `stash@{n} prints` — the `@{` lives BEFORE the regex match, so the
	// surrounding-context filter misses it. Look behind a few chars.
	before := line[max(0, m[0]-5) : m[0]+1]
	return !strings.Contains(before, "@{")
}

func isPluralizeNounFalsePositive(slice string) bool {
	for _, re := range pluralizeNounAllowSubstrings {
		if re.MatchString(slice) {
			return true
		}
	}
	return false
}

func hasAllowPluralizeNounComment(line string) bool {
	return strings.Contains(line, AllowPluralizeNounComment) ||
		strings.Contains(line, allowPluralizeNounCommentTs)
}
