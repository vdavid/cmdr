package checks

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopI18nCoverage FAILS when a locale carries the wrong key set for its
// KIND. A full translation (`de`, `hu`) must cover the English catalog: a key
// MISSING from it (the runtime silently renders English) or a value
// byte-IDENTICAL to English without a `@key.sameAsSourceJustification` (likely
// untranslated) is a gap. An OVERLAY (`en-GB` over `en`, `pt-PT` over `pt`)
// inverts that: missing keys are the point, while a value identical to the
// catalog it overrides, or a key neither that catalog nor `en` has, is dead
// weight. An honest-coverage gate so a "100% translated" claim is trustworthy. It's an ERROR, not a warn:
// a translation feature is exactly the kind of headline a warn-only signal lets
// slip past a release, so coverage gaps block the build. Deliberately-identical
// strings (brand names, units) opt out per-key via `@key.sameAsSourceJustification`.
// See `apps/desktop/scripts/i18n-check-coverage.ts`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one coverage gap (→ ERROR), any other code = a genuine
// script error (→ ERROR). Nine locales ship today and all pass, so it stays
// green until a locale regresses or a new key lands untranslated.
func RunDesktopI18nCoverage(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-coverage.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		translations, overlays := localeCounts(ctx.RootDir)
		switch {
		case overlays > 0:
			return Success(fmt.Sprintf("full coverage: %d %s cover the catalog, %d overlay %s carry only their forks",
				translations, Pluralize(translations, "locale", "locales"),
				overlays, Pluralize(overlays, "locale", "locales"))), nil
		case translations > 0:
			return Success(fmt.Sprintf("full coverage: all %d %s cover the catalog", translations, Pluralize(translations, "locale", "locales"))), nil
		}
		return Success("full translation coverage (English-only: no locales to check yet)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n coverage check\n%s", indentOutput(output))
	}

	gaps := countDriftLines(output)
	return CheckResult{}, fmt.Errorf(
		"%d %s off their locale's coverage contract. Translate a missing or copied-through key, mark a "+
			"deliberately-identical string (brand name, unit) with @key.sameAsSourceJustification, or delete an "+
			"overlay key that's identical to what it overrides:\n%s",
		gaps, Pluralize(gaps, "key", "keys"), indentOutput(output),
	)
}

// localeCounts counts the non-`en` locale directories under `messages/` the way
// the JS `listLocales` does (a subdirectory holding at least one `.json`,
// excluding `en` and the reserved `screenshots/` sibling), split by KIND:
//
//   - translations: full catalogs of every English key (`de`, `hu`, and the
//     generated pseudolocale `en-XA`).
//   - overlays: regional variants whose language base also ships (`en-GB` over
//     `en`, `pt-PT` over `pt`), carrying ONLY the keys they fork.
//
// The split keeps the success messages honest: an overlay deliberately does NOT
// cover the catalog, so counting it as one that does would state something false.
// Source of truth for both rules: `i18n-catalog-lib.ts` (`listLocales`,
// `GENERATED_LOCALES`) and `i18n-locale-check-lib.ts` (`resolveLocaleSource`);
// this is a cosmetic count for the success line, never a classification the
// checks act on. Returns zeroes on any read error, so a passing check degrades to
// the English-only phrasing and never fails on this.
func localeCounts(rootDir string) (translations, overlays int) {
	messagesDir := filepath.Join(rootDir, "apps", "desktop", "src", "lib", "intl", "messages")
	entries, err := os.ReadDir(messagesDir)
	if err != nil {
		return 0, 0
	}
	shipped := map[string]bool{}
	for _, entry := range entries {
		if !entry.IsDir() || entry.Name() == "screenshots" {
			continue
		}
		files, err := os.ReadDir(filepath.Join(messagesDir, entry.Name()))
		if err != nil {
			continue
		}
		for _, f := range files {
			if strings.HasSuffix(f.Name(), ".json") {
				shipped[entry.Name()] = true
				break
			}
		}
	}
	for tag := range shipped {
		if tag == "en" {
			continue
		}
		if isOverlayLocale(tag, shipped) {
			overlays++
		} else {
			translations++
		}
	}
	return translations, overlays
}

// generatedLocales are locale dirs a generator writes as FULL translations, so
// they're never overlays however variant-shaped the tag looks. Mirrors
// `GENERATED_LOCALES` in `apps/desktop/scripts/i18n-catalog-lib.ts`.
var generatedLocales = map[string]bool{"en-XA": true}

// isOverlayLocale reports whether `tag` is an overlay: a variant whose language
// base (the part before the first `-`) also ships a catalog. Mirrors
// `resolveLocaleSource` in `apps/desktop/scripts/i18n-locale-check-lib.ts`.
func isOverlayLocale(tag string, shipped map[string]bool) bool {
	base, _, found := strings.Cut(tag, "-")
	if !found || generatedLocales[tag] {
		return false
	}
	return shipped[strings.ToLower(base)]
}
