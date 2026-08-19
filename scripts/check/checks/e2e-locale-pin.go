package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// enUsLocaleArgs are process arguments that make a macOS launch look like an
// en-US machine.
//
// NSUserDefaults reads its argument domain first, so these outrank the machine's
// own System Settings for this process alone; nothing global changes. AppleLocale
// is the half that matters most, since it carries the region override Foundation
// formats by (`en_US@rg=sezzzz` on a US-English Mac living in Sweden), which the
// app follows and which no setting overrides. AppleLanguages makes the native
// menu bar English from the first frame.
//
// The TypeScript twin is EN_US_LOCALE_ARGS in
// apps/desktop/test/e2e-shared/pin-locale.ts; keep the two in sync.
func enUsLocaleArgs() []string {
	return []string{"-AppleLocale", "en_US", "-AppleLanguages", "(en-US)"}
}

// pinUiLanguage writes `appearance.language: "en"` into the shard's settings.json
// before the app reads it at startup, so the suite asserts English however the
// machine's own language preferences are set.
//
// A Go twin of apps/desktop/test/e2e-shared/pin-locale.ts, which the Linux
// Docker harness and both capture orchestrators call; keep the two in sync. It
// writes rather than merges because a shard's data dir is fresh. Why this exists,
// and what an unpinned run costs: apps/desktop/test/e2e-playwright/DETAILS.md
// § "The locale pin".
func pinUiLanguage(dataDir string) error {
	if err := os.MkdirAll(dataDir, 0o755); err != nil {
		return fmt.Errorf("failed to create data dir %s: %w", dataDir, err)
	}
	body, err := json.MarshalIndent(map[string]string{"appearance.language": "en"}, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to encode the pinned settings: %w", err)
	}
	settingsPath := filepath.Join(dataDir, "settings.json")
	if err := os.WriteFile(settingsPath, append(body, '\n'), 0o644); err != nil {
		return fmt.Errorf("failed to write %s: %w", settingsPath, err)
	}
	return nil
}
