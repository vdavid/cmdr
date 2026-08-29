package checks

import (
	"os"
	"path/filepath"
	"testing"
)

// The locale checks report how many locales they covered. The count is
// structural on purpose: whether a locale is an overlay or a full translation is
// decided once, in TypeScript, because it needs CLDR script data Go can't reach
// (see `nonEnLocaleCount`). So this pins counting, not classification.
func TestNonEnLocaleCountCountsCatalogDirsOnly(t *testing.T) {
	root := t.TempDir()
	messages := filepath.Join(root, "apps", "desktop", "src", "lib", "intl", "messages")
	for _, dir := range []string{"en", "de", "hu", "en-GB", "pt", "pt-PT", "zh", "zh-Hant", "en-XA", "screenshots", "empty"} {
		if err := os.MkdirAll(filepath.Join(messages, dir), 0o755); err != nil {
			t.Fatal(err)
		}
		if dir == "empty" {
			continue // a dir with no JSON isn't a locale
		}
		if err := os.WriteFile(filepath.Join(messages, dir, "app.json"), []byte("{}"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	// Everything with JSON except `en` itself and the non-locale `screenshots/`:
	// de, hu, en-GB, pt, pt-PT, zh, zh-Hant, en-XA.
	if got := nonEnLocaleCount(root); got != 8 {
		t.Errorf("nonEnLocaleCount = %d, want 8", got)
	}
}

func TestNonEnLocaleCountIsZeroWithoutACatalog(t *testing.T) {
	if got := nonEnLocaleCount(t.TempDir()); got != 0 {
		t.Errorf("nonEnLocaleCount on an empty root = %d, want 0", got)
	}
}
