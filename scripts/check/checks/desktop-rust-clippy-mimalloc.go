package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"runtime"
)

// macOS ships on the system allocator, and the `cmdr/mimalloc` feature puts it on
// mimalloc (`crates/cmdr-fs/DETAILS.md` § "Which global allocator"). Nothing else
// compiles that combination: the default clippy lane builds macOS without the
// feature, and CI's Linux runners take mimalloc without the macOS-only readers
// (the heap census, `mi_process_info`). This lane clippies it so the path we'd
// flip back to can't rot.
//
// It runs in its OWN build directory, nested inside the shared one so
// `cargo clean` takes it too. A feature flip on `cmdr-fs` changes the fingerprint
// of every workspace crate above it, so sharing `target/` would rebuild the
// workspace twice per run: once here, once for the next lane that asks without
// the feature (`SharedTargetFeatureArgs` explains the cost).

// clippyMimallocTargetDir is this lane's private cargo build directory.
func clippyMimallocTargetDir(rootDir string) string {
	return filepath.Join(cargoTargetDir(rootDir), "clippy-mimalloc")
}

// RunClippyMimalloc runs clippy over the workspace with mimalloc as macOS's global
// allocator.
func RunClippyMimalloc(ctx *CheckContext) (CheckResult, error) {
	if runtime.GOOS != "darwin" {
		return Skipped("macOS only: every other platform runs on mimalloc in the default clippy lane"), nil
	}
	selection, err := HostCargoSelectionArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	args := append([]string{"clippy", "--locked", "--all-targets"}, selection...)
	args = append(args, "--features", "cmdr/mimalloc", "--", "-D", "warnings")
	cmd := exec.Command("cargo", args...)
	cmd.Dir = ctx.RootDir
	cmd.Env = append(cmd.Environ(), "CARGO_TARGET_DIR="+clippyMimallocTargetDir(ctx.RootDir))
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("clippy with mimalloc as the global allocator found issues\n%s",
			indentOutput(output))
	}
	return Success("Checked the mimalloc build, no warnings"), nil
}
