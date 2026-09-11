package checks

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

const (
	e2eSelSpecDir   = "apps/desktop/test/e2e-playwright/"
	e2eSelSharedDir = "apps/desktop/test/e2e-shared/"
	e2eSelSrcDir    = "apps/desktop/src/"
)

// e2eSelTS turns `§` into a backtick, so TypeScript template literals fit in a Go
// raw string.
func e2eSelTS(s string) string { return strings.ReplaceAll(s, "§", "`") }

// runE2EStaleSelectorOn writes the files (paths relative to the repo root, `§` for a
// backtick) into a temp repo and runs the check. The frontend source dir always
// exists, so a fixture can declare no tokens at all.
func runE2EStaleSelectorOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, filepath.FromSlash(e2eSelSrcDir)), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	for rel, body := range files {
		full := filepath.Join(root, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(e2eSelTS(body)), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunE2EStaleSelector(&CheckContext{RootDir: root})
}

func requireE2EStaleSelectorHits(t *testing.T, err error, sites ...string) {
	t.Helper()
	if err == nil {
		t.Fatalf("expected stale selectors at %q, got success", sites)
	}
	for _, site := range sites {
		if !strings.Contains(err.Error(), site) {
			t.Errorf("expected %q in:\n%s", site, err)
		}
	}
}

func requireNoE2EStaleSelectorHits(t *testing.T, err error, sites ...string) {
	t.Helper()
	if err == nil {
		return
	}
	for _, site := range sites {
		if strings.Contains(err.Error(), site) {
			t.Errorf("did not expect %q in:\n%s", site, err)
		}
	}
}

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

// ---- the check ----

func TestE2EStaleSelector_FlagsThePreFixServersHubCapture(t *testing.T) {
	// Verbatim from `43a711b1d^:apps/desktop/test/e2e-playwright/i18n-capture-surfaces.ts`.
	// The capture waited on the network browser's connect row for five days after the
	// servers hub replaced it, because nothing runs the capture routinely.
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSrcDir + "lib/ServersHub.svelte": `<div class="servers-hub"><div class="add-row"></div></div>
<div data-dialog-id={dialogId}></div>
`,
		e2eSelSpecDir + "i18n-capture-surfaces.ts": `  await mainOverlay('connect-to-server', async () => {
    await mcpSelectVolume('left', 'Servers')
    await main.waitForSelector('.network-browser .connect-row', 10000)
    await main.evaluate(§(function(){
      var el = document.querySelector('.network-browser .connect-row');
      if (el) el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    })()§)
    return '[data-dialog-id="connect-to-server"]'
  })
`,
	})
	requireE2EStaleSelectorHits(t, err,
		"i18n-capture-surfaces.ts:3: class `network-browser`",
		"i18n-capture-surfaces.ts:3: class `connect-row`",
		"i18n-capture-surfaces.ts:5: class `connect-row`",
		AllowStaleSelectorComment,
	)
	requireNoE2EStaleSelectorHits(t, err, "i18n-capture-surfaces.ts:8:")
}

func TestE2EStaleSelector_VocabularyMatchesWholeTokensFromComponentsStylesAndScripts(t *testing.T) {
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSrcDir + "lib/Pane.svelte": `<div class="file-pane" class:is-focused={focused} data-pane-tint={tint}>
  <span class="entry-{kind}"></span>
</div>
<style>
  .header-row { color: red; }
</style>
`,
		e2eSelSrcDir + "lib/list.ts": `row.classList.add('queue-row')
dialog.dataset.dialogId = id
`,
		e2eSelSrcDir + "app.css": `.volume-dropdown { display: block; }
`,
		e2eSelSpecDir + "vocab.spec.ts": `await page.waitForSelector('.file-pane')
await page.waitForSelector('.is-focused')
await page.waitForSelector('[data-pane-tint]')
await page.waitForSelector('.header-row')
await page.waitForSelector('.queue-row')
await page.waitForSelector('[data-dialog-id="about"]')
await page.waitForSelector('.volume-dropdown')
await page.waitForSelector('.entry-file')
await page.waitForSelector('.pane')
`,
	})
	requireE2EStaleSelectorHits(t, err, "vocab.spec.ts:8:", "vocab.spec.ts:9:")
	requireNoE2EStaleSelectorHits(t, err,
		"vocab.spec.ts:1:", "vocab.spec.ts:2:", "vocab.spec.ts:3:", "vocab.spec.ts:4:",
		"vocab.spec.ts:5:", "vocab.spec.ts:6:", "vocab.spec.ts:7:",
	)
}

