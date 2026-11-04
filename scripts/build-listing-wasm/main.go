// Command build-listing-wasm builds the browser demo's listing engine
// (`crates/cmdr-listing-wasm`) into a `.wasm` plus its ES-module JS glue.
//
//  1. Installs the pinned LLVM clang that compiles SQLite for wasm32 into a private mise home
//     (`toolchain.go`), then `cargo build` for `wasm32-unknown-unknown` with the size-tuned
//     `wasm-release` profile, the crate plus `browserBound` (crates the browser build doesn't
//     link yet but will, compiled here so a host-only dependency can't creep into them).
//  2. Makes sure `wasm-bindgen` matches the `wasm-bindgen` crate in `Cargo.lock` exactly (the
//     CLI refuses any other version), installing that pinned version when it doesn't.
//  3. `wasm-bindgen --target web` into the out dir (default `crates/cmdr-listing-wasm/pkg/`,
//     gitignored): `cmdr_listing_wasm.js`, `cmdr_listing_wasm_bg.wasm`, and their `.d.ts`.
//     Then `wasm-opt` (binaryen, pinned in `.mise.toml`) shrinks the `.wasm` in place
//     (`wasmOptArgs`).
//  4. With `-smoke`, loads the result in Node and drives one listing (`smoke.mjs`).
//  5. With `-golden`, builds the crate's wasm TEST target (the dev profile, so the shipped `.wasm`
//     carries no scenarios) and runs it under Node with `wasm-bindgen-test-runner`: the volume
//     engine's golden traces replayed on the demo's host against the fixtures the native engine
//     blesses (`crates/cmdr-listing-wasm/src/operations_host/golden_test.rs`).
//
// Usage (from the wrapper): ./scripts/build-listing-wasm.sh [-out DIR] [-smoke] [-golden] | -toolchain-only
package main

