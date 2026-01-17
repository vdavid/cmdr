package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunCSSUnused checks for unused and undefined CSS classes and variables.
func RunCSSUnused(ctx *CheckContext) (CheckResult, error) {
	scriptDir := filepath.Join(ctx.RootDir, "scripts", "check-css-unused")

	cmd := exec.Command("go", "run", ".")
	cmd.Dir = scriptDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("CSS issues found\n%s", indentOutput(output))
	}

	return Success("No unused or undefined CSS"), nil
}
