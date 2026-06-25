package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nPlural FAILS (error, not warn) when a non-`en` `plural` message
// doesn't provide a branch for every CLDR category that LOCALE requires (e.g.
// Polish needs one/few/many/other). A `count` value landing in a missing
// category renders the wrong branch or throws, so it's a correctness break. The
// required set is data-driven from `Intl.PluralRules` (no bundled CLDR). `select`
// args are NOT checked here (their categories are message-defined, covered by
// parity). See `apps/desktop/scripts/i18n-check-plural.js`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one under-covered plural, any other code = a genuine
// script error. Both map to an error here. English-only today, so it's a no-op
// until a real locale lands.
func RunDesktopI18nPlural(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-plural.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("plural CLDR categories covered across %d %s", n, Pluralize(n, "locale", "locales"))), nil
		}
		return Success("every plural covers its locale's CLDR categories (English-only: no locales to check yet)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n plural-coverage check\n%s", indentOutput(output))
	}

	if exitErr.ExitCode() == 1 {
		under := countDriftLines(output)
		return CheckResult{}, fmt.Errorf(
			"%d plural %s missing a CLDR category this locale requires; add the missing branch(es)\n%s",
			under, Pluralize(under, "message", "messages"), indentOutput(output),
		)
	}

	return CheckResult{}, fmt.Errorf("the i18n plural-coverage check exited abnormally\n%s", indentOutput(output))
}
