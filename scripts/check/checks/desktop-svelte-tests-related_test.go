package checks

import (
	"os"
	"path/filepath"
	"slices"
	"testing"
)

func TestRelatedTestTargets(t *testing.T) {
	changed := []string{
		"apps/desktop/src/lib/file-explorer/pane/pane-commands.ts",
		"apps/desktop/src/lib/file-explorer/pane/FilePane.svelte",
		"apps/desktop/src/routes/(main)/command-handlers/file-handlers.ts",
		"apps/desktop/scripts/tauri-wrapper.ts",
		"apps/desktop/eslint-plugins/no-raw-lucide-import.js",
		"eslint-plugins/no-error-string-match.js",
		"apps/desktop/src/lib/file-explorer/pane/DETAILS.md",
		"apps/desktop/src-tauri/src/lib.rs",
		"apps/website/src/pages/index.astro",
		"scripts/land/main.go",
		"apps/desktop/src/app.css",
	}
	want := []string{
		"src/lib/file-explorer/pane/pane-commands.ts",
		"src/lib/file-explorer/pane/FilePane.svelte",
		"src/routes/(main)/command-handlers/file-handlers.ts",
		"scripts/tauri-wrapper.ts",
		"eslint-plugins/no-raw-lucide-import.js",
		"../../eslint-plugins/no-error-string-match.js",
	}
	if got := relatedTestTargets(changed); !slices.Equal(got, want) {
		t.Errorf("got  %v\nwant %v", got, want)
	}
}

func TestRelatedTestTargetsDedupesAndSkipsBlankLines(t *testing.T) {
	changed := []string{"", "apps/desktop/src/a.ts", "apps/desktop/src/a.ts"}
	if got := relatedTestTargets(changed); !slices.Equal(got, []string{"src/a.ts"}) {
		t.Errorf("got %v", got)
	}
}

func TestChangedCatalogsDetectsOnlyMessageCatalogs(t *testing.T) {
	if !changedCatalogs([]string{"apps/desktop/src/lib/app.ts", "apps/desktop/src/lib/intl/messages/en/common.json"}) {
		t.Error("an English catalog edit wasn't seen")
	}
	if !changedCatalogs([]string{"apps/desktop/src/lib/intl/messages/pt-PT/settings.json"}) {
		t.Error("an overlay catalog edit wasn't seen")
	}
	if changedCatalogs([]string{"apps/desktop/src/lib/intl/messages.svelte.ts", "apps/desktop/package.json", ""}) {
		t.Error("a non-catalog change read as a catalog edit")
	}
}

// Regression: an edit to a catalog JSON selected zero tests, since catalogs reach the
// runtime through a virtual module the graph walk can't see.
func TestCatalogReaderTargetsPicksTheTestsThatReadCatalogs(t *testing.T) {
	desktop := t.TempDir()
	files := map[string]string{
		"src/lib/intl/czech-catalog.svelte.test.ts":               "import { t } from './messages.svelte.js'",
		"src/lib/intl/locale.ts":                                  "export const x = 1",
		"src/lib/settings/settings-search.i18n.test.ts":           "import en from 'virtual:cmdr-catalog/en'",
		"src/lib/commands/command-registry.parity.test.ts":        "const p = join(dir, 'messages', 'en')",
		"src/lib/file-explorer/file-explorer-i18n-parity.test.ts": "readFileSync('../intl/messages/en/fileExplorer.json')",
		"scripts/vite-locale-catalogs.test.ts":                    "import { localeCatalogs } from './vite-locale-catalogs.ts'",
		"src/lib/pane/pane.test.ts":                               "import { t } from '#lib/intl/messages.svelte.js'",
		"src/lib/pane/pane.ts":                                    "readFileSync('../intl/messages/en/pane.json')",
		"test/e2e-playwright/dialog-resize.spec.ts":               "readFileSync('messages/en/x.json')",
	}
	for rel, content := range files {
		path := filepath.Join(desktop, rel)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	got, err := catalogReaderTargets(desktop)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{
		"scripts/vite-locale-catalogs.test.ts",
		"src/lib/commands/command-registry.parity.test.ts",
		"src/lib/file-explorer/file-explorer-i18n-parity.test.ts",
		"src/lib/intl/czech-catalog.svelte.test.ts",
		"src/lib/settings/settings-search.i18n.test.ts",
	}
	if !slices.Equal(got, want) {
		t.Errorf("got  %v\nwant %v", got, want)
	}
}
