package checks

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunDesktopShippedLocalesFresh fails if
// `apps/desktop/src-tauri/src/intl/shipped_locales.gen.rs` is out of sync with
// what the codegen would produce from the `messages/<locale>/` catalog dirs,
// i.e. somebody added or removed a locale without regenerating the table the
// Rust locale resolver reads (or hand-edited the generated file).
//
// Mirrors `desktop-message-keys-fresh.go`: in `--ci` mode the original is
// restored and any drift fails; outside `--ci` the regenerated file is kept so
// the dev gets the same auto-fix UX as oxfmt/clippy `--fix` and commits the diff
// alongside the catalog change that caused it.
//
// A stale table would leave a new locale unreachable by auto-selection AND
// unguarded against a script mismatch, which is the exact failure the script
// guard exists to prevent. A Rust unit test
// (`intl::tests::the_generated_table_covers_every_shipped_catalog`) catches the
// added/removed case too; this check additionally catches drift in the script
// facts, which no test can derive without CLDR data.
func RunDesktopShippedLocalesFresh(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	tablePath := filepath.Join(desktopDir, "src-tauri", "src", "intl", "shipped_locales.gen.rs")

	original, err := os.ReadFile(tablePath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", tablePath, err)
	}

	if ctx.CI {
		defer func() {
			_ = os.WriteFile(tablePath, original, 0o644)
		}()
	}

	regenCmd := exec.Command("node", "scripts/gen-shipped-locales.ts")
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		if !ctx.CI {
			_ = os.WriteFile(tablePath, original, 0o644)
		}
		return CheckResult{}, fmt.Errorf("`node scripts/gen-shipped-locales.ts` failed:\n%s", indentOutput(output))
	}

	regenerated, err := os.ReadFile(tablePath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read regenerated shipped_locales.gen.rs: %w", err)
	}

	changed := !bytes.Equal(regenerated, original)

	if ctx.CI && changed {
		return CheckResult{}, fmt.Errorf(
			"shipped_locales.gen.rs is stale. Run `pnpm intl:shipped-locales` from `apps/desktop/`",
		)
	}

	if changed {
		return SuccessWithChanges("shipped_locales.gen.rs regenerated"), nil
	}
	return Success("shipped_locales.gen.rs in sync"), nil
}
