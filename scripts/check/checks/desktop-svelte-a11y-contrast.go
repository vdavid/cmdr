package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunA11yContrast runs the design-time WCAG contrast checker.
// It parses the Svelte app's design tokens and scoped styles, resolves
// color-mix / var() chains, and reports any color/background pairs that
// fall below the WCAG 2.2 contrast thresholds, plus the enforced APCA Lc-45
// floor. Both are hard failures.
//
// A third category, unmodeled `opacity` dimming (opacity_check.go in the
// tool), is advisory (ResultWarning) rather than a hard gate: the tool can't
// verify a dimmed text color's real contrast without a browser, so a
// reported case might be a real bug or might be fine — it's surfaced for
// triage (a color-token conversion or a synthesizer), not blocked on. The
// tool itself always exits 0 for opacity-only findings (for any caller, not
// just this one), so we can't tell "clean" apart from "clean of hard
// failures, but opacity findings remain" from the exit code alone — and
// `go run` collapses any non-zero exit to 1 regardless, so exit codes
// couldn't carry a third state here even if the tool wanted them to. Instead
// the tool writes the finding count to CMDR_A11Y_OPACITY_STATUS_FILE when
// set (mirrors `desktop-rust-provider-smoke.go`'s statusFile side channel):
// a dedicated, structured signal rather than parsing the tool's
// human-readable stdout for a marker string.
func RunA11yContrast(ctx *CheckContext) (CheckResult, error) {
	scriptDir := filepath.Join(ctx.RootDir, "scripts", "check-a11y-contrast")

	statusDir, err := os.MkdirTemp("", "cmdr-a11y-opacity-status")
	if err != nil {
		return CheckResult{}, err
	}
	defer func() { _ = os.RemoveAll(statusDir) }()
	statusFile := filepath.Join(statusDir, "count.txt")

	cmd := exec.Command("go", "run", ".")
	cmd.Dir = scriptDir
	cmd.Env = append(os.Environ(), "CMDR_A11Y_OPACITY_STATUS_FILE="+statusFile)
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("contrast violations found\n%s", indentOutput(output))
	}

	if count, ok := readOpacityFindingCount(statusFile); ok {
		return CheckResult{
			Code: ResultWarning,
			Message: fmt.Sprintf(
				"%d unmodeled opacity %s (advisory: see scripts/check-a11y-contrast/README.md § Opacity)\n%s",
				count, Pluralize(count, "dim", "dims"), indentOutput(output),
			),
			Total: -1, Issues: count, Changes: -1,
		}, nil
	}

	return Success("No contrast violations"), nil
}

// readOpacityFindingCount reads the finding count the tool wrote to
// statusFile, if any. ok is false when the file doesn't exist (no opacity
// findings this run) or can't be parsed (treated the same as "none" — the
// human-readable stdout already reported whatever happened either way).
func readOpacityFindingCount(statusFile string) (count int, ok bool) {
	data, err := os.ReadFile(statusFile)
	if err != nil {
		return 0, false
	}
	n := 0
	if _, scanErr := fmt.Sscanf(strings.TrimSpace(string(data)), "%d", &n); scanErr != nil || n <= 0 {
		return 0, false
	}
	return n, true
}
