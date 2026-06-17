package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nCoverage warns (never fails) when a non-`en` locale doesn't fully
// cover the English catalog: a key MISSING from the locale (the runtime silently
// renders English), or a value byte-IDENTICAL to English (likely untranslated).
// An honest-coverage signal so a "100% translated" claim is trustworthy — not a
// crash (the runtime falls back to English), so warn-only. See
// `apps/desktop/scripts/i18n-check-coverage.js`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one coverage gap (→ WARN), any other code = a genuine
// script error (→ ERROR). English-only today, so it's a no-op.
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
	msg := fmt.Sprintf(
		"%d untranslated %s (missing → English fallback, or identical to English) "+
			"(warn-only: untranslated keys don't block the build):\n%s",
		gaps, Pluralize(gaps, "key", "keys"), indentOutput(output),
	)
	return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: gaps, Changes: -1}, nil
}
