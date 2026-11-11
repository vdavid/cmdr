package checks

import (
	"strings"
	"testing"
)

func TestScanLanguageCounts(t *testing.T) {
	tests := []struct {
		name       string
		line       string
		wantClaims int
		wantHits   []string // each hit's problem text
	}{
		{"matching digits", "Translated into 17 languages.", 1, nil},
		{"stale digits", "- Localization: Translated into 15 languages.", 0, []string{"says 15 languages"}},
		{"spelled out, capitalized", "{ say: 'Fifteen languages,' }", 0, []string{"says 15 languages"}},
		{"spelled out, matching", "Seventeen languages", 1, nil},
		{"a dozen", "ArrowDown through a dozen languages", 0, []string{"says 12 languages"}},
		{"short form", "Cmdr has 16 langs now.", 0, []string{"says 16 languages"}},
		{"plus suffix", "in 10+ languages", 0, []string{"says 10 languages"}},
		{"blog table span", `:yes: Yes (15 <span title="languages">langs</span>)`, 0, []string{"says 15 languages"}},
		{"small number word is not a claim", "the two languages agree", 0, nil},
		{"no count", "Speaks your language", 0, nil},
		{"natural-language is not a count", "natural-language search in 3 steps", 0, nil},
		{"ignored history", "Cmdr has 16 langs now. // language-count-ignore: 16", 0, nil},
		{
			"ignore excuses only its own number",
			`(~45 <span title="languages">langs</span>) | (15 <span title="languages">langs</span>) <!-- language-count-ignore: 45 -->`,
			0, []string{"says 15 languages"},
		},
		{
			"ignore beside a correct count",
			`(~45 <span>langs</span>) | (17 <span>langs</span>) <!-- language-count-ignore: 45 -->`,
			1, nil,
		},
		{
			"stale ignore marker",
			"Translated into 17 languages. <!-- language-count-ignore: 15 -->",
			1, []string{"`language-count-ignore: 15` excuses a count this line doesn't state; remove it"},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			claims, hits := scanLanguageCounts("f.md", tt.line, 17)
			if claims != tt.wantClaims {
				t.Errorf("claims = %d, want %d", claims, tt.wantClaims)
			}
			var got []string
			for _, h := range hits {
				got = append(got, h.problem)
			}
			if strings.Join(got, "|") != strings.Join(tt.wantHits, "|") {
				t.Errorf("hits = %q, want %q", got, tt.wantHits)
			}
		})
	}
}

func TestCountShippedLocales(t *testing.T) {
	gen := "pub(crate) const SHIPPED_LOCALES: &[ShippedLocale] = &[\n" +
		"    ShippedLocale {\n        tag: \"cs\",\n        script: \"latn\",\n    },\n" +
		"    ShippedLocale {\n        tag: \"en-GB\",\n        covers: &[\"en-001\"],\n    },\n" +
		"];\n\npub(crate) const PARENT_LOCALES: &[(&str, &str)] = &[\n    (\"en-150\", \"en-001\"),\n];\n"
	n, err := countShippedLocales(gen)
	if err != nil || n != 2 {
		t.Fatalf("countShippedLocales = %d, %v; want 2, nil", n, err)
	}
	if _, err := countShippedLocales("// no table here\n"); err == nil {
		t.Fatal("want an error for a file with no SHIPPED_LOCALES table")
	}
}

func TestIsLanguageCountCopyFile(t *testing.T) {
	for rel, want := range map[string]bool{
		"README.md":                       true,
		"brand/listings/alternativeto.md": true,
		"apps/website/src/lib/roadmap.ts": true,
		"apps/website/src/pages/x.astro":  true,
		"brand/CLAUDE.md":                 false,
		"brand/DETAILS.md":                false,
		"brand/screenshots/app-main.webp": false,
		"brand/logos/cmdr-512.png":        false,
	} {
		if got := isLanguageCountCopyFile(rel); got != want {
			t.Errorf("isLanguageCountCopyFile(%q) = %v, want %v", rel, got, want)
		}
	}
}
