package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopI18nTermbase holds the termbase (`docs/i18n/concepts.json` plus each
// locale's `terms.json`) to two things. The translation brief prints it to a
// translator as settled, so a broken pointer in it is a false fact.
//
//   - Schema (ERROR): a ruling for an unknown concept, a missing `chosen` /
//     `confidence` / `sources`, a confidence outside `confirmed` / `high` /
//     `tentative`, an `exceptions` key the English catalog lacks, a `decision`
//     naming a heading `decisions.md` doesn't have.
//   - Coverage drift (WARN): a shipped key whose English uses a concept while its
//     translation carries none of the ruling's forms and isn't in `exceptions`.
//     Held to a per-locale count baseline that only ratchets down, so a
//     half-cleaned locale reports one line instead of breaking the build.
//   - Decisions growth (WARN): a locale's `decisions.md` grew past its byte
//     budget in the same baseline, which also only ratchets down, so the file
//     stays distilled rulings rather than regrowing into a journal.
//
// A locale without `terms.json` is skipped. See
// `apps/desktop/scripts/i18n-check-termbase.ts` and `docs/i18n/termbase.md`.
//
// Exit-code contract: 0 = clean (or drift within baseline), 1 = drift past a
// baseline (→ WARN), 3 = schema problem (→ ERROR), anything else = the script
// itself broke.
func RunDesktopI18nTermbase(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-termbase.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		// The script's last line is its own summary, including how much drift still
		// sits behind a baseline; echoing it keeps that count single-sourced.
		if summary := lastNonEmptyLine(output); summary != "" {
			return Success(summary), nil
		}
		return Success("termbase holds"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n termbase check\n%s", indentOutput(output))
	}

	switch exitErr.ExitCode() {
	case 1:
		findings := countDriftLines(output) + countDecisionsGrowth(output)
		msg := fmt.Sprintf(
			"the termbase grew past its baseline (%d %s). A drifting key: the English uses a ruled concept and "+
				"the translation uses none of its forms; fix the translation, widen the ruling's \"accept\", or record "+
				"the key in the term's \"exceptions\" with the reason. A grown decisions.md: distill it instead of "+
				"appending:\n%s",
			findings, Pluralize(findings, "finding", "findings"), indentOutput(output),
		)
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: findings, Changes: -1}, nil
	case 3:
		return CheckResult{}, fmt.Errorf(
			"the termbase has schema problems, and the translation brief would print them as settled "+
				"(docs/i18n/termbase.md has the schema):\n%s", indentOutput(output))
	}
	return CheckResult{}, fmt.Errorf("couldn't run the i18n termbase check\n%s", indentOutput(output))
}

// countDecisionsGrowth counts the script's "<tag>/decisions.md grew to …" lines.
func countDecisionsGrowth(output string) int {
	n := 0
	for _, line := range strings.Split(output, "\n") {
		if strings.Contains(line, "/decisions.md grew to ") {
			n++
		}
	}
	return n
}