import (
	"bufio"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

const (
	crateName  = "cmdr-listing-wasm"
	wasmTarget = "wasm32-unknown-unknown"
	profile    = "wasm-release"
	outName    = "cmdr_listing_wasm"
)

// browserBound are workspace crates the browser engine is about to link (the write engine and its
// operation log), built for wasm32 beside it so the wasm compile guards them already.
var browserBound = []string{"cmdr-operation-log", "cmdr-operations"}

func main() {
	root := flag.String("root", ".", "the repo root")
	out := flag.String("out", "", "where the .wasm and JS glue go (default crates/cmdr-listing-wasm/pkg)")
	smoke := flag.Bool("smoke", false, "load the result in Node and drive one listing")
	golden := flag.Bool("golden", false, "replay the golden traces as a wasm test build under Node")
	toolchainOnly := flag.Bool("toolchain-only", false, "install the pinned wasm C toolchain and stop (CI warms its cache with this)")
	flag.Parse()

	if *toolchainOnly {
		if err := installToolchainOnly(); err != nil {
			fmt.Fprintf(os.Stderr, "build-listing-wasm: %v\n", err)
			os.Exit(1)
		}
		return
	}
	if err := run(*root, *out, *smoke, *golden); err != nil {
		fmt.Fprintf(os.Stderr, "build-listing-wasm: %v\n", err)
		os.Exit(1)
	}
}

func run(root, out string, smoke, golden bool) error {
	root, err := filepath.Abs(root)
	if err != nil {
		return err
	}
	crateDir := filepath.Join(root, "crates", crateName)
	if out == "" {
		out = filepath.Join(crateDir, "pkg")
	}
	if out, err = filepath.Abs(out); err != nil {
		return err
	}

	tcDir, err := toolchainDir()
	if err != nil {
		return err
	}
	tc, err := ensureToolchain(tcDir)
	if err != nil {
		return err
	}
	build := exec.Command("cargo", cargoBuildArgs()...)
	build.Dir = root
	build.Env = cargoEnv(os.Environ(), tc)
	build.Stdout = os.Stdout
	build.Stderr = os.Stderr
	if err := build.Run(); err != nil {
		return fmt.Errorf("cargo build: %w", err)
	}

	version, err := lockedVersion(filepath.Join(root, "Cargo.lock"), "wasm-bindgen")
	if err != nil {
		return err
	}
	if err := ensureWasmBindgen(root, version); err != nil {
		return err
	}

	if err := bindAndOptimize(root, out); err != nil {
		return err
	}

	if smoke {
		if err := command(root, "node", filepath.Join(crateDir, "smoke.mjs"), out); err != nil {
			return fmt.Errorf("smoke test: %w", err)
		}
	}
	if golden {
		return replayGolden(root, tc)
	}
	return nil
}

// bindAndOptimize turns cargo's `.wasm` into the shipped module in `out`: `wasm-bindgen --target
// web`, the ES-module marker, then `wasm-opt` in place.
func bindAndOptimize(root, out string) error {
	wasm := filepath.Join(root, "target", wasmTarget, profile, strings.ReplaceAll(crateName, "-", "_")+".wasm")
	if err := command(root, "wasm-bindgen", "--target", "web", "--out-dir", out, "--out-name", outName, wasm); err != nil {
		return fmt.Errorf("wasm-bindgen: %w", err)
	}
	// The glue is an ES module; saying so keeps Node from guessing (and warning) when it loads it.
	if err := os.WriteFile(filepath.Join(out, "package.json"), []byte("{ \"type\": \"module\" }\n"), 0o644); err != nil {
		return err
	}
	bound := filepath.Join(out, outName+"_bg.wasm")
	if err := command(root, "wasm-opt", wasmOptArgs(bound)...); err != nil {
		return fmt.Errorf("wasm-opt (binaryen, pinned in .mise.toml; run `mise install`): %w", err)
	}
	if info, err := os.Stat(bound); err == nil {
		fmt.Printf("build-listing-wasm: %s (%d KiB)\n", out, info.Size()/1024)
	}
	return nil
}

// wasmFeatures are the wasm proposals rustc's `wasm32-unknown-unknown` enables by default
// (`rustc --print cfg --target wasm32-unknown-unknown`), so `wasm-opt` accepts what rustc emits
// and uses nothing the browser target doesn't already need. `bulk-memory-opt` is binaryen's
// split-out half of `bulk-memory` (`memory.copy`/`memory.fill`).
var wasmFeatures = []string{
	"--enable-bulk-memory",
	"--enable-bulk-memory-opt",
	"--enable-multivalue",
	"--enable-mutable-globals",
	"--enable-nontrapping-float-to-int",
	"--enable-reference-types",
	"--enable-sign-ext",
}

// wasmOptArgs optimizes the bindgen output in place. `-O1` plus duplicate-function elimination,
// not `-Oz`: `-Oz` shrinks the raw file more but makes it compress worse, so the page downloads
// more (`crates/cmdr-listing-wasm/DETAILS.md` § The build has the measurements).
func wasmOptArgs(wasm string) []string {
	args := []string{"-O1", "--duplicate-function-elimination"}
	args = append(args, wasmFeatures...)
	return append(args, wasm, "-o", wasm)
}

// replayGolden builds the crate's wasm test target and runs it under Node.
func replayGolden(root string, tc wasmToolchain) error {
	test := exec.Command("cargo", cargoGoldenArgs()...)
	test.Dir = root
	test.Env = append(cargoEnv(os.Environ(), tc), "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner")
	test.Stdout = os.Stdout
	test.Stderr = os.Stderr
	if err := test.Run(); err != nil {
		return fmt.Errorf("golden replay: %w", err)
	}
	fmt.Println("listing-wasm golden: OK")
	return nil
}

// cargoGoldenArgs builds and runs only the golden replay: the crate's other test modules are
// native-only. `wasm-bindgen-test-runner` ships in the same `wasm-bindgen-cli` that
// `ensureWasmBindgen` matched to `Cargo.lock`, so it pairs with the locked crate too.
func cargoGoldenArgs() []string {
	return []string{"test", "-p", crateName, "--lib", "--target", wasmTarget, "--", "golden_test"}
}

func installToolchainOnly() error {
	dir, err := toolchainDir()
	if err != nil {
		return err
	}
	tc, err := ensureToolchain(dir)
	if err != nil {
		return err
	}
	fmt.Printf("build-listing-wasm: wasm toolchain ready (%s, %s)\n", tc.clang, tc.ar)
	return nil
}

func cargoBuildArgs() []string {
	args := []string{"build", "-p", crateName}
	for _, crate := range browserBound {
		args = append(args, "-p", crate)
	}
	return append(args, "--target", wasmTarget, "--profile", profile)
}

// lockedVersion reads the one version of `name` that `Cargo.lock` resolves.
func lockedVersion(lockPath, name string) (string, error) {
	file, err := os.Open(lockPath)
	if err != nil {
		return "", err
	}
	defer file.Close()
	return lockedVersionIn(bufio.NewScanner(file), name, lockPath)
}

func lockedVersionIn(lines *bufio.Scanner, name, source string) (string, error) {
	var versions []string
	inPackage := false
	for lines.Scan() {
		line := strings.TrimSpace(lines.Text())
		switch {
		case line == `name = "`+name+`"`:
			inPackage = true
		case inPackage && strings.HasPrefix(line, "version = "):
			versions = append(versions, strings.Trim(strings.TrimPrefix(line, "version = "), `"`))
			inPackage = false
		case line == "[[package]]":
			inPackage = false
		}
	}
	if err := lines.Err(); err != nil {
		return "", err
	}
	switch len(versions) {
	case 0:
		return "", fmt.Errorf("%s doesn't resolve %s", source, name)
	case 1:
		return versions[0], nil
	default:
		return "", fmt.Errorf("%s resolves %s at %d versions (%s); the CLI can only match one",
			source, name, len(versions), strings.Join(versions, ", "))
	}
}

// ensureWasmBindgen installs `wasm-bindgen-cli` at exactly `version` unless that's what's on PATH.
func ensureWasmBindgen(root, version string) error {
	installed, err := exec.Command("wasm-bindgen", "--version").Output()
	var notFound *exec.Error
	if err != nil && !errors.As(err, &notFound) {
		return fmt.Errorf("wasm-bindgen --version: %w", err)
	}
	if strings.TrimSpace(string(installed)) == "wasm-bindgen "+version {
		return nil
	}
	fmt.Printf("build-listing-wasm: installing wasm-bindgen-cli %s to match Cargo.lock\n", version)
	if err := command(root, "cargo", "install", "wasm-bindgen-cli", "--version", version, "--locked"); err != nil {
		return fmt.Errorf("installing wasm-bindgen-cli %s: %w", version, err)
	}
	return nil
}

func command(dir, name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}
