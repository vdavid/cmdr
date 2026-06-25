package checks

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunDesktopMessageKeysFresh fails if `apps/desktop/src/lib/intl/keys.gen.ts` is
// out of sync with what the codegen would produce from the `messages/en/*.json`
// catalogs, i.e. somebody edited a catalog without regenerating the typed
// `MessageKey` union (or hand-edited the generated file).
//
// Mirrors `desktop-bindings-fresh.go`'s regenerate-and-diff strategy, minus the
// hash-marker short-circuit: the codegen is a ~50 ms node script over a dozen
// small JSON files (no cargo compile), so there's nothing to amortize. In
// `--ci` mode the original is restored and any drift fails; outside `--ci` the
// regenerated file is kept so the dev gets the same auto-fix UX as oxfmt/clippy
// `--fix` and commits the diff alongside the catalog change that caused it.
//
// The codegen ALSO reports keys used-in-code-but-missing (it exits non-zero in
// that case, surfaced here as a failure) and catalog-keys-never-used (a warning
// printed to stderr, non-fatal); this check inherits both.
func RunDesktopMessageKeysFresh(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	keysPath := filepath.Join(desktopDir, "src", "lib", "intl", "keys.gen.ts")

	original, err := os.ReadFile(keysPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", keysPath, err)
	}

	// In CI we never modify the working tree: restore on any exit path.
	// Outside CI we restore only on regen failure (so a half-written file
	// can't survive an error), then keep the regenerated content on success.
	if ctx.CI {
		defer func() {
			_ = os.WriteFile(keysPath, original, 0o644)
		}()
	}

	regenCmd := exec.Command("node", "scripts/gen-message-keys.ts")
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		if !ctx.CI {
			_ = os.WriteFile(keysPath, original, 0o644)
		}
		// The codegen exits non-zero only on missing keys (used in code, absent
		// from the catalog); surface its report verbatim.
		return CheckResult{}, fmt.Errorf("`node scripts/gen-message-keys.ts` failed:\n%s", indentOutput(output))
	}

	regenerated, err := os.ReadFile(keysPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read regenerated keys.gen.ts: %w", err)
	}

	changed := !bytes.Equal(regenerated, original)

	if ctx.CI && changed {
		return CheckResult{}, fmt.Errorf(
			"keys.gen.ts is stale. Run `pnpm intl:keys` from `apps/desktop/`",
		)
	}

	lineCount := bytes.Count(regenerated, []byte{'\n'})
	if changed {
		return SuccessWithChanges(fmt.Sprintf("keys.gen.ts regenerated (%d lines)", lineCount)), nil
	}
	return Success(fmt.Sprintf("keys.gen.ts in sync (%d lines)", lineCount)), nil
}
