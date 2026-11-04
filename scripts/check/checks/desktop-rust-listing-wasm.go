package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopRustListingWasm builds the browser demo's listing engine (`crates/cmdr-listing-wasm`)
// for `wasm32-unknown-unknown` through the same `scripts/build-listing-wasm.sh` a developer runs,
// into a temp dir, and loads the result in Node (`-smoke`).
//
// Two failures only this lane sees. A host-only dependency or `std::os::unix` import reaching
// `cmdr-fs` or `cmdr-listing` breaks the wasm compile while every native lane stays green. And a
// direct `std::time::Instant::now`, `tokio::time`, or thread spawn in the engine compiles for wasm
// but panics there at runtime, which only the smoke test's real listing reaches.
//
// With `-golden` it also builds the crate's wasm TEST target and runs it under Node
// (`wasm-bindgen-test-runner`): the volume engine's golden traces replayed on the demo's host
// must equal the fixtures the native engine blesses, catching browser-only drift (the `JoinSet`
// ordering, the inline op-log writer, inline `spawn_blocking`). A test build, so the shipped
// `.wasm` carries no scenarios.
//
// It builds its own target triple with its own profile (`wasm-release`), so it shares no
// artifacts with the host lanes and can't invalidate them; it still holds the build directory
// while cargo runs.
func RunDesktopRustListingWasm(ctx *CheckContext) (CheckResult, error) {
	outDir, err := os.MkdirTemp("", "cmdr-listing-wasm-")
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't make a temp dir: %w", err)
	}
	defer os.RemoveAll(outDir)

	script := filepath.Join(ctx.RootDir, "scripts", "build-listing-wasm.sh")
	cmd := exec.Command(script, "-out", outDir, "-smoke", "-golden")
	cmd.Dir = ctx.RootDir
	output, runErr := RunCommand(cmd, true)
	if runErr != nil {
		return CheckResult{}, fmt.Errorf("the browser listing engine doesn't build or run:\n%s", indentOutput(output))
	}

	info, err := os.Stat(filepath.Join(outDir, "cmdr_listing_wasm_bg.wasm"))
	if err != nil {
		return CheckResult{}, fmt.Errorf("the build left no .wasm: %w", err)
	}
	smoke := "smoke test passed"
	golden := "golden traces match"
	for _, line := range strings.Split(output, "\n") {
		if strings.HasPrefix(line, "listing-wasm smoke: OK") {
			smoke = strings.TrimPrefix(line, "listing-wasm smoke: ")
		}
		// libtest's summary: `test result: ok. 7 passed; 0 failed; …`.
		if rest, ok := strings.CutPrefix(line, "test result: ok. "); ok {
			passed, _, _ := strings.Cut(rest, ";")
			golden = fmt.Sprintf("golden traces match in wasm (%s)", passed)
		}
	}
	return Success(fmt.Sprintf("%d KiB .wasm, %s, %s", info.Size()/1024, smoke, golden)), nil
}
