package checks

import (
	"fmt"
	"path"
	"regexp"
	"slices"
	"strconv"
	"strings"
)

// The "Translated into N languages" claim sits on a dozen public surfaces, and
// it went stale on most of them every time a catalog landed (listings said 15
// and the blog 15 while the app shipped 17). The website derives the number at
// build time (`apps/website/src/lib/shipped-languages.ts`); this check holds
// every surface the build can't reach to the same number.
//
// The truth is the `SHIPPED_LOCALES` table in the generated Rust file: the list
// the Settings language dropdown offers, without the `en-XA` pseudolocale, and
// kept equal to the catalog dirs by `shipped-locales-fresh`. Counting the dirs
// instead would count a local gitignored `en-XA/` too.

const shippedLocalesGenPath = "apps/desktop/src-tauri/src/intl/shipped_locales.gen.rs"

// languageCountRoots are the public copy surfaces, as git pathspecs. Agent docs
// (`docs/`, every CLAUDE.md / DETAILS.md) are out: they cite counts as history
// and evidence, not as claims to a reader.
var languageCountRoots = []string{
	"README.md",
	"brand/",
	"apps/website/src/",
	"apps/desktop/src/lib/intl/messages/en/",
	"tools/demo-video/src/",
}

// languageCountInputs is the check's fingerprint: the generated table plus every root it scans.
var languageCountInputs = []string{
	shippedLocalesGenPath,
	"README.md",
	"brand/**",
	"apps/website/src/**",
	"apps/desktop/src/lib/intl/messages/en/**",
	"tools/demo-video/src/**",
}

var (
	shippedLocalesTableRe = regexp.MustCompile(`(?s)SHIPPED_LOCALES: &\[ShippedLocale\] = &\[(.*?)\n\];`)
	shippedLocaleTagRe    = regexp.MustCompile(`(?m)^\s*tag: "[^"]+",$`)
	// languageCountRe matches a stated count: "17 languages", "Fifteen languages",
	// "a dozen languages", "10+ languages", and the blog table's
	// `15 <span title="languages">langs</span>`. Number words start at ten, since
	// a smaller one ("two languages") is never a claim about Cmdr.
	languageCountRe = gateRegexp(regexp.MustCompile(
		`\b(\d+|(?i:ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty|a dozen))\+?\s+(?:<[^>]*>\s*)?[Ll]ang(?:uage)?s\b`))
	// languageCountIgnoreRe is the per-line opt-out for a count that isn't Cmdr's
	// current one (a roadmap entry's "16 langs now", Total Commander's 45). It
	// names the numbers it excuses, so it can't hide a different, stale one.
	languageCountIgnoreRe = regexp.MustCompile(`language-count-ignore:\s*(\d+(?:\s*,\s*\d+)*)`)
)

var languageCountWords = map[string]int{
	"ten": 10, "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14, "fifteen": 15, "sixteen": 16,
	"seventeen": 17, "eighteen": 18, "nineteen": 19, "twenty": 20, "a dozen": 12,
}

// languageCountHit is one line that disagrees with the shipped count.
type languageCountHit struct {
	file    string
	line    int
	problem string
}

