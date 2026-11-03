package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
)

// finderExtensionTargets are the slices the release pipeline's `compile` job builds, one per
// arch (`release-pipeline.yml`). Each targets its own macOS floor in `build.sh`.
var finderExtensionTargets = []string{"aarch64-apple-darwin", "x86_64-apple-darwin"}

// RunDesktopFinderExtensionCompile compiles the Finder extension (`apps/desktop/finder-extension/`)
// for both arches, through the same `build.sh compile` the release pipeline runs. Nothing else
// before a release compiles it, so a Swift API newer than a slice's macOS floor only showed up as a
// red release build: `Logger` (macOS 11) broke the x86_64 slice, which targets 10.15, in the
// `v0.58.1-pipelinetest.1` run.
//
// macOS only: it needs the macOS SDK. `swiftc` cross-compiles, so one Apple Silicon runner builds
// both slices in seconds, inside the existing macOS lane (`macos-lane.yml`).
func RunDesktopFinderExtensionCompile(ctx *CheckContext) (CheckResult, error) {
	if runtime.GOOS != "darwin" {
		return Skipped("needs the macOS SDK"), nil
	}

	outDir, err := os.MkdirTemp("", "cmdr-finder-extension-")
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't make a temp dir: %w", err)
	}
	defer os.RemoveAll(outDir)

	script := filepath.Join(ctx.RootDir, "apps", "desktop", "finder-extension", "build.sh")
	for _, target := range finderExtensionTargets {
		cmd := exec.Command(script, "compile", target, filepath.Join(outDir, target, "CmdrFinderExtension"))
		cmd.Dir = ctx.RootDir
		output, runErr := RunCommand(cmd, true)
		if runErr != nil {
			return CheckResult{}, fmt.Errorf("the Finder extension doesn't compile for %s:\n%s", target, indentOutput(output))
		}
	}

	return Success(fmt.Sprintf("Finder extension compiles for %d arches", len(finderExtensionTargets))), nil
}
