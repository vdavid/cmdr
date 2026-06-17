package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nParity FAILS (error, not warn) when a non-`en` translation's
// substitution structure doesn't match its English source: a missing, renamed,
// or extra `{placeholder}` or `<tag>` (ICU keys), or a dropped/changed `{token}`
// (the raw `errors.*` family). This is the #1 runtime CRASH class —
// `intl-messageformat` throws on a `{name}` it has no value for, and the raw
// error pipeline mis-substitutes a token — so unlike the maintenance-signal
// checks (stale, key parity, don't-translate), a parity break MUST fail the
// build. See `apps/desktop/scripts/i18n-check-parity.js`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one parity mismatch, any other code = a genuine script
// error. Unlike the warn-only locale checks, BOTH exit 1 and any other non-zero
// map to an error here (a parity finding is a build breaker). We still
// distinguish them in the message so a node-missing crash reads differently from
// a real parity failure.
//
// In today's English-only repo there are no non-`en` locales, so the script is a
// clean no-op (exit 0). It becomes a real CI gate the moment a locale lands.
func RunDesktopI18nParity(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-parity.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		// Exit 0: no non-en locales today, or every translation matches English.
		return Success("placeholder/tag parity holds (no non-en locales to check today)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n parity check\n%s", indentOutput(output))
	}

	// Exit 1: the script ran and found parity mismatches. Each is on its own
	// `  - ` line (countDriftLines, shared with the other locale checks). These
	// are crash-class, so we return them as an ERROR.
	if exitErr.ExitCode() == 1 {
		mismatches := countDriftLines(output)
		return CheckResult{}, fmt.Errorf(
			"%d translation %s with a placeholder/tag mismatch (a missing/renamed/extra {arg} or <tag> "+
				"crashes at runtime); fix the translation to match the English structure\n%s",
			mismatches, Pluralize(mismatches, "key", "keys"), indentOutput(output),
		)
	}

	// Any other non-zero code is a genuine script error (node missing, a crash).
	return CheckResult{}, fmt.Errorf("the i18n parity check exited abnormally\n%s", indentOutput(output))
}
