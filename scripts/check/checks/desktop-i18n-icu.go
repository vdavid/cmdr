package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nIcu FAILS (error, not warn) when a non-`en` ICU message doesn't
// compile via `intl-messageformat` — a stray unescaped `'`/`{`/`<`, an unclosed
// tag, or a malformed `plural`/`select`. An invalid ICU message THROWS at render
// time, so it's a runtime crash, not a typo. The raw `errors.*` family is
// excluded (it resolves raw via `getMessage()`, not ICU). See
// `apps/desktop/scripts/i18n-check-icu.js`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one invalid ICU message, any other code = a genuine
// script error. Like the parity check, both map to an error here. English-only
// today, so it's a no-op until a real locale lands.
func RunDesktopI18nIcu(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-icu.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		return Success("every locale message is valid ICU (no non-en locales to check today)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n ICU-validity check\n%s", indentOutput(output))
	}

	if exitErr.ExitCode() == 1 {
		invalid := countDriftLines(output)
		return CheckResult{}, fmt.Errorf(
			"%d locale %s that don't compile as ICU (they'd throw at render time); fix the ICU syntax\n%s",
			invalid, Pluralize(invalid, "message", "messages"), indentOutput(output),
		)
	}

	return CheckResult{}, fmt.Errorf("the i18n ICU-validity check exited abnormally\n%s", indentOutput(output))
}
