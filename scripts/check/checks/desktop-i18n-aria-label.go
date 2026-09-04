package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nAriaLabel flags a translated accessible name that no longer
// CONTAINS its visible label (WCAG 2.5.3, Label in Name). Voice-control users say
// what they can see, so a name that paraphrases the label leaves them unable to
// press the button at all.
//
// English gets this right by construction (one author writes both keys).
// Translation is where it breaks, silently and per-language, because inflection
// pulls the two apart: a German case the label doesn't carry, a Hungarian suffix,
// a Swedish definite form, or a translator simply picking a smoother verb for the
// sentence than for the button. `docs/guides/i18n-translation.md` documented the
// failure shapes and noted that nothing enforced them; this is that enforcement.
//
// It needs no allowlist, because the pairing is GATED ON ENGLISH: a `fooAria` is
// only held to the standard when English's own `fooAria` contains `foo`. The
// `*Aria` keys that aren't accessible names for a sibling label (a timer
// description next to a countdown sentence, a narration of a `v{prev} → v{next}`
// badge) fail that gate and are never reported, so there's nothing to silence.
//
// ERROR class, and it runs in CI on every push: no pair has a legitimate reason to
// stay broken, and the English-containment gate above means a report is always a
// real regression rather than a judgment call. ❌ Don't soften this to a warn. See
// `apps/desktop/scripts/i18n-check-aria-label.ts`.
//
// Exit-code contract (shared with the other locale checks): 0 = clean / no
// locales, 1 = at least one broken pair (→ ERROR), any other code = a genuine
// script error.
func RunDesktopI18nAriaLabel(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-aria-label.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("every accessible name contains its label across %d %s", n, Pluralize(n, "locale", "locales"))), nil
		}
		return Success("aria label containment holds (English-only: no locales to check yet)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n aria-label check\n%s", indentOutput(output))
	}

	if exitErr.ExitCode() == 1 {
		broken := countDriftLines(output)
		tail := "names that no longer contain their visible label"
		if broken == 1 {
			tail = "name that no longer contains its visible label"
		}
		return CheckResult{}, fmt.Errorf(
			"%d translated accessible %s (WCAG 2.5.3): a voice-control user can't press a control whose name "+
				"doesn't repeat what they see. Fix by giving the LABEL the form the natural aria sentence already "+
				"uses, then cutting the label out of that sentence:\n%s",
			broken, tail, indentOutput(output),
		)
	}

	return CheckResult{}, fmt.Errorf("couldn't run the i18n aria-label check\n%s", indentOutput(output))
}
