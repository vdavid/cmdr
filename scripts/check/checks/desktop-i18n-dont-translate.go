package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nDontTranslate warns (never fails) when a non-`en` locale dropped a
// curated brand/product/system token that must survive translation verbatim
// (`Cmdr`, `macOS`, `GitHub`, `SMB`, `MTP`, the `{system_settings}`-family
// substitution tokens, …). A quality slip (the translator localized something
// that shouldn't be), not a crash, so warn-only. The curated list lives in
// `apps/desktop/scripts/i18n-check-dont-translate.js`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one dropped token (→ WARN), any other code = a genuine
// script error (→ ERROR). English-only today, so it's a no-op.
func RunDesktopI18nDontTranslate(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-dont-translate.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("brand/system tokens preserved across %d %s", n, Pluralize(n, "locale", "locales"))), nil
		}
		return Success("brand/system tokens preserved (English-only: no locales to check yet)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n don't-translate check\n%s", indentOutput(output))
	}

	dropped := countDriftLines(output)
	msg := fmt.Sprintf(
		"%d %s dropped a brand/system token that must stay verbatim "+
			"(warn-only: a quality slip, not a build breaker):\n%s",
		dropped, Pluralize(dropped, "key", "keys"), indentOutput(output),
	)
	return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: dropped, Changes: -1}, nil
}
