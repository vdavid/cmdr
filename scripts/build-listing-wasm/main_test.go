package main

import (
	"bufio"
	"strings"
	"testing"
)

func scan(text string) *bufio.Scanner {
	return bufio.NewScanner(strings.NewReader(text))
}

func TestLockedVersionReadsThePackagesOwnVersion(t *testing.T) {
	lock := `
[[package]]
name = "wasm-bindgen-futures"
version = "0.4.76"

[[package]]
name = "wasm-bindgen"
version = "0.2.126"
dependencies = [
 "cfg-if",
]
`
	got, err := lockedVersionIn(scan(lock), "wasm-bindgen", "Cargo.lock")
	if err != nil || got != "0.2.126" {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestLockedVersionRefusesTwoVersions(t *testing.T) {
	lock := `
[[package]]
name = "wasm-bindgen"
version = "0.2.126"

[[package]]
name = "wasm-bindgen"
version = "0.2.130"
`
	if _, err := lockedVersionIn(scan(lock), "wasm-bindgen", "Cargo.lock"); err == nil {
		t.Fatal("two versions resolved to one")
	}
}

func envValue(env []string, key string) (string, bool) {
	for _, entry := range env {
		if strings.HasPrefix(entry, key+"=") {
			return strings.TrimPrefix(entry, key+"="), true
		}
	}
	return "", false
}

func TestCargoEnvSetsOnlyTheWasmTargetCompilers(t *testing.T) {
	base := []string{"PATH=/usr/bin:/bin", "CC=/usr/bin/cc", "CC_wasm32_unknown_unknown=/stale/clang"}
	env := cargoEnv(base, wasmToolchain{clang: "/tc/clang/bin/clang", ar: "/tc/tools/bin/llvm-ar"})
	if got, _ := envValue(env, "PATH"); got != "/usr/bin:/bin" {
		t.Fatalf("PATH changed: %q", got)
	}
	if got, _ := envValue(env, "CC"); got != "/usr/bin/cc" {
		t.Fatalf("the host compiler changed: %q", got)
	}
	if got, _ := envValue(env, "CC_wasm32_unknown_unknown"); got != "/tc/clang/bin/clang" {
		t.Fatalf("wasm CC = %q", got)
	}
	if got, _ := envValue(env, "AR_wasm32_unknown_unknown"); got != "/tc/tools/bin/llvm-ar" {
		t.Fatalf("wasm AR = %q", got)
	}
	count := 0
	for _, entry := range env {
		if strings.HasPrefix(entry, "CC_wasm32_unknown_unknown=") {
			count++
		}
	}
	if count != 1 {
		t.Fatalf("%d wasm CC entries, want 1", count)
	}
}

func TestMiseEnvKeepsTheToolchainOutOfTheDefaultMiseHome(t *testing.T) {
	base := []string{"PATH=/usr/bin", "MISE_DATA_DIR=/home/me/.local/share/mise"}
	env := miseEnv(base, "/cache/wasm-toolchain")
	if got, _ := envValue(env, "MISE_DATA_DIR"); got != "/cache/wasm-toolchain/data" {
		t.Fatalf("MISE_DATA_DIR = %q", got)
	}
	if got, _ := envValue(env, "MISE_CACHE_DIR"); got != "/cache/wasm-toolchain/cache" {
		t.Fatalf("MISE_CACHE_DIR = %q", got)
	}
	if got, _ := envValue(env, "PATH"); got != "/usr/bin" {
		t.Fatalf("PATH changed: %q", got)
	}
}

func TestBothToolsArePinnedToOneLLVMRelease(t *testing.T) {
	version := func(tool string) string { return tool[strings.LastIndex(tool, "@")+1:] }
	if version(clangTool) != version(llvmToolsTool) {
		t.Fatalf("%s and %s are on different releases", clangTool, llvmToolsTool)
	}
}

func TestLockedVersionRefusesAMissingPackage(t *testing.T) {
	if _, err := lockedVersionIn(scan(`[[package]]
name = "serde"
version = "1.0.0"
`), "wasm-bindgen", "Cargo.lock"); err == nil {
		t.Fatal("a missing package resolved")
	}
}
