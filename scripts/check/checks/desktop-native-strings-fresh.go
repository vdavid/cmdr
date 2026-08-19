package checks

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunDesktopNativeStringsFresh fails if
// `apps/desktop/src-tauri/src/intl/native_strings.gen.rs` is out of sync with
// what the codegen would produce from the `messages/<locale>/` catalogs, i.e.
// somebody edited a `menu.*` label (or one of the two pre-webview strings)
// without regenerating the table Rust compiles in, or hand-edited the generated
// file.
//
// Mirrors `desktop-shipped-locales-fresh.go`: in `--ci` mode the original is
// restored and any drift fails; outside `--ci` the regenerated file is kept so
// the dev gets the same auto-fix UX as oxfmt/clippy `--fix` and commits the diff
// alongside the catalog change that caused it.
//
// A stale table is invisible at runtime in the worst way: the menu bar keeps
// drawing the OLD label, so a copy fix or a landed translation silently doesn't
// ship, and nothing else in the pipeline compares the two.
func RunDesktopNativeStringsFresh(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	tablePath := filepath.Join(desktopDir, "src-tauri", "src", "intl", "native_strings.gen.rs")

	original, err := os.ReadFile(tablePath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", tablePath, err)
	}

	if ctx.CI {
		defer func() {
			_ = os.WriteFile(tablePath, original, 0o644)
		}()
	}

	regenCmd := exec.Command("node", "scripts/gen-native-strings.ts")
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		if !ctx.CI {
			_ = os.WriteFile(tablePath, original, 0o644)
		}
		return CheckResult{}, fmt.Errorf("`node scripts/gen-native-strings.ts` failed:\n%s", indentOutput(output))
	}

	regenerated, err := os.ReadFile(tablePath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read regenerated native_strings.gen.rs: %w", err)
	}

	changed := !bytes.Equal(regenerated, original)

	if ctx.CI && changed {
		return CheckResult{}, fmt.Errorf(
			"native_strings.gen.rs is stale. Run `pnpm intl:native-strings` from `apps/desktop/`",
		)
	}

	if changed {
		return SuccessWithChanges("native_strings.gen.rs regenerated"), nil
	}
	return Success("native_strings.gen.rs in sync"), nil
}