// RunCopyLanguageCount fails when public copy states a language count other
// than the number of catalogs the app ships.
func RunCopyLanguageCount(ctx *CheckContext) (CheckResult, error) {
	gen, present, err := readTrackedFile(ctx.RootDir, shippedLocalesGenPath)
	if err != nil {
		return CheckResult{}, err
	}
	if !present {
		return CheckResult{}, fmt.Errorf("%s is missing; run `pnpm intl:shipped-locales` in apps/desktop", shippedLocalesGenPath)
	}
	shipped, err := countShippedLocales(gen)
	if err != nil {
		return CheckResult{}, err
	}

	files, err := corpus.GitLsFiles(ctx.RootDir, append([]string{"--"}, languageCountRoots...)...)
	if err != nil {
		return CheckResult{}, err
	}
	var hits []languageCountHit
	scanned, claims := 0, 0
	for _, rel := range files {
		if !isLanguageCountCopyFile(rel) {
			continue
		}
		text, ok, readErr := readTrackedFile(ctx.RootDir, rel)
		if readErr != nil {
			return CheckResult{}, readErr
		}
		if !ok {
			continue
		}
		scanned++
		fileClaims, fileHits := scanLanguageCounts(rel, text, shipped)
		claims += fileClaims
		hits = append(hits, fileHits...)
	}

	if len(hits) > 0 {
		var sb strings.Builder
		for _, h := range hits {
			fmt.Fprintf(&sb, "%s:%d: %s\n", h.file, h.line, h.problem)
		}
		return CheckResult{}, fmt.Errorf(
			"%d %s disagree with the %d languages the app ships (%s):\n%s\n"+
				"Say %d, or derive it where a build can (`apps/website/src/lib/shipped-languages.ts`). A count that "+
				"isn't Cmdr's current one (history, another product) takes a same-line `language-count-ignore: <n>` "+
				"comment naming that number",
			len(hits), Pluralize(len(hits), "line", "lines"), shipped, shippedLocalesGenPath,
			indentOutput(strings.TrimRight(sb.String(), "\n")), shipped)
	}
	return Success(fmt.Sprintf("%d language %s across %d %s match the %d shipped languages",
		claims, Pluralize(claims, "count", "counts"), scanned, Pluralize(scanned, "file", "files"), shipped)), nil
}

// isLanguageCountCopyFile keeps the text files a reader sees, dropping images
// and the agent docs that sit beside the copy (`brand/CLAUDE.md`).
func isLanguageCountCopyFile(rel string) bool {
	switch path.Base(rel) {
	case "CLAUDE.md", "DETAILS.md":
		return false
	}
	switch path.Ext(rel) {
	case ".md", ".mdx", ".astro", ".ts", ".tsx", ".js", ".json", ".svelte", ".html", ".txt":
		return true
	}
	return false
}

// countShippedLocales counts the entries of the generated `SHIPPED_LOCALES` table.
func countShippedLocales(gen string) (int, error) {
	table := shippedLocalesTableRe.FindStringSubmatch(gen)
	if table == nil {
		return 0, fmt.Errorf("%s has no SHIPPED_LOCALES table; did gen-shipped-locales.ts change shape?", shippedLocalesGenPath)
	}
	n := len(shippedLocaleTagRe.FindAllString(table[1], -1))
	if n == 0 {
		return 0, fmt.Errorf("%s lists no locales; did gen-shipped-locales.ts change shape?", shippedLocalesGenPath)
	}
	return n, nil
}

// scanLanguageCounts returns how many counts a file states that the check
// accepted, and every line where a count disagrees with `shipped` or an
// ignore marker excuses a number the line doesn't state.
func scanLanguageCounts(rel, text string, shipped int) (int, []languageCountHit) {
	claims := 0
	var hits []languageCountHit
	for i, line := range strings.Split(text, "\n") {
		var ignored []int
		if m := languageCountIgnoreRe.FindStringSubmatch(line); m != nil {
			for _, s := range strings.Split(m[1], ",") {
				n, _ := strconv.Atoi(strings.TrimSpace(s))
				ignored = append(ignored, n)
			}
		}
		var stated []int
		if languageCountRe.passes(line) {
			for _, m := range languageCountRe.re.FindAllStringSubmatch(line, -1) {
				stated = append(stated, languageCountValue(m[1]))
			}
		}
		for _, n := range stated {
			switch {
			case n == shipped:
				claims++
			case slices.Contains(ignored, n):
			default:
				hits = append(hits, languageCountHit{rel, i + 1, fmt.Sprintf("says %d languages", n)})
			}
		}
		for _, n := range ignored {
			if !slices.Contains(stated, n) {
				hits = append(hits, languageCountHit{rel, i + 1,
					fmt.Sprintf("`language-count-ignore: %d` excuses a count this line doesn't state; remove it", n)})
			}
		}
	}
	return claims, hits
}

// languageCountValue turns a matched count ("17", "Fifteen", "a dozen") into a number.
func languageCountValue(s string) int {
	if n, err := strconv.Atoi(s); err == nil {
		return n
	}
	return languageCountWords[strings.ToLower(s)]
}
