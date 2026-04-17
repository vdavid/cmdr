package main

import (
	"slices"
	"strings"
	"testing"
)

const sampleAppCSS = `
:root {
  --color-bg-primary: #ffffff;
  --color-text-primary: #1a1a1a;
  --color-accent: #d4a006;
}

@media (prefers-color-scheme: dark) {
  :root {
    --color-bg-primary: #1e1e1e;
    --color-text-primary: #e8e8e8;
    --color-accent: #ffc206;
  }
}
`

func TestParseAppCSS(t *testing.T) {
	vt := ParseAppCSS(sampleAppCSS)
	if vt.Light["color-bg-primary"] != "#ffffff" {
		t.Errorf("light bg-primary = %q", vt.Light["color-bg-primary"])
	}
	if vt.Dark["color-bg-primary"] != "#1e1e1e" {
		t.Errorf("dark bg-primary = %q", vt.Dark["color-bg-primary"])
	}
	if vt.Light["color-accent"] != "#d4a006" {
		t.Errorf("light accent = %q", vt.Light["color-accent"])
	}
	if vt.Dark["color-accent"] != "#ffc206" {
		t.Errorf("dark accent = %q", vt.Dark["color-accent"])
	}
}

const sampleSvelte = `<script>let x = 1;</script>
<div class="foo">hi</div>
<style>
  .foo {
    color: var(--color-text-primary);
    background: var(--color-bg-primary);
    font-size: var(--font-size-md);
  }

  .bar.baz::placeholder {
    color: var(--color-text-tertiary);
  }

  @media (prefers-reduced-motion: reduce) {
    .foo { color: red; }
  }

  .big {
    font-size: 24px;
    font-weight: bold;
    color: #666;
    background: #f5f5f5;
  }
</style>
`

func TestParseSvelteFile(t *testing.T) {
	pf := ParseSvelteFile("sample.svelte", sampleSvelte)
	// Expect: .foo, .bar.baz::placeholder (primary class `baz`), .foo (inside media), .big
	if len(pf.Rules) < 3 {
		t.Fatalf("expected >=3 rules, got %d: %+v", len(pf.Rules), pf.Rules)
	}

	foo := findRule(pf.Rules, "foo", "")
	if foo == nil {
		t.Fatal("missing .foo rule")
	}
	if foo.Color != "var(--color-text-primary)" {
		t.Errorf("foo color = %q", foo.Color)
	}
	if foo.Background != "var(--color-bg-primary)" {
		t.Errorf("foo bg = %q", foo.Background)
	}
	if foo.FontSizePx != 14 {
		t.Errorf("foo font px = %v", foo.FontSizePx)
	}

	placeholder := findRule(pf.Rules, "baz", "placeholder")
	if placeholder == nil {
		// `.bar.baz::placeholder` — classes should be [bar baz], pseudo=placeholder
		for _, r := range pf.Rules {
			if strings.Contains(r.Selector, "placeholder") {
				placeholder = &r
				break
			}
		}
	}
	if placeholder == nil {
		t.Fatal("missing placeholder rule")
	}
	if placeholder.Pseudo != "placeholder" {
		t.Errorf("pseudo = %q", placeholder.Pseudo)
	}

	big := findRule(pf.Rules, "big", "")
	if big == nil {
		t.Fatal("missing .big rule")
	}
	if big.FontSizePx != 24 {
		t.Errorf("big px = %v", big.FontSizePx)
	}
	if big.FontWeight != 700 {
		t.Errorf("big weight = %v", big.FontWeight)
	}
}

func findRule(rules []Rule, class, pseudo string) *Rule {
	for i := range rules {
		r := rules[i]
		if r.Pseudo != pseudo {
			continue
		}
		if slices.Contains(r.Classes, class) {
			return &rules[i]
		}
	}
	return nil
}

func TestSplitSelectorList(t *testing.T) {
	got := splitSelectorList(".a, .b, .c:not(.d)")
	want := []string{".a", ".b", ".c:not(.d)"}
	if len(got) != len(want) {
		t.Fatalf("got %v", got)
	}
}

func TestExtractClassesAndPseudo(t *testing.T) {
	cases := []struct {
		sel    string
		class  string
		pseudo string
	}{
		{".foo", "foo", ""},
		{".foo.bar", "bar", ""},
		{".wrap .baz", "baz", ""},
		{".name-input::placeholder", "name-input", "placeholder"},
		{".x:hover", "x", ""},
		{"button.primary", "primary", ""},
	}
	for _, tc := range cases {
		classes, pseudo := extractClassesAndPseudo(tc.sel)
		if pseudo != tc.pseudo {
			t.Errorf("%s: pseudo = %q, want %q", tc.sel, pseudo, tc.pseudo)
		}
		if len(classes) == 0 || classes[len(classes)-1] != tc.class {
			t.Errorf("%s: classes = %v, want last %q", tc.sel, classes, tc.class)
		}
	}
}