func TestE2EStaleSelector_ComponentTestsCountTowardTheVocabulary(t *testing.T) {
	// A component test mounts the real component, Ark primitives included, so it's where
	// library-rendered markup like Ark's `data-value` gets spelled, and it fails in
	// `svelte-tests` if that markup goes away. Without it, every Ark item selector in the
	// suite reads as stale.
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSrcDir + "routes/viewer/EncodingPicker.test.ts": `const item = document.querySelector('[data-part="item"][data-value="utf8"]')
`,
		e2eSelSpecDir + "viewer-encoding-picker.spec.ts": `await page.click('[data-part="item"][data-value="utf16Le"]')
`,
	})
	if err != nil {
		t.Fatalf("expected an attribute spelled only in a component test to count, got: %v", err)
	}
}

func TestE2EStaleSelector_PassesWhenEveryTokenExists(t *testing.T) {
	res, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSrcDir + "lib/Hub.svelte": `<div class="servers-hub"><div class="add-row"></div></div>
`,
		e2eSelSpecDir + "servers.spec.ts": `await page.waitForSelector('.servers-hub .add-row')
`,
	})
	if err != nil {
		t.Fatalf("expected success, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestE2EStaleSelector_ScansE2EShared(t *testing.T) {
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSharedDir + "mcp-client.ts": `document.querySelector('.gone-row')
`,
	})
	requireE2EStaleSelectorHits(t, err, "e2e-shared/mcp-client.ts:1: class `gone-row`")
}

func TestE2EStaleSelector_IgnoresVitestUnitTests(t *testing.T) {
	// A Vitest file builds its own happy-dom tree, so its classes owe nothing to src.
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSpecDir + "helpers/click-button-by-text.test.ts": `document.body.innerHTML = '<div class="row"></div>'
document.querySelector('.row')
`,
	})
	if err != nil {
		t.Fatalf("expected a Vitest file to be skipped, got: %v", err)
	}
}

func TestE2EStaleSelector_OptOutOnTheLineAboveOrTheSameLine(t *testing.T) {
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSpecDir + "optout.spec.ts": `// allowed-stale-selector: the row class comes from a third-party widget
await page.click('.vendor-row')
await page.click('.vendor-cell') // allowed-stale-selector: same widget
await page.click('.gone-row')
`,
	})
	requireE2EStaleSelectorHits(t, err, "optout.spec.ts:4: class `gone-row`")
	requireNoE2EStaleSelectorHits(t, err, "optout.spec.ts:2:", "optout.spec.ts:3:")
}

func TestE2EStaleSelector_OptOutNeedsAReason(t *testing.T) {
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSpecDir + "reasonless.spec.ts": `// allowed-stale-selector:
await page.click('.vendor-row')
`,
	})
	requireE2EStaleSelectorHits(t, err, "reasonless.spec.ts:2: class `vendor-row`", "needs a reason")
}

func TestE2EStaleSelector_ReportsAnUnusedOptOut(t *testing.T) {
	_, err := runE2EStaleSelectorOn(t, map[string]string{
		e2eSelSrcDir + "lib/Row.svelte": `<div class="real-row"></div>
`,
		e2eSelSpecDir + "unused.spec.ts": `// allowed-stale-selector: nothing below needs it anymore
await page.click('.real-row')
`,
	})
	requireE2EStaleSelectorHits(t, err, "unused", "unused.spec.ts:1:")
}
