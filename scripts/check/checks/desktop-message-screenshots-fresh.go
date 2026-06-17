package checks

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopMessageScreenshotsFresh warns (never fails) when the committed
// `@key.screenshot` couplings in the message catalogs have drifted from the
// committed capture report (`messages/screenshots/capture-report.json`). It runs
// the coupler's `--check` mode, which compares report↔catalog only: it reads no
// PNG files (those are gitignored and regenerable), so this check works on a
// clean checkout that never ran a capture.
//
// Warn-only by design (David's call): screenshots are an OPTIONAL translator aid,
// so stale couplings shouldn't block the build. The signal is "re-run
// `pnpm i18n:shots`", not "you broke something". Drift normally means a catalog
// key was renamed/removed after the last capture, or a new capture report landed
// without re-coupling.
//
// The coupler's `--check` exits 0 when every captured key is already coupled, and
// 1 when some coupling is missing/stale (printing each on its own line). We map
// exit 1 to a warn, not an error.
func RunDesktopMessageScreenshotsFresh(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	reportPath := filepath.Join(
		desktopDir, "src", "lib", "intl", "messages", "screenshots", "capture-report.json",
	)

	// No capture report yet → nothing to be stale against. Not a warn: a repo that
	// hasn't run a capture is a valid state (screenshots are optional).
	if _, err := os.Stat(reportPath); os.IsNotExist(err) {
		return Skipped("no capture report yet (run `pnpm i18n:shots`)"), nil
	} else if err != nil {
		return CheckResult{}, err
	}

	cmd := exec.Command("node", "scripts/couple-screenshots.js", "--check")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		return Success("screenshot couplings match the capture report"), nil
	}

	// Distinguish "drift" (coupler ran, exit 1) from "couldn't run the coupler at
	// all" (node missing, script error). Only the former is the warn we want; the
	// latter is a real error worth failing on.
	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the screenshot coupler\n%s", indentOutput(output))
	}

	drift := countDriftLines(output)
	msg := fmt.Sprintf(
		"%d screenshot %s drifted from the capture report; re-run `pnpm i18n:shots` to refresh "+
			"(warn-only: screenshots are an optional translator aid):\n%s",
		drift, Pluralize(drift, "coupling", "couplings"), indentOutput(output),
	)
	return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: drift, Changes: -1}, nil
}

// countDriftLines counts the per-coupling drift lines the coupler prints under
// its "Missing/stale screenshot couplings (N):" header (each prefixed with "  - ").
func countDriftLines(output string) int {
	n := 0
	for _, line := range strings.Split(output, "\n") {
		if strings.HasPrefix(strings.TrimLeft(line, " "), "- ") {
			n++
		}
	}
	return n
}
