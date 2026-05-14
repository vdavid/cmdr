package checks

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunDesktopBindingsFresh fails if `apps/desktop/src/lib/ipc/bindings.ts` is
// out of sync with what `pnpm bindings:regen` would produce, i.e. somebody
// edited a Rust command surface without regenerating the typed IPC bindings.
//
// Strategy: snapshot the committed file → run the regen → diff bytes → restore
// the snapshot regardless of outcome (so the working tree stays exactly as the
// caller left it). No git involvement: the check is safe to run with a dirty
// working tree.
func RunDesktopBindingsFresh(ctx *CheckContext) (CheckResult, error) {
	bindingsPath := filepath.Join(ctx.RootDir, "apps", "desktop", "src", "lib", "ipc", "bindings.ts")
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	original, err := os.ReadFile(bindingsPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", bindingsPath, err)
	}

	// Always restore before returning, even on test/regen failure.
	defer func() {
		_ = os.WriteFile(bindingsPath, original, 0o644)
	}()

	regenCmd := exec.Command("pnpm", "bindings:regen")
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		return CheckResult{}, fmt.Errorf("`pnpm bindings:regen` failed:\n%s", indentOutput(output))
	}

	regenerated, err := os.ReadFile(bindingsPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read regenerated bindings: %w", err)
	}

	if string(regenerated) != string(original) {
		return CheckResult{}, fmt.Errorf(
			"bindings.ts is stale. Run `pnpm bindings:regen` from `apps/desktop/` and commit the diff",
		)
	}

	return Success(fmt.Sprintf("bindings.ts in sync (%d lines)", bytes.Count(original, []byte{'\n'}))), nil
}
