package main

// The C toolchain the browser build compiles SQLite with. `rusqlite`'s wasm backend
// (`sqlite-wasm-rs`) always compiles `sqlite3.c` with `cc`, and Apple's clang can't target
// wasm32, so the build uses conda-forge's LLVM clang, installed through mise.
//
// ❗ Into a PRIVATE mise data dir (`~/.cache/cmdr/wasm-toolchain/`), never the default one and
// never on PATH: the conda env ships its own `cc`, `cpp`, `ar`, and `ld`, and mise's shims for
// them would sit ahead of `/usr/bin`, one `mise use` away from switching every native C build
// (bundled SQLite, `ring`, the objc shims) to another compiler. Cargo hears only the two
// wasm-target variables, with absolute paths.
//
// `crates/cmdr-listing-wasm/DETAILS.md` § The build has the why and the evidence.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// The pinned tools. Renovate bumps both (a custom manager in `renovate.json`); keep them on the
// same LLVM release.
const (
	// renovate: datasource=conda depName=conda-forge/clang
	clangTool = "conda:clang@23.1.4"
	// renovate: datasource=conda depName=conda-forge/llvm-tools
	llvmToolsTool = "conda:llvm-tools@23.1.4"
)

// toolchainDirEnv overrides where the private toolchain lives (CI points it at its cache path).
const toolchainDirEnv = "CMDR_WASM_TOOLCHAIN_DIR"

// wasmToolchain is what cargo needs to compile C for wasm32.
type wasmToolchain struct {
	clang string
	ar    string
}

// toolchainDir is the private mise home: `$CMDR_WASM_TOOLCHAIN_DIR`, else `~/.cache/cmdr/wasm-toolchain`.
func toolchainDir() (string, error) {
	if dir := os.Getenv(toolchainDirEnv); dir != "" {
		return filepath.Abs(dir)
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("no home dir for the wasm toolchain: %w", err)
	}
	return filepath.Join(home, ".cache", "cmdr", "wasm-toolchain"), nil
}

// ensureToolchain installs the pinned tools into `dir` (a no-op once they're there) and answers
// their absolute paths.
func ensureToolchain(dir string) (wasmToolchain, error) {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return wasmToolchain{}, err
	}
	env := miseEnv(os.Environ(), dir)

	// `--quiet`: mise otherwise suggests `mise use conda:clang`, exactly what must never happen
	// (the header says why).
	install := exec.Command("mise", "install", "--quiet", clangTool, llvmToolsTool)
	// Run outside the repo, so no project config (and its tool list) is read or trusted.
	install.Dir = dir
	install.Env = env
	install.Stdout = os.Stdout
	install.Stderr = os.Stderr
	if err := install.Run(); err != nil {
		return wasmToolchain{}, fmt.Errorf("mise install %s %s: %w", clangTool, llvmToolsTool, err)
	}

	clangHome, err := miseWhere(dir, env, clangTool)
	if err != nil {
		return wasmToolchain{}, err
	}
	toolsHome, err := miseWhere(dir, env, llvmToolsTool)
	if err != nil {
		return wasmToolchain{}, err
	}
	tc := wasmToolchain{
		clang: filepath.Join(clangHome, "bin", "clang"),
		ar:    filepath.Join(toolsHome, "bin", "llvm-ar"),
	}
	for _, bin := range []string{tc.clang, tc.ar} {
		if _, err := os.Stat(bin); err != nil {
			return wasmToolchain{}, fmt.Errorf("the wasm toolchain has no %s: %w", bin, err)
		}
	}
	return tc, nil
}

func miseWhere(dir string, env []string, tool string) (string, error) {
	cmd := exec.Command("mise", "where", tool)
	cmd.Dir = dir
	cmd.Env = env
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("mise where %s: %w", tool, err)
	}
	return strings.TrimSpace(string(out)), nil
}

// miseEnv is `base` with mise's data and cache dirs moved under `dir`, so nothing lands in (or is
// read from) the default mise home, its shims included.
func miseEnv(base []string, dir string) []string {
	env := without(base, "MISE_DATA_DIR", "MISE_CACHE_DIR")
	return append(env,
		"MISE_DATA_DIR="+filepath.Join(dir, "data"),
		"MISE_CACHE_DIR="+filepath.Join(dir, "cache"),
	)
}

// cargoEnv is `base` plus the two wasm-target compiler variables cargo's `cc` reads. PATH and
// every host compiler variable stay as they were, so native builds can't see this toolchain.
func cargoEnv(base []string, tc wasmToolchain) []string {
	env := without(base, "CC_wasm32_unknown_unknown", "AR_wasm32_unknown_unknown")
	return append(env,
		"CC_wasm32_unknown_unknown="+tc.clang,
		"AR_wasm32_unknown_unknown="+tc.ar,
	)
}

// without drops every `KEY=…` entry for the given keys.
func without(env []string, keys ...string) []string {
	out := make([]string, 0, len(env))
	for _, entry := range env {
		drop := false
		for _, key := range keys {
			if strings.HasPrefix(entry, key+"=") {
				drop = true
				break
			}
		}
		if !drop {
			out = append(out, entry)
		}
	}
	return out
}
