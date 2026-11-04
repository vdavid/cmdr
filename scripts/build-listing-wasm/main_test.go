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

func TestLockedVersionRefusesAMissingPackage(t *testing.T) {
	if _, err := lockedVersionIn(scan(`[[package]]
name = "serde"
version = "1.0.0"
`), "wasm-bindgen", "Cargo.lock"); err == nil {
		t.Fatal("a missing package resolved")
	}
}
