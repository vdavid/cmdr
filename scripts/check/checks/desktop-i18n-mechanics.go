package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nMechanics holds each locale's catalog values to the typography
// it declares in `docs/i18n/<tag>/mechanics.json`: its quotation marks and
// apostrophes, required spacing, and the hedged-grammar patterns ("file(s)"
// and its cousins) its grammar tempts translators into. A straight `"` and
// three dots are findings in every declared locale.
//
//   - Schema (ERROR): a malformed `mechanics.json` (a missing quote pair, a
//     non-quote mark, a pattern that doesn't compile).
//   - Findings (WARN): held to a per-locale count baseline that only ratchets
//     down, so a locale can declare its rules first and clean up after.
//
// A locale without the file is skipped (not declared yet); an overlay inherits
// its base language's. See `apps/desktop/scripts/i18n-check-mechanics.ts` and
// `docs/i18n/termbase.md`.
//
// Exit-code contract: 0 = clean (or within baseline), 1 = findings past a
// baseline (→ WARN), 3 = schema problem (→ ERROR), anything else = the script
// itself broke.
func RunDesktopI18nMechanics(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-mechanics.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		// The script's last line is its own summary, including what still sits
		// behind a baseline; echoing it keeps that count single-sourced.
		if summary := lastNonEmptyLine(output); summary != "" {
			return Success(summary), nil
		}
		return Success("typography holds"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n mechanics check\n%s", indentOutput(output))
	}

	switch exitErr.ExitCode() {
	case 1:
		findings := countDriftLines(output)
		msg := fmt.Sprintf(
			"%d %s break their locale's declared typography past its baseline (quote marks, spacing, "+
				"three dots, or a hedged form). Fix the value, or the locale's mechanics.json if a rule is wrong:\n%s",
			findings, Pluralize(findings, "value", "values"), indentOutput(output),
		)
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: findings, Changes: -1}, nil
	case 3:
		return CheckResult{}, fmt.Errorf(
			"a locale's mechanics.json has schema problems (docs/i18n/termbase.md has the schema):\n%s",
			indentOutput(output))
	}
	return CheckResult{}, fmt.Errorf("couldn't run the i18n mechanics check\n%s", indentOutput(output))
}
