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
// script error (→ ERROR). The 12 non-`en` catalogs all pass today, so it stays
// green until a locale regresses or a new key lands untranslated.
func RunDesktopI18nCoverage(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-coverage.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		// "Covers the catalog" would be false for an overlay, which deliberately
		// carries only its forks, so the line states what actually held: every
		// locale met the contract for its own kind.
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("full coverage across %d %s", n, Pluralize(n, "locale", "locales"))), nil
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

// nonEnLocaleCount counts the non-`en` locale directories under `messages/` the
// way the JS `listLocales` does: a subdirectory holding at least one `.json`,
// excluding `en` and the reserved `screenshots/` sibling. It lets the locale
// checks report the real locale count instead of implying no locales exist (exit
// 0 means "no locales OR all clean"). Source of truth for the rules:
// `i18n-catalog-lib.ts` (`listLocales` / `NON_LOCALE_DIRS`). Returns 0 on any
// read error, so a passing check degrades to the English-only phrasing and never
// fails on this.
//
// It deliberately does NOT split full translations from overlays, though the
// success lines would read a little richer if it did. Deciding that needs CLDR
// likely-subtags data (a `zh-Hant` catalog is NOT an overlay of Simplified `zh`,
// and neither is `zh-TW`), which Node's `Intl` answers for free and Go can't
// without a new dependency. A second, approximate copy of the rule would drift
// from the real one in `src/lib/intl/locale-inheritance.ts` exactly where it
// matters, so the classification lives in ONE language and this stays a count.
func nonEnLocaleCount(rootDir string) int {
	messagesDir := filepath.Join(rootDir, "apps", "desktop", "src", "lib", "intl", "messages")
	entries, err := os.ReadDir(messagesDir)
	if err != nil {
		return 0
	}
	count := 0
	for _, entry := range entries {
		if !entry.IsDir() || entry.Name() == "en" || entry.Name() == "screenshots" {
			continue
		}
		files, err := os.ReadDir(filepath.Join(messagesDir, entry.Name()))
		if err != nil {
			continue
		}
		for _, f := range files {
			if strings.HasSuffix(f.Name(), ".json") {
				count++
				break
			}
		}
	}
	return count
}
