package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunBtnRestyle runs the scoped-CSS guard that forbids feature components
// from overriding color / background on the canonical `<Button>` classes
// (`.btn`, `.btn-primary`, `.btn-secondary`, `.btn-danger`). Layout-only
// overrides via `:global(button)` are unaffected; only color / background
// declarations trip the check.
//
// See `scripts/check-btn-restyle/main.go` for the rule definition and the
// `/* allowed-btn-restyle: <reason> */` allowlist convention.
func RunBtnRestyle(ctx *CheckContext) (CheckResult, error) {
	scriptDir := filepath.Join(ctx.RootDir, "scripts", "check-btn-restyle")

	cmd := exec.Command("go", "run", ".")
	cmd.Dir = scriptDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("scoped CSS overrides Button styles\n%s", indentOutput(output))
	}
	return Success("No .btn-* restyles"), nil
}
