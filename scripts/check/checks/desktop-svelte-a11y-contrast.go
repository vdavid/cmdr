package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunA11yContrast runs the design-time WCAG contrast checker.
// It parses the Svelte app's design tokens and scoped styles, resolves
// color-mix / var() chains, and reports any color/background pairs that
// fall below the WCAG 2.2 contrast thresholds.
func RunA11yContrast(ctx *CheckContext) (CheckResult, error) {
	scriptDir := filepath.Join(ctx.RootDir, "scripts", "check-a11y-contrast")

	cmd := exec.Command("go", "run", ".")
	cmd.Dir = scriptDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("contrast violations found\n%s", indentOutput(output))
	}

	return Success("No contrast violations"), nil
}
