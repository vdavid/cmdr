package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopI18nTermConsistency flags a locale that gives ONE English string two
// different names, so the app contradicts itself: the Traditional Chinese menu
// item said `命令選擇區…` while the palette it opened said `指令面板`, and the menu
// item `輸入授權金鑰…` opened a dialog titled `輸入授權碼`. Both shipped. Every
// other i18n check reads one key at a time against its source, which is exactly
// why this class kept getting through: it's only visible ACROSS keys.
//
// For an OVERLAY (`en-GB` over `en`) it compares what the user actually sees:
// the overlay's value where it forks a key, the base value where it doesn't. So
// a half-forked term ("colour" in one file, "color" still rendering in another)
// is caught even though each key on its own looks fine.
//
// WARN, not an error, and the allowlist entries carry a REASON. Plenty of
// same-English pairs SHOULD diverge, because English is doing two jobs with one
// word (`Done` is a screen-reader word after a checklist step in one place and an
// operation's lifecycle status in another; `Running` is a server process in one
// and a task in progress in another), and no checker can tell those from the
// drift. So the check doesn't decide: it forces the boundary to be WRITTEN DOWN
// once, next to the term. An entry with a blank reason doesn't silence anything.
//
// Locales that predate the check carry a `notYetReviewed` COUNT instead of
// per-term entries, so nine untriaged locales report one line each rather than
// ~300 findings that would train everyone to ignore the check. That count only
// ratchets DOWN (on local runs), and a locale in neither section is strict from
// its first day. See `apps/desktop/scripts/i18n-check-term-consistency.ts` and
// `apps/desktop/scripts/i18n-term-consistency-allowlist.json`.
//
// Exit-code contract (shared with the other locale checks via
// `i18n-locale-check-lib.ts`): 0 = clean / no locales, 1 = at least one
// unexplained divergence or a grown baseline (→ WARN), any other code = a genuine
// script error.
func RunDesktopI18nTermConsistency(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-term-consistency.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		// The script's LAST line is its own summary, and it's the only line that
		// knows how many divergences are still sitting behind a `notYetReviewed`
		// baseline. Echoing it beats recomputing a count here that could quietly
		// disagree, and beats the old "one thing has one name in each of N
		// locales", which was untrue while 263 divergences awaited triage.
		if summary := lastNonEmptyLine(output); summary != "" {
			return Success(summary), nil
		}
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("one thing has one name in each of %d %s", n, Pluralize(n, "locale", "locales"))), nil
		}
		return Success("term consistency holds (English-only: no locales to check yet)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n term-consistency check\n%s", indentOutput(output))
	}

	if exitErr.ExitCode() == 1 {
		findings := countDriftLines(output)
		msg := fmt.Sprintf(
			"%d unexplained divergent %s: one English string rendered two ways in the same locale. "+
				"Pick one wording, or record the split (with the reason the two surfaces genuinely differ) in "+
				"apps/desktop/scripts/i18n-term-consistency-allowlist.json:\n%s",
			findings, Pluralize(findings, "term", "terms"), indentOutput(output),
		)
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: findings, Changes: -1}, nil
	}

	return CheckResult{}, fmt.Errorf("couldn't run the i18n term-consistency check\n%s", indentOutput(output))
}

// lastNonEmptyLine returns the final non-blank line of `output`, or "" when
// there isn't one. Positional, so it carries no assumption about the wording.
func lastNonEmptyLine(output string) string {
	lines := strings.Split(output, "\n")
	for i := len(lines) - 1; i >= 0; i-- {
		if line := strings.TrimSpace(lines[i]); line != "" {
			return line
		}
	}
	return ""
}
