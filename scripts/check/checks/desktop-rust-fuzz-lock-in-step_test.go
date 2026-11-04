package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

func lockWith(packages ...string) string {
	return "version = 4\n\n" + strings.Join(packages, "\n")
}

func registryPkg(name, version string) string {
	return "[[package]]\nname = \"" + name + "\"\nversion = \"" + version + "\"\n" +
		"source = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
}

func pathPkg(name, version string) string {
	return "[[package]]\nname = \"" + name + "\"\nversion = \"" + version + "\"\n"
}

func runFuzzLockInStepOn(t *testing.T, rootLock, fuzzLock string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	mustWrite(t, filepath.Join(root, "Cargo.lock"), rootLock)
	if fuzzLock != "" {
		mustWrite(t, filepath.Join(root, "fuzz", "Cargo.lock"), fuzzLock)
	}
	return RunFuzzLockInStep(&CheckContext{RootDir: root})
}

func TestFuzzLockInStep_PassesWhenSharedCratesMatch(t *testing.T) {
	result, err := runFuzzLockInStepOn(t,
		lockWith(registryPkg("twox-hash", "2.1.5"), registryPkg("serde", "1.0.228"), pathPkg("cmdr-adb", "0.1.0")),
		lockWith(registryPkg("twox-hash", "2.1.5"), pathPkg("cmdr-adb", "0.1.0"), registryPkg("libfuzzer-sys", "0.4.10")))
	if err != nil {
		t.Fatalf("every shared crate matches, so this should pass, got: %s", err.Error())
	}
	if !strings.Contains(result.Message, "2 shared crates") {
		t.Errorf("the success line should count the shared crates, got: %s", result.Message)
	}
}

// The regression the check exists for: `fuzz/Cargo.lock` kept `twox-hash` 2.1.2
// while the root moved to 2.1.5, and the lane panicked on a bug the app never had.
func TestFuzzLockInStep_FailsWhenTheFuzzLockLags(t *testing.T) {
	_, err := runFuzzLockInStepOn(t,
		lockWith(registryPkg("twox-hash", "2.1.5")),
		lockWith(registryPkg("twox-hash", "2.1.2")))
	if err == nil {
		t.Fatal("expected a failure for a lagging fuzz lock, got success")
	}
	for _, want := range []string{"twox-hash", "2.1.2", "2.1.5", "cargo update -p twox-hash --precise 2.1.5", "fuzz/"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected the failure to mention %q, got: %s", want, err.Error())
		}
	}
}

// Ahead is as wrong as behind: the lane then fuzzes code the app doesn't ship.
func TestFuzzLockInStep_FailsWhenTheFuzzLockIsAhead(t *testing.T) {
	_, err := runFuzzLockInStepOn(t,
		lockWith(registryPkg("serde", "1.0.220")),
		lockWith(registryPkg("serde", "1.0.228")))
	if err == nil {
		t.Fatal("expected a failure for a fuzz lock ahead of the root, got success")
	}
}

// A crate can legitimately resolve on two semver lines at once (`zip` 4 and 9). Each
// line is compared on its own, and a line only one lock has is nobody's business:
// the fuzz crate's own dependencies (or the app's that no target links) pull it.
func TestFuzzLockInStep_ComparesEachSemverLineOnItsOwn(t *testing.T) {
	root := lockWith(registryPkg("zip", "4.6.1"), pathPkg("zip", "9.2.0"), registryPkg("rand", "0.9.2"))
	inStep := lockWith(pathPkg("zip", "9.2.0"), registryPkg("rand", "0.8.5"))
	if _, err := runFuzzLockInStepOn(t, root, inStep); err != nil {
		t.Fatalf("zip 9 matches and rand 0.8 is a line the root doesn't have, got: %s", err.Error())
	}

	lagging := lockWith(registryPkg("zip", "4.6.0"), pathPkg("zip", "9.2.0"))
	_, err := runFuzzLockInStepOn(t, root, lagging)
	if err == nil {
		t.Fatal("expected a failure for zip 4.6.0 against the root's 4.6.1, got success")
	}
	// With two copies in the fuzz lock, a bare `-p zip` is ambiguous to cargo.
	if !strings.Contains(err.Error(), "cargo update -p zip@4.6.0 --precise 4.6.1") {
		t.Errorf("expected a disambiguated fix command, got: %s", err.Error())
	}
	if strings.Contains(err.Error(), "9.2.0") {
		t.Errorf("the matching zip 9 line shouldn't be reported, got: %s", err.Error())
	}
}

func TestFuzzLockInStep_ReportsEveryMismatch(t *testing.T) {
	_, err := runFuzzLockInStepOn(t,
		lockWith(registryPkg("twox-hash", "2.1.5"), registryPkg("lz4_flex", "0.11.5")),
		lockWith(registryPkg("twox-hash", "2.1.2"), registryPkg("lz4_flex", "0.11.3")))
	if err == nil {
		t.Fatal("expected a failure, got success")
	}
	for _, want := range []string{"twox-hash", "lz4_flex"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected every mismatch listed, missing %q in: %s", want, err.Error())
		}
	}
}

func TestFuzzLockInStep_SkipsWithoutAFuzzLock(t *testing.T) {
	result, err := runFuzzLockInStepOn(t, lockWith(registryPkg("serde", "1.0.228")), "")
	if err != nil {
		t.Fatalf("no fuzz lock means nothing to compare, got: %s", err.Error())
	}
	if result.Code != ResultSkipped {
		t.Errorf("expected a skip, got code %v: %s", result.Code, result.Message)
	}
}
