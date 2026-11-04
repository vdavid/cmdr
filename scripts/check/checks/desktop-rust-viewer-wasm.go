package checks

import (
	"fmt"
	"os/exec"
)

// RunDesktopRustViewerWasm compiles the file viewer's core (`crates/cmdr-viewer`) for
// `wasm32-unknown-unknown`, the target the browser demo hosts it on.
//
// Only this lane sees a host-only dependency or a `std::os::unix` call reaching the viewer:
// every native lane stays green while the browser build breaks. A compile isn't proof the
// viewer RUNS in a browser (a thread spawn or a `std` clock read compiles there and then
// panics); the demo engine's smoke run is.
//
// It builds its own target triple with the `wasm-release` profile, like
// `desktop-rust-listing-wasm`, so it shares no artifacts with the host lanes.
func RunDesktopRustViewerWasm(ctx *CheckContext) (CheckResult, error) {
	cmd := exec.Command("cargo", "build", "-p", "cmdr-viewer",
		"--target", "wasm32-unknown-unknown", "--profile", "wasm-release")
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("the viewer core doesn't build for wasm32:\n%s", indentOutput(output))
	}
	return Success("cmdr-viewer builds for wasm32"), nil
}
