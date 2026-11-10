package checks

import (
	"fmt"
	"reflect"
	"testing"
)

const pnpmBootstrapRootFixture = `{"name": "cmdr", "packageManager": "pnpm@12.11.2"}`

func pnpmBootstrapFixtures(manifestVersion, rootDep, lockedVersion, integrity string) ([]byte, []byte) {
	manifest := fmt.Sprintf(`{"private": true, "dependencies": {"pnpm": %q}}`, manifestVersion)
	lockfile := fmt.Sprintf(`{"lockfileVersion": 3, "packages": {
		"": {"dependencies": {"pnpm": %q}},
		"node_modules/@pnpm/exe.linux-arm64": {"version": "9.9.9", "integrity": "sha512-x", "optional": true},
		"node_modules/pnpm": {"version": %q, "integrity": %q}
	}}`, rootDep, lockedVersion, integrity)
	return []byte(manifest), []byte(lockfile)
}

func TestPnpmBootstrapInStepPasses(t *testing.T) {
	manifest, lockfile := pnpmBootstrapFixtures("12.11.2", "12.11.2", "12.11.2", "sha512-abc")
	want, mismatches, err := pnpmBootstrapMismatches([]byte(pnpmBootstrapRootFixture), manifest, lockfile)
	if err != nil {
		t.Fatalf("pnpmBootstrapMismatches: %v", err)
	}
	if want != "12.11.2" || len(mismatches) != 0 {
		t.Errorf("got want=%q mismatches=%v, want 12.11.2 and none", want, mismatches)
	}
}

func TestPnpmBootstrapInStepFlagsEachDriftedPin(t *testing.T) {
	manifest, lockfile := pnpmBootstrapFixtures("12.12.0", "12.12.0", "12.10.0", "sha512-abc")
	_, mismatches, err := pnpmBootstrapMismatches([]byte(pnpmBootstrapRootFixture), manifest, lockfile)
	if err != nil {
		t.Fatalf("pnpmBootstrapMismatches: %v", err)
	}
	wantLines := []string{
		pnpmBootstrapManifest + " `dependencies.pnpm`: 12.12.0",
		pnpmBootstrapLockfile + " root `dependencies.pnpm`: 12.12.0",
		pnpmBootstrapLockfile + " `node_modules/pnpm`: 12.10.0",
	}
	if !reflect.DeepEqual(mismatches, wantLines) {
		t.Errorf("mismatches = %v, want %v", mismatches, wantLines)
	}
}

func TestPnpmBootstrapInStepFlagsAMissingIntegrity(t *testing.T) {
	manifest, lockfile := pnpmBootstrapFixtures("12.11.2", "12.11.2", "12.11.2", "")
	_, mismatches, err := pnpmBootstrapMismatches([]byte(pnpmBootstrapRootFixture), manifest, lockfile)
	if err != nil {
		t.Fatalf("pnpmBootstrapMismatches: %v", err)
	}
	if len(mismatches) != 1 {
		t.Errorf("mismatches = %v, want one line about the missing integrity", mismatches)
	}
}

func TestPnpmBootstrapInStepFlagsAMissingLockEntry(t *testing.T) {
	manifest, _ := pnpmBootstrapFixtures("12.11.2", "12.11.2", "12.11.2", "sha512-abc")
	_, mismatches, err := pnpmBootstrapMismatches([]byte(pnpmBootstrapRootFixture), manifest, []byte(`{"packages": {}}`))
	if err != nil {
		t.Fatalf("pnpmBootstrapMismatches: %v", err)
	}
	if len(mismatches) != 2 {
		t.Errorf("mismatches = %v, want the root dep and the locked package both missing", mismatches)
	}
}

func TestPnpmBootstrapInStepIgnoresACorepackHashSuffix(t *testing.T) {
	manifest, lockfile := pnpmBootstrapFixtures("12.11.2", "12.11.2", "12.11.2", "sha512-abc")
	root := []byte(`{"packageManager": "pnpm@12.11.2+sha512.deadbeef"}`)
	_, mismatches, err := pnpmBootstrapMismatches(root, manifest, lockfile)
	if err != nil || len(mismatches) != 0 {
		t.Errorf("got mismatches=%v err=%v, want none", mismatches, err)
	}
}

func TestPnpmBootstrapInStepRejectsANonPnpmPackageManager(t *testing.T) {
	manifest, lockfile := pnpmBootstrapFixtures("12.11.2", "12.11.2", "12.11.2", "sha512-abc")
	if _, _, err := pnpmBootstrapMismatches([]byte(`{"packageManager": "yarn@4.0.0"}`), manifest, lockfile); err == nil {
		t.Fatal("expected an error when packageManager doesn't name pnpm")
	}
}
