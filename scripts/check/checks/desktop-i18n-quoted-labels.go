package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nQuotedLabels warns (never fails) when a non-`en` value quotes a
// button, setting, or command label in words the label itself doesn't use, so
// the reader hunts for a control that doesn't exist. Pairs come from English: a
// value quoting another key's whole label. See
// `apps/desktop/scripts/i18n-check-quoted-labels.ts`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.ts`): 0 = clean / no
// locales, 1 = at least one mismatched quote (→ WARN), any other code = a
// genuine script error (→ ERROR).
func RunDesktopI18nQuotedLabels(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-quoted-labels.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		n := nonEnLocaleCount(ctx.RootDir)
		return Success(fmt.Sprintf("quoted labels match their labels across %d %s", n, Pluralize(n, "locale", "locales"))), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n quoted-label check\n%s", indentOutput(output))
	}

	mismatched := countDriftLines(output)
	msg := fmt.Sprintf(
		"%d %s quote a label in words the label itself doesn't use "+
			"(warn-only: copy the label, or fork the quoting text with it):\n%s",
		mismatched, Pluralize(mismatched, "key", "keys"), indentOutput(output),
	)
	return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: mismatched, Changes: -1}, nil
}
