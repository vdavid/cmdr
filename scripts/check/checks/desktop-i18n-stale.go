package checks

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// staleStrictEnv is the env var the release flow sets to escalate a stale
// translation from a WARN to a build-failing ERROR. Kept in lockstep with
// `STALE_STRICT_ENV` in `apps/desktop/scripts/i18n-check-stale.js`.
const staleStrictEnv = "CMDR_I18N_STALE_STRICT"

// RunDesktopI18nStale flags a non-`en` locale that holds a translation whose
// stored `@key.sourceHash` no longer matches the current English value's hash
// (the English source changed since the string was translated, so the
// translation is STALE). It also flags a present translation with no stored hash,
// a translated key whose English source was removed, and a stale key still
// carrying `reviewed: true` (the human sign-off no longer applies). See
// `apps/desktop/scripts/i18n-check-stale.js`.
//
// Two strictness modes, selected by the `CMDR_I18N_STALE_STRICT` env var so the
// SAME check serves daily dev and release gating:
//   - NORMAL (unset): a stale finding is a WARN. Stale translations are a
//     maintenance signal ("re-translate the changed keys"), not a daily-dev build
//     breaker (David's call), so normal `pnpm check` never fails on staleness.
//   - RELEASE-STRICT (set): a stale finding is a build-failing ERROR. The release
//     flow (`scripts/release.sh`) exports the var before its `pnpm check` so a
//     release can NOT ship a stale translation. The node script returns exit 2 in
//     this mode; we map it to an ERROR result here.
//
// Review is NEVER a gate in either mode: a stale key's prior `reviewed` flag is
// reported as no-longer-applicable, but the absence of review never fails a check.
//
// In today's English-only repo there are no non-`en` locales, so the script is a
// clean no-op (exit 0, "no non-en locales to check") in both modes. It becomes a
// live warn (or, at release, a live error) the moment a real locale lands.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.js`): 0 = clean / no
// locales, 1 = at least one stale finding (→ WARN), 2 = either a genuine script
// error (node missing, a crash) OR, in strict mode, a stale finding escalated to
// an ERROR. We tell the two exit-2 cases apart by whether the strict env var is
// set: strict + exit 2 = "stale translations block this release"; otherwise exit
// 2 = "the script couldn't run". We distinguish a clean exit 0 the same way
// `desktop-message-screenshots-fresh` does.
func RunDesktopI18nStale(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	strict := os.Getenv(staleStrictEnv) == "1"

	cmd := exec.Command("node", "scripts/i18n-check-stale.js")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		// Exit 0: no non-en locales today, or every translation is fresh.
		return Success("no stale translations (no non-en locales to check today)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n stale check\n%s", indentOutput(output))
	}

	switch exitErr.ExitCode() {
	case 1:
		// Exit 1: the script ran and found stale translations (normal mode → WARN).
		stale := countDriftLines(output)
		msg := fmt.Sprintf(
			"%d stale %s; re-translate the changed keys and refresh @key.sourceHash "+
				"(warn-only outside release: stale translations don't block a normal build, "+
				"but DO fail a release):\n%s",
			stale, Pluralize(stale, "translation", "translations"), indentOutput(output),
		)
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: stale, Changes: -1}, nil
	case 2:
		if strict {
			// Strict mode: the script escalated a stale finding to exit 2 (→ ERROR).
			// This is the release gate firing: a release must not ship stale copy.
			stale := countDriftLines(output)
			return CheckResult{}, fmt.Errorf(
				"%d stale %s block this release; re-translate the changed keys and refresh "+
					"@key.sourceHash, then re-run the release\n%s",
				stale, Pluralize(stale, "translation", "translations"), indentOutput(output),
			)
		}
		fallthrough
	default:
		// Exit 2 outside strict mode (or any other code): the script couldn't run.
		return CheckResult{}, fmt.Errorf("couldn't run the i18n stale check\n%s", indentOutput(output))
	}
}
