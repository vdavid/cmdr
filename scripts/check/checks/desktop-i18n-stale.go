package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nStale warns (never fails) when a non-`en` locale holds a
// translation whose stored `@key.sourceHash` no longer matches the current
// English value's hash — i.e. the English source changed since the string was
// translated, so the translation is STALE. It also flags a present translation
// with no stored hash, a translated key whose English source was removed, and a
// stale key still carrying `reviewed: true` (the human sign-off no longer
// applies). See `apps/desktop/scripts/i18n-check-stale.js`.
//
// Warn-only by design (David's call): a stale translation is a maintenance
// signal, not a broken build, and the policy of escalating to a release-blocking
// error is a later choice (see the i18n plan). The signal is "re-translate the
// changed keys", not "you broke something".
//
// In today's English-only repo there are no non-`en` locales, so the script is a
// clean no-op (exit 0, "no non-en locales to check"). It becomes a live warn the
// moment a real locale lands.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one stale finding (→ WARN), any other code = a genuine
// script error (→ ERROR). We distinguish "the script ran and found staleness"
// (an `*exec.ExitError` with code 1) from "the script couldn't run at all" (node
// missing, a crash) the same way `desktop-message-screenshots-fresh` does.
func RunDesktopI18nStale(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-stale.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		// Exit 0: no non-en locales today, or every translation is fresh.
		return Success("no stale translations (no non-en locales to check today)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n stale check\n%s", indentOutput(output))
	}

	// Exit 1: the script ran and found stale translations. Each is printed on its
	// own `  - ` line (countDriftLines, shared with the screenshots check).
	stale := countDriftLines(output)
	msg := fmt.Sprintf(
		"%d stale %s; re-translate the changed keys and refresh @key.sourceHash "+
			"(warn-only: stale translations don't block the build):\n%s",
		stale, Pluralize(stale, "translation", "translations"), indentOutput(output),
	)
	return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: stale, Changes: -1}, nil
}
