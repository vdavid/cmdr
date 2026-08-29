package checks

import (
	"os"
	"path/filepath"
	"testing"
)

func TestIsOverlayLocaleMirrorsTheRuntimeFallbackChain(t *testing.T) {
	shipped := map[string]bool{"en": true, "en-GB": true, "en-XA": true, "de": true, "pt": true, "pt-PT": true, "zh": true}
	cases := []struct {
		tag  string
		want bool
	}{
		{"de", false},          // a language base is never an overlay: it IS the base
		{"pt-PT", true},        // its language base ships, so it carries only its forks
		{"en-GB", true},        // `en` always ships, so an en variant is always an overlay
		{"fr-CA", false},       // no `fr` catalog, so it's a full translation of en
		{"zh-Hant-TW", true},   // resolves to `zh`, the language base, like the runtime does
		{"en-XA", false},       // the generated pseudolocale renders every English key
		{"screenshots", false}, // not a locale at all, and holds no `-`
	}
	for _, tc := range cases {
		if got := isOverlayLocale(tc.tag, shipped); got != tc.want {
			t.Errorf("isOverlayLocale(%q) = %v, want %v", tc.tag, got, tc.want)
		}
	}
}

func TestLocaleCountsSplitsTranslationsFromOverlays(t *testing.T) {
	root := t.TempDir()
	messages := filepath.Join(root, "apps", "desktop", "src", "lib", "intl", "messages")
	for _, dir := range []string{"en", "de", "hu", "en-GB", "pt", "pt-PT", "en-XA", "screenshots", "empty"} {
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

	// de, hu, pt, en-XA are full translations; en-GB and pt-PT are overlays.
	translations, overlays := localeCounts(root)
	if translations != 4 || overlays != 2 {
		t.Errorf("localeCounts = (%d translations, %d overlays), want (4, 2)", translations, overlays)
	}
}

func TestLocaleCountsIsZeroWithoutACatalog(t *testing.T) {
	translations, overlays := localeCounts(t.TempDir())
	if translations != 0 || overlays != 0 {
		t.Errorf("localeCounts on an empty root = (%d, %d), want (0, 0)", translations, overlays)
	}
}
