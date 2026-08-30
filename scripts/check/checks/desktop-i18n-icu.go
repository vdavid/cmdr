package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nIcu FAILS (error, not warn) when a message isn't written in the
// grammar of its own family, which is two mirrored rules:
//
//   - An ICU message must compile via `intl-messageformat`. A stray unescaped
//     `'`/`{`/`<`, an unclosed tag, or a malformed `plural`/`select` THROWS at
//     render time, so it's a runtime crash, not a typo.
//   - A RAW value (`errors.*` plus the native `menu.*` families Rust draws) must
//     NOT carry ICU escaping. There's no ICU engine on that path to collapse a
//     doubled `”`, so it reaches the real macOS menu bar as two apostrophes.
//
// `en` is checked alongside the locales here, unlike in the other locale checks:
// the rule is about a catalog's own syntax, not about a translation's standing
// against its source. See `apps/desktop/scripts/i18n-check-icu.ts`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean, 1 = at
// least one bad message, any other code = a genuine script error. Like the parity
// check, both map to an error here.
func RunDesktopI18nIcu(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-icu.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("valid ICU, and no ICU escaping in a raw value, across en and all %d %s", n, Pluralize(n, "locale", "locales"))), nil
		}
		return Success("every message is valid ICU, with no ICU escaping in a raw value (English-only so far)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n message-syntax check\n%s", indentOutput(output))
	}

	if exitErr.ExitCode() == 1 {
		invalid := countDriftLines(output)
		return CheckResult{}, fmt.Errorf(
			"%d %s not written in their family's grammar (invalid ICU throws at render time; ICU escaping in a raw value renders verbatim)\n%s",
			invalid, Pluralize(invalid, "message", "messages"), indentOutput(output),
		)
	}

	return CheckResult{}, fmt.Errorf("the i18n message-syntax check exited abnormally\n%s", indentOutput(output))
}
