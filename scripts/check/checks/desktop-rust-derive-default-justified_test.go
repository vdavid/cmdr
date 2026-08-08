package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeFixtureFiles drops rel-path files under base, creating subdirs.
func writeFixtureFiles(t *testing.T, base string, files map[string]string) {
	t.Helper()
	for rel, body := range files {
		full := filepath.Join(base, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
}

// runDeriveDefaultOn writes the supplied files under the app's `file_system/`
// tree and runs the check.
func runDeriveDefaultOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	writeFixtureFiles(t, filepath.Join(root, "apps", "desktop", "src-tauri", "src", "file_system"), files)
	return RunDeriveDefaultJustified(&CheckContext{RootDir: root})
}

func TestDeriveDefault_FlagsAnUnjustifiedDerive(t *testing.T) {
	_, err := runDeriveDefaultOn(t, map[string]string{
		"preflight.rs": `
/// A fact about a file.
#[derive(Clone, Copy, Default, Debug)]
pub struct SourceHint {
    pub is_directory: bool,
}
`,
	})
	if err == nil {
		t.Fatal("expected a violation, got success")
	}
	if !strings.Contains(err.Error(), "preflight.rs:3") {
		t.Errorf("expected preflight.rs:3, got: %s", err.Error())
	}
}

func TestDeriveDefault_HonorsTheDirectiveAbove(t *testing.T) {
	_, err := runDeriveDefaultOn(t, map[string]string{
		"listing.rs": `
/// Progress so far.
// DEFAULT-OK: zero really is "nothing enumerated yet"
#[derive(Debug, Clone, Copy, Default)]
pub struct ListingProgress {
    pub files: usize,
}
`,
	})
	if err != nil {
		t.Fatalf("directive should excuse the derive, got: %s", err.Error())
	}
}

// A derive rarely touches its doc comment: `#[repr(C)]` and `#[cfg(...)]`
// legitimately sit between them, and the directive has to survive that.
func TestDeriveDefault_LooksPastInterveningAttributes(t *testing.T) {
	_, err := runDeriveDefaultOn(t, map[string]string{
		"ffi.rs": `
/// ` + "`malloc_statistics_t`" + ` from the system header.
// DEFAULT-OK: an all-zero stats struct is what the C API fills in
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct MallocStatistics {
    blocks_in_use: u32,
}
`,
	})
	if err != nil {
		t.Fatalf("the directive should reach past the attributes, got: %s", err.Error())
	}
}

func TestDeriveDefault_FailsOnAnOrphanDirective(t *testing.T) {
	_, err := runDeriveDefaultOn(t, map[string]string{
		"clean.rs": `
// DEFAULT-OK: nothing here derives one
pub struct Plain {}
`,
	})
	if err == nil {
		t.Fatal("expected an orphan-directive failure, got success")
	}
	if !strings.Contains(err.Error(), "unused") {
		t.Errorf("expected an unused-directive report, got: %s", err.Error())
	}
}

// A derive with no `Default` in the list is none of this check's business, and
// neither is a longer identifier that merely ends in the word.
func TestDeriveDefault_IgnoresDerivesWithoutDefault(t *testing.T) {
	_, err := runDeriveDefaultOn(t, map[string]string{
		"plain.rs": `
#[derive(Debug, Clone, PartialEq)]
pub struct A {}

#[derive(Debug, MyDefaultish)]
pub struct B {}
`,
	})
	if err != nil {
		t.Fatalf("only a real `Default` is in scope, got: %s", err.Error())
	}
}

// A test double's zero value is a test's problem, in both senses of "test code".
func TestDeriveDefault_IgnoresTestFilesAndInlineTestMods(t *testing.T) {
	_, err := runDeriveDefaultOn(t, map[string]string{
		"strategy_test_support.rs": `
#[derive(Default)]
struct Recording {}
`,
		"listing_progress_tests.rs": `
#[derive(Default)]
struct Counts {}
`,
		"conflict.rs": `
pub fn production() {}

#[cfg(test)]
mod tests {
    #[derive(Default)]
    struct Probe {}
}
`,
	})
	if err != nil {
		t.Fatalf("test code is out of jurisdiction, got: %s", err.Error())
	}
}

// The `cmdr-fs` host stubs are gated `#[cfg(any(test, feature = "testing"))]`,
// not the bare `#[cfg(test)]`. A tracker that only arms on the literal form
// would demand annotations on all six, which is exactly the churn the carve-out
// exists to avoid.
func TestDeriveDefault_IgnoresTheTestingFeatureGatedModForm(t *testing.T) {
	root := t.TempDir()
	seedCmdrFsFixtureWorkspace(t, root)
	writeFixtureFiles(t, filepath.Join(root, "crates", "cmdr-fs", "src"), map[string]string{
		"volume/host/events.rs": `
pub trait VolumeEventSink {}

#[cfg(any(test, feature = "testing"))]
mod recording {
    /// Remembers every transition.
    #[derive(Default)]
    pub struct RecordingVolumeEvents {}
}
`,
	})
	if _, err := RunDeriveDefaultJustified(&CheckContext{RootDir: root}); err != nil {
		t.Fatalf("a `testing`-feature-gated mod is a test double, got: %s", err.Error())
	}
}

// All of `cmdr-fs` is in scope, not just a `file_system/` subdir of it.
func TestDeriveDefault_CoversTheWholeCmdrFsTree(t *testing.T) {
	root := t.TempDir()
	seedCmdrFsFixtureWorkspace(t, root)
	writeFixtureFiles(t, filepath.Join(root, "crates", "cmdr-fs", "src"), map[string]string{
		"volume/types.rs": `
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListingProgress {}
`,
	})
	_, err := RunDeriveDefaultJustified(&CheckContext{RootDir: root})
	if err == nil {
		t.Fatal("expected a violation in cmdr-fs, got success")
	}
	if !strings.Contains(err.Error(), "volume/types.rs:2") {
		t.Errorf("expected volume/types.rs:2, got: %s", err.Error())
	}
}

// Outside the two filesystem trees a `Default` carries no filesystem fact.
func TestDeriveDefault_IgnoresTreesOutsideTheFilesystemOnes(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	writeFixtureFiles(t, filepath.Join(root, "apps", "desktop", "src-tauri", "src"), map[string]string{
		"settings/mod.rs": `
#[derive(Default)]
pub struct Prefs {}
`,
	})
	if _, err := RunDeriveDefaultJustified(&CheckContext{RootDir: root}); err != nil {
		t.Fatalf("outside `file_system/` is out of scope, got: %s", err.Error())
	}
}

// seedCmdrFsFixtureWorkspace builds a one-member workspace whose member is
// `crates/cmdr-fs`, so the cmdr-fs half of the jurisdiction can be exercised.
func seedCmdrFsFixtureWorkspace(t *testing.T, root string) {
	t.Helper()
	mustWrite(t, filepath.Join(root, "Cargo.toml"),
		"[workspace]\nmembers = [\"crates/cmdr-fs\"]\nresolver = \"2\"\n")
	mustWrite(t, filepath.Join(root, "crates", "cmdr-fs", "Cargo.toml"),
		"[package]\nname = \"cmdr-fs\"\nversion = \"0.0.0\"\n")
}
