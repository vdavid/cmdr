package checks

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopMessageScreenshotsFresh checks the catalogs' `@key.screenshot`
// couplings and the representative rules (`scripts/representative-screenshots.ts`)
// against the committed capture report (`messages/screenshots/capture-report.json`),
// by running the coupler's `--check`. Every input is tracked and it reads no PNG,
// so it runs the same on a clean checkout as in CI. The findings logic lives in
// the coupler; this only maps its exit code.
//
// Two severities:
//   - Error: a structural break a key or surface rename leaves behind, wrong the
//     moment it's committed: a representative rule that reaches no catalog key,
//     or a rule or catalog screenshot naming an image the report doesn't have.
//   - Warn: stale couplings, which `pnpm i18n:couple` rewrites, and a
//     representative rule every key of which has its own capture. Drift stays a
//     warn because screenshots are an optional translator aid, and a key rename
//     would otherwise force dropping a still-truthful screenshot in the same
//     commit: the report only learns the new key name on the next capture.
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

	cmd := exec.Command("node", "scripts/couple-screenshots.ts", "--check")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err == nil {
		return screenshotsCheckResult(0, output)
	}
	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return CheckResult{}, fmt.Errorf("couldn't run the screenshot coupler\n%s", indentOutput(output))
	}
	return screenshotsCheckResult(exitErr.ExitCode(), output)
}

// Exit codes `couple-screenshots.ts --check` reports findings with (mirrored as
// CHECK_EXIT_WARN / CHECK_EXIT_ERROR there). Both sit clear of Node's own codes,
// so an uncaught exception (exit 1) can't pass for a finding.
const (
	screenshotsExitWarn  = 20
	screenshotsExitError = 21
)

// errScreenshotCouplerAbnormalExit marks an exit code outside the coupler's
// contract: the script crashed rather than reporting findings.
var errScreenshotCouplerAbnormalExit = errors.New("the screenshot coupler exited abnormally")

// screenshotsCheckResult maps the coupler's `--check` exit code to the check's
// outcome. Every finding sits on its own `  - ` line.
func screenshotsCheckResult(exitCode int, output string) (CheckResult, error) {
	switch exitCode {
	case 0:
		return Success("screenshot couplings match the capture report, and every representative rule is live"), nil
	case screenshotsExitWarn:
		findings := countDriftLines(output)
		msg := fmt.Sprintf(
			"%d screenshot %s to look at (warn-only: screenshots are an optional translator aid):\n%s",
			findings, Pluralize(findings, "finding", "findings"), indentOutput(output),
		)
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: findings, Changes: -1}, nil
	case screenshotsExitError:
		return CheckResult{}, fmt.Errorf(
			"a screenshot rule or coupling no longer matches the catalog or the capture report, "+
				"usually after a key or surface rename; fix the ones listed as broken\n%s",
			indentOutput(output),
		)
	default:
		return CheckResult{}, fmt.Errorf("%w (exit %d)\n%s", errScreenshotCouplerAbnormalExit, exitCode, indentOutput(output))
	}
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
