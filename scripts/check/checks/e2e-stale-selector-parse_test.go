package checks

import (
	"reflect"
	"testing"
	"time"
)

func hasE2ESelLiteral(lits []e2eSelLiteral, line int, text string) bool {
	for _, lit := range lits {
		if lit.line == line && lit.text == text {
			return true
		}
	}
	return false
}

// ---- lexer ----

func TestE2EStaleSelector_LexerFindsQuotedAndTemplateLiterals(t *testing.T) {
	got := lexE2ESelLiterals(e2eSelTS(`const a = '.one-x'
const b = ".two-x"
const c = §${A} .three-x§
`))
	want := []e2eSelLiteral{
		{line: 1, text: ".one-x"},
		{line: 2, text: ".two-x"},
		{line: 3, text: string(e2eSelInterpolation) + " .three-x"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %#v\nwant %#v", got, want)
	}
}

func TestE2EStaleSelector_LexerReadsStringsInsideEvaluateBodies(t *testing.T) {
	got := lexE2ESelLiterals(e2eSelTS(`await page.evaluate(§(function(){
  var el = document.querySelector('.network-browser .connect-row');
  if (el) el.click();
})()§)
`))
	if !hasE2ESelLiteral(got, 2, ".network-browser .connect-row") {
		t.Errorf("expected the evaluate body's querySelector string on line 2, got %#v", got)
	}
}

func TestE2EStaleSelector_LexerReadsLiteralsInsideInterpolations(t *testing.T) {
	got := lexE2ESelLiterals(e2eSelTS(`const s = §${ok ? '.a-b' : '.c-d'} .e-f§
`))
	for _, want := range []string{".a-b", ".c-d", string(e2eSelInterpolation) + " .e-f"} {
		if !hasE2ESelLiteral(got, 1, want) {
			t.Errorf("expected %q on line 1, got %#v", want, got)
		}
	}
}

func TestE2EStaleSelector_LexerSkipsComments(t *testing.T) {
	got := lexE2ESelLiterals(`// '.in-line-comment'
/* '.in-block-comment'
   '.still-in-block' */
const real = '.real-one'
`)
	want := []e2eSelLiteral{{line: 4, text: ".real-one"}}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %#v\nwant %#v", got, want)
	}
}

func TestE2EStaleSelector_LexerSurvivesQuotesInRegexLiteralsAndDivision(t *testing.T) {
	got := lexE2ESelLiterals(`if (/it's/.test(name)) x = '.after-regex'
const half = total / 2; const q = '.after-division'; const r = n / 3
`)
	if !hasE2ESelLiteral(got, 1, ".after-regex") {
		t.Errorf("a quote inside a regex literal swallowed the next string: %#v", got)
	}
	if !hasE2ESelLiteral(got, 2, ".after-division") {
		t.Errorf("a division was read as a regex literal: %#v", got)
	}
}

func TestE2EStaleSelector_TerminatesOnEveryInputPrefix(t *testing.T) {
	// Regression anchor: a selector that parsed right up to the end of its string
	// once spun forever, because the interpolation marker equalled the parser's
	// end-of-input byte. Truncated input also exercises every unterminated shape.
	src := e2eSelTS(`await main.waitForSelector('.servers-hub .add-row')
return §${DIALOG} [data-dialog-id="x"]§
const re = /a'b/; const s = §${a ? '.x-y' : §${b}.z§}§
var el = document.querySelector('.file-pane.${state}')
`)
	done := make(chan struct{})
	go func() {
		defer close(done)
		for i := 0; i <= len(src); i++ {
			for _, lit := range lexE2ESelLiterals(src[:i]) {
				e2eSelTokens(lit.text)
			}
		}
	}()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("lexing and parsing every prefix didn't finish in 5 s: a loop stopped advancing")
	}
}

// ---- selector extraction ----

func TestE2EStaleSelector_ExtractsClassesAndDataAttributes(t *testing.T) {
	cases := []struct {
		name   string
		source string   // exactly one TypeScript literal
		want   []string // "class:x" / "attribute:data-x"; nil means nothing to check
	}{
		{"descendant classes", `'.servers-hub .add-row'`, []string{"class:servers-hub", "class:add-row"}},
		{"data attribute name, value unchecked", `'[data-dialog-id="about"]'`, []string{"attribute:data-dialog-id"}},
		{"interpolated ancestor", `§${WIZARD} .step-shell§`, []string{"class:step-shell"}},
		{"quoted value is not a class", `'.file-entry[data-filename="file-a.txt"]'`, []string{"class:file-entry", "attribute:data-filename"}},
		{"classes inside :not()", `'.chip-filter:not(.is-configured)'`, []string{"class:chip-filter", "class:is-configured"}},
		{"relative :has()", `'.row-x:has(> .cell-x)'`, []string{"class:row-x", "class:cell-x"}},
		{"selector list and combinators", `'.a-b, .c-d > .e-f ~ .g-h + .i-j'`, []string{"class:a-b", "class:c-d", "class:e-f", "class:g-h", "class:i-j"}},
		{"non-data attributes are not checked", `'.search-overlay [aria-label="Size"]'`, []string{"class:search-overlay"}},
		{"tag, role, and nth-child only", `'button[role="radio"]:nth-child(2)'`, nil},
		{"lone class", `'.toast'`, []string{"class:toast"}},
		{"dotfile-shaped lone class is still a selector", `'.hidden-file'`, []string{"class:hidden-file"}},
		{"class built from an interpolation", `§.entry-${kind}§`, nil},
		{"dynamic class beside a static one", `§.file-pane.${state}§`, []string{"class:file-pane"}},
		{"attribute name built from an interpolation", `§[data-${name}-id]§`, nil},
		{"id selector", `'#loading-screen'`, nil},

		{"file name", `'file-a.txt'`, nil},
		{"tag-shaped file name", `'a.txt'`, nil},
		{"dotted command id", `'nav.back'`, nil},
		{"camelCase command id", `'nav.goToPath'`, nil},
		{"uppercase attribute name", `'[Content_Types].xml'`, nil},
		{"interpolated base name plus extension", `§${name}.png§`, nil},
		{"prefix string", `'.cmdr-temp-'`, nil},
		{"prose", `'Hello. World'`, nil},
		{"number", `'0.5'`, nil},
		{"URL", `'https://getcmdr.com'`, nil},
		{"settings key", `'appearance.language'`, nil},
		{"an uppercase class disqualifies the whole literal", `'.foo-bar .barBaz'`, nil},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			lits := lexE2ESelLiterals(e2eSelTS(tc.source))
			if len(lits) != 1 {
				t.Fatalf("fixture must hold exactly one literal, lexed %d from %s", len(lits), tc.source)
			}
			var got []string
			for _, tok := range e2eSelTokens(lits[0].text) {
				got = append(got, tok.kind+":"+tok.name)
			}
			if !reflect.DeepEqual(got, tc.want) {
				t.Errorf("tokens of %s: got %q, want %q", tc.source, got, tc.want)
			}
		})
	}
}
