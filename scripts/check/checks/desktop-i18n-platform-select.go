package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopI18nPlatformSelect FAILS (error, not warn) when a value that reads
// differently per OS isn't the one platform-select shape,
// `{platform, select, macos {…} windows {…} other {…}}` as the WHOLE value:
// canonical spelling and branch order, a legal branch set (no `macos`/`windows`
// branch equal to `other`), `platform` nowhere else, raw bodies the shared
// splitter reads the same way ICU does, no block in a key read as-is (the
// manifest keys, the Finder menu title), and every locale carrying its source's
// branch set. The raw readers (`getMessage()`, the native-strings generator)
// split blocks with one splitter, so a value off this shape renders wrong text
// or raw syntax. `en` is checked too. See
// `apps/desktop/scripts/i18n-check-platform-select.ts`.
//
// Exit-code contract (mirrored by `i18n-locale-check-lib.ts`): 0 = clean, 1 = at
// least one finding, any other code = a genuine script error.
func RunDesktopI18nPlatformSelect(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("node", "scripts/i18n-check-platform-select.ts")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		if n := nonEnLocaleCount(ctx.RootDir); n > 0 {
			return Success(fmt.Sprintf("every platform block is in the one shape across en and all %d %s", n, Pluralize(n, "locale", "locales"))), nil
		}
		return Success("every platform block is in the one shape (English-only so far)"), nil
	}

	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the i18n platform-select check\n%s", indentOutput(output))
	}

	if exitErr.ExitCode() == 1 {
		bad := countDriftLines(output)
		return CheckResult{}, fmt.Errorf(
			"%d %s not in the one platform-select shape (the raw readers would split them wrong or show raw syntax)\n%s",
			bad, Pluralize(bad, "finding", "findings"), indentOutput(output),
		)
	}

	return CheckResult{}, fmt.Errorf("the i18n platform-select check exited abnormally\n%s", indentOutput(output))
}
