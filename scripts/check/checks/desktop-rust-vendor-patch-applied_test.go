package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

const vendorPatchManifest = `[workspace]
members = ["apps/desktop/src-tauri"]
exclude = ["vendor/mdns-sd"]

[patch.crates-io]
mdns-sd = { path = "vendor/mdns-sd" }
`

// What cargo writes while the patch applies: the vendored copy resolves as a path
// package, which carries no `source`.
const vendorPatchAppliedLock = `version = 4

[[package]]
name = "flume"
version = "0.11.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "da0e4dd2a88388a1f4ccc7c9ce104604dab68d9f408dc34cd45823d5a9069095"

[[package]]
name = "mdns-sd"
version = "0.20.3"
dependencies = [
 "flume",
]
`

// What cargo writes once a dependency bump moves past the vendored version: the
// crate comes from crates.io again, and the patch lands in `[[patch.unused]]` with
// nothing but a build warning to say so. Captured from `cargo generate-lockfile`
// (cargo 1.97.1, 2026-10-01) on a probe crate asking for `mdns-sd = "0.21"` with
// this repo's patch in place.
const vendorPatchUnusedLock = `version = 4

[[package]]
name = "mdns-sd"
version = "0.21.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "1067efe2aadc6967f84c95977dca910e98f3edfd6403efc8fa8508d289ce012d"

[[patch.unused]]
name = "mdns-sd"
version = "0.20.3"
`

func runVendorPatchOn(t *testing.T, manifest, lock string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	mustWrite(t, filepath.Join(root, "Cargo.toml"), manifest)
	mustWrite(t, filepath.Join(root, "Cargo.lock"), lock)
	return RunVendorPatchApplied(&CheckContext{RootDir: root})
}

func TestVendorPatch_PassesWhileThePatchApplies(t *testing.T) {
	result, err := runVendorPatchOn(t, vendorPatchManifest, vendorPatchAppliedLock)
	if err != nil {
		t.Fatalf("the vendored copy resolves, so this should pass, got: %s", err.Error())
	}
	if !strings.Contains(result.Message, "mdns-sd") {
		t.Errorf("the success line should name the patched crate, got: %s", result.Message)
	}
}

// The regression the check exists for: Renovate bumps `mdns-sd` past 0.20.x, cargo
// quietly resolves crates.io, and the retry storm is back.
func TestVendorPatch_FailsWhenABumpLeavesThePatchUnused(t *testing.T) {
	_, err := runVendorPatchOn(t, vendorPatchManifest, vendorPatchUnusedLock)
	if err == nil {
		t.Fatal("expected a failure for an unused patch, got success")
	}
	for _, want := range []string{"mdns-sd", "0.21.4", "crates.io", "vendor/mdns-sd"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected the failure to mention %q, got: %s", want, err.Error())
		}
	}
}

// Two copies resolving at once (the vendored one for one dependent, crates.io for
// another) still ships the unpatched code to whoever takes the second.
func TestVendorPatch_FailsWhenACratesIoCopyResolvesBesideTheVendoredOne(t *testing.T) {
	lock := vendorPatchAppliedLock + `
[[package]]
name = "mdns-sd"
version = "0.21.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
`
	_, err := runVendorPatchOn(t, vendorPatchManifest, lock)
	if err == nil {
		t.Fatal("expected a failure for a second, unpatched copy, got success")
	}
	if !strings.Contains(err.Error(), "0.21.4") {
		t.Errorf("expected the unpatched version named, got: %s", err.Error())
	}
}

// A patch for a crate nothing depends on any more is dead weight: say so, so the
// fork gets dropped instead of silently carried.
func TestVendorPatch_FailsWhenThePatchedCrateIsNoLongerADependency(t *testing.T) {
	lock := `version = 4

[[package]]
name = "flume"
version = "0.11.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
`
	_, err := runVendorPatchOn(t, vendorPatchManifest, lock)
	if err == nil {
		t.Fatal("expected a failure for a patch nothing uses, got success")
	}
	if !strings.Contains(err.Error(), "mdns-sd") {
		t.Errorf("expected the crate named, got: %s", err.Error())
	}
}

func TestVendorPatch_SkipsWithoutAnyPathPatch(t *testing.T) {
	result, err := runVendorPatchOn(t, "[workspace]\nmembers = []\n", vendorPatchAppliedLock)
	if err != nil {
		t.Fatalf("no patch means nothing to guard, got: %s", err.Error())
	}
	if result.Code != ResultSkipped {
		t.Errorf("expected a skip, got code %v: %s", result.Code, result.Message)
	}
}
