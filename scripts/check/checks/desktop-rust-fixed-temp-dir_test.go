package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runFixedTempDirOn writes the supplied files under `apps/desktop/src-tauri/src/`
// (rel paths keep their subdirs) and runs the check.
func runFixedTempDirOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	for rel, body := range files {
		full := filepath.Join(srcDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunFixedTempDir(&CheckContext{RootDir: root})
}

func TestFixedTempDir_FlagsFixtureInDedicatedTestFile(t *testing.T) {
	_, err := runFixedTempDirOn(t, map[string]string{
		"feature_test.rs": `
fn fixture() -> PathBuf {
    let dir = std::env::temp_dir().join("cmdr_feature_test");
    fs::create_dir_all(&dir).unwrap();
    dir
}
`,
	})
	if err == nil {
		t.Fatal("expected violation, got success")
	}
	if !strings.Contains(err.Error(), "feature_test.rs:3") {
		t.Errorf("expected feature_test.rs:3, got: %s", err.Error())
	}
}

func TestFixedTempDir_FlagsFixtureInsideCfgTestModOfProductionFile(t *testing.T) {
	_, err := runFixedTempDirOn(t, map[string]string{
		"session.rs": `
pub fn staging() -> PathBuf {
    std::env::temp_dir().join("cmdr-staging")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let dir = std::env::temp_dir().join("cmdr_session_test");
    }
}
`,
	})
	if err == nil {
		t.Fatal("expected violation inside the cfg(test) mod, got success")
	}
	if !strings.Contains(err.Error(), "session.rs:12") {
		t.Errorf("expected session.rs:12, got: %s", err.Error())
	}
	// The production staging path above the test mod must NOT be flagged.
	if strings.Contains(err.Error(), "session.rs:3") {
		t.Errorf("production temp-dir use was flagged: %s", err.Error())
	}
}

func TestFixedTempDir_IgnoresProductionFile(t *testing.T) {
	result, err := runFixedTempDirOn(t, map[string]string{
		"updater.rs": `
pub fn staging_dir() -> PathBuf {
    std::env::temp_dir().join("cmdr-update-staging")
}
`,
	})
	if err != nil {
		t.Fatalf("production code is out of jurisdiction, got: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got %v", result.Code)
	}
}

func TestFixedTempDir_HonorsDirectiveAboveAndTrailing(t *testing.T) {
	_, err := runFixedTempDirOn(t, map[string]string{
		"above_test.rs": `
fn t() {
    // allowed-fixed-temp-dir: the temp root IS the assertion
    assert!(dir.starts_with(std::env::temp_dir()));
}
`,
		"trailing_test.rs": `
fn t() {
    assert!(dir.starts_with(std::env::temp_dir())); // allowed-fixed-temp-dir: same
}
`,
	})
	if err != nil {
		t.Fatalf("expected both directives to be honored, got: %v", err)
	}
}

// A `test_fixtures.rs` helper module is gated at its declaration site
// (`#[cfg(test)] mod test_fixtures;`), so there's no in-file marker for the
// region tracker to find. isRustTestPath has to recognize it by name.
func TestFixedTempDir_FlagsTestFixturesHelperModule(t *testing.T) {
	_, err := runFixedTempDirOn(t, map[string]string{
		"git/test_fixtures.rs": `
pub(super) fn temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cmdr_git_{name}"))
}
`,
	})
	if err == nil {
		t.Fatal("expected a test_fixtures helper module to be in jurisdiction, got success")
	}
}

// A doc comment naming the anti-pattern is prose, not a fixture. `TestDir`'s own
// "why not env::temp_dir()" rationale lives in exactly such a comment.
func TestFixedTempDir_IgnoresDocCommentMentions(t *testing.T) {
	_, err := runFixedTempDirOn(t, map[string]string{
		"test_support.rs": `
//! Scratch directories for tests.
//!
//! Don't hand-roll std::env::temp_dir().join("cmdr_foo_test"): every process
//! on the machine shares that path.

/// Prefer this over std::env::temp_dir().
pub struct TestDir(tempfile::TempDir);
`,
	})
	if err != nil {
		t.Fatalf("doc comments must not be flagged, got: %v", err)
	}
}

func TestFixedTempDir_ReportsOrphanedDirective(t *testing.T) {
	_, err := runFixedTempDirOn(t, map[string]string{
		"stale_test.rs": `
fn t() {
    // allowed-fixed-temp-dir: nothing below this uses the temp root any more
    let dir = TestDir::new("stale");
}
`,
	})
	if err == nil {
		t.Fatal("expected an orphaned directive to be reported, got success")
	}
	if !strings.Contains(err.Error(), "stale_test.rs") {
		t.Errorf("expected the orphan report to name stale_test.rs, got: %s", err.Error())
	}
}
