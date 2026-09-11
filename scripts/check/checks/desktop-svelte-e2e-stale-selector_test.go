package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The lexer and selector-parser tests live in `e2e-stale-selector-parse_test.go`.

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
