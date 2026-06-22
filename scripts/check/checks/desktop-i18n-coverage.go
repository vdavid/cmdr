package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
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
// script error (→ ERROR). English-only with no gaps today, so it's a no-op until
// a locale regresses.
func RunDesktopI18nCoverage(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-coverage.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		return Success("full translation coverage (no non-en locales to check today)"), nil
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
