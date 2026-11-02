package checks

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunDesktopNativeStringsFresh fails if what the codegen would produce from the
// `messages/<locale>/` catalogs differs from what's committed: the table Rust
// compiles in (`src-tauri/src/intl/native_strings.gen.rs`), and the Finder menu
// item's titles, which macOS reads from files (`src-tauri/Info.plist` and
// `finder-extension/lproj/`). So somebody edited a `menu.*` label (or one of the
// pre-webview strings, or the menu item's name) without regenerating, or
// hand-edited a generated file.
//
// Mirrors `desktop-shipped-locales-fresh.go`: in `--ci` mode the originals are
// restored and any drift fails; outside `--ci` the regenerated files are kept so
// the dev gets the same auto-fix UX as oxfmt/clippy `--fix` and commits the diff
// alongside the catalog change that caused it.
//
// A stale table is invisible at runtime in the worst way: the menu bar keeps
// drawing the OLD label, so a copy fix or a landed translation silently doesn't
// ship, and nothing else in the pipeline compares the two.
func RunDesktopNativeStringsFresh(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	original, err := snapshotNativeStringsOutputs(desktopDir)
	if err != nil {
		return CheckResult{}, err
	}

	if ctx.CI {
		defer func() {
			restoreNativeStringsOutputs(desktopDir, original)
		}()
	}

	regenCmd := exec.Command("node", "scripts/gen-native-strings.ts")
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		if !ctx.CI {
			restoreNativeStringsOutputs(desktopDir, original)
		}
		return CheckResult{}, fmt.Errorf("`node scripts/gen-native-strings.ts` failed:\n%s", indentOutput(output))
	}

	regenerated, err := snapshotNativeStringsOutputs(desktopDir)
	if err != nil {
		return CheckResult{}, err
	}

	changed := !sameFileContents(original, regenerated)

	if ctx.CI && changed {
		return CheckResult{}, fmt.Errorf(
			"the native strings are stale. Run `pnpm intl:native-strings` from `apps/desktop/`",
		)
	}

	if changed {
		return SuccessWithChanges("native strings regenerated"), nil
	}
	return Success("native strings in sync"), nil
}

// snapshotNativeStringsOutputs reads every file the generator writes.
func snapshotNativeStringsOutputs(desktopDir string) (map[string][]byte, error) {
	paths, err := filepath.Glob(filepath.Join(desktopDir, "finder-extension", "lproj", "*.lproj", "Localizable.strings"))
	if err != nil {
		return nil, fmt.Errorf("couldn't list the Finder menu item's strings: %w", err)
	}
	paths = append(paths,
		filepath.Join(desktopDir, "src-tauri", "src", "intl", "native_strings.gen.rs"),
		filepath.Join(desktopDir, "src-tauri", "Info.plist"),
	)
	contents := make(map[string][]byte, len(paths))
	for _, path := range paths {
		data, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("couldn't read %s: %w", path, err)
		}
		contents[path] = data
	}
	return contents, nil
}

// restoreNativeStringsOutputs puts the snapshot back, the generated folders exactly as they were.
func restoreNativeStringsOutputs(desktopDir string, original map[string][]byte) {
	_ = os.RemoveAll(filepath.Join(desktopDir, "finder-extension", "lproj"))
	for path, data := range original {
		_ = os.MkdirAll(filepath.Dir(path), 0o755)
		_ = os.WriteFile(path, data, 0o644)
	}
}

func sameFileContents(a, b map[string][]byte) bool {
	if len(a) != len(b) {
		return false
	}
	for path, data := range a {
		other, ok := b[path]
		if !ok || !bytes.Equal(data, other) {
			return false
		}
	}
	return true
}
