package checks

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopI18nCoverage FAILS when a non-`en` locale doesn't fully cover the
// English catalog: a key MISSING from the locale (the runtime silently renders
// English), or a value byte-IDENTICAL to English without a
// `@key.sameAsSourceJustification` (likely untranslated). An honest-coverage
// gate so a "100% translated" claim is trustworthy. It's an ERROR, not a warn:
// a translation feature is exactly the kind of headline a warn-only signal lets
// slip past a release, so coverage gaps block the build. Deliberately-identical
// strings (brand names, units) opt out per-key via `@key.sameAsSourceJustification`.
// See `apps/desktop/scripts/i18n-check-coverage.js`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one coverage gap (→ ERROR), any other code = a genuine
// script error (→ ERROR). Nine locales ship today and all pass, so it stays
// green until a locale regresses or a new key lands untranslated.
func RunDesktopI18nCoverage(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-coverage.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("full coverage: all %d %s cover the catalog", n, Pluralize(n, "locale", "locales"))), nil
		}
		return Success("full translation coverage (English-only: no locales to check yet)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n coverage check\n%s", indentOutput(output))
	}

	gaps := countDriftLines(output)
	return CheckResult{}, fmt.Errorf(
		"%d untranslated %s (missing → English fallback, or identical to English). "+
			"Translate each, or mark a deliberately-identical string (brand name, unit) with "+
			"@key.sameAsSourceJustification:\n%s",
		gaps, Pluralize(gaps, "key", "keys"), indentOutput(output),
	)
}

// nonEnLocaleCount counts the non-`en` locale directories under `messages/` the
// way the JS `listLocales` does: a subdirectory holding at least one `.json`,
// excluding `en` and the reserved `screenshots/` sibling. It lets the coverage
// and stale success messages report the real locale count instead of implying no
// locales exist (exit 0 means "no locales OR all clean", and the old hardcoded
// wording always said the former). Source of truth for the rules:
// `i18n-catalog-lib.js` (`listLocales` / `NON_LOCALE_DIRS`). Returns 0 on any
// read error, so a passing check degrades to the English-only phrasing and never
// fails on this.
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
