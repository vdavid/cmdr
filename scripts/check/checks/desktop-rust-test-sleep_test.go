package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runTestSleepOn writes the supplied files under `apps/desktop/src-tauri/src/`
// (rel paths keep their subdirs) and runs the check.
func runTestSleepOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
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
	return RunTestSleep(&CheckContext{RootDir: root})
}

func TestTestSleep_FlagsSleepInDedicatedTestFile(t *testing.T) {
	_, err := runTestSleepOn(t, map[string]string{
		"feature_test.rs": `
fn helper() {
    std::thread::sleep(Duration::from_millis(50));
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

func TestTestSleep_FlagsTokioSleep(t *testing.T) {
	_, err := runTestSleepOn(t, map[string]string{
		"tests.rs": `
async fn t() {
    tokio::time::sleep(Duration::from_secs(1)).await;
}
`,
	})
	if err == nil {
		t.Fatal("expected violation for tokio::time::sleep, got success")
	}
}

func TestTestSleep_FlagsSleepInsideCfgTestModOfProductionFile(t *testing.T) {
	_, err := runTestSleepOn(t, map[string]string{
		"session.rs": `
pub fn run() {
    std::thread::sleep(Duration::from_millis(5)); // production: must NOT be flagged
}

#[cfg(test)]
mod tests {
    #[test]
    fn slow() {
        thread::sleep(Duration::from_millis(20));
    }
}
`,
	})
	if err == nil {
		t.Fatal("expected the sleep inside the cfg(test) mod to be flagged, got success")
	}
	// The production sleep on line 3 must NOT appear; only the in-test one on line 11.
	if strings.Contains(err.Error(), "session.rs:3") {
		t.Errorf("production sleep was wrongly flagged: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "session.rs:10") {
		t.Errorf("expected the in-test sleep at session.rs:10, got: %s", err.Error())
	}
}

func TestTestSleep_IgnoresProductionSleepOutsideTestMod(t *testing.T) {
	res, err := runTestSleepOn(t, map[string]string{
		"walker.rs": `
pub fn walk() {
    std::thread::sleep(Duration::from_millis(100));
}
`,
	})
	if err != nil {
		t.Fatalf("production-only sleep must be clean, got: %v", err)
	}
	if !strings.Contains(res.Message, "no fixed sleeps in test code") {
		t.Errorf("unexpected success message: %s", res.Message)
	}
}

func TestTestSleep_DirectiveOnPreviousLineSuppresses(t *testing.T) {
	res, err := runTestSleepOn(t, map[string]string{
		"pace_tests.rs": `
fn load_generator() {
    // allowed-test-sleep: background load generator; the delay is the load
    std::thread::sleep(Duration::from_millis(5));
}
`,
	})
	if err != nil {
		t.Fatalf("directive'd sleep must pass, got: %v", err)
	}
	if !strings.Contains(res.Message, "no fixed sleeps in test code") {
		t.Errorf("unexpected message: %s", res.Message)
	}
}

func TestTestSleep_TrailingDirectiveSuppresses(t *testing.T) {
	_, err := runTestSleepOn(t, map[string]string{
		"tests.rs": `
fn t() {
    thread::sleep(POLL); // allowed-test-sleep: the sanctioned poll interval
}
`,
	})
	if err != nil {
		t.Fatalf("trailing directive must suppress, got: %v", err)
	}
}

func TestTestSleep_OrphanDirectiveFails(t *testing.T) {
	_, err := runTestSleepOn(t, map[string]string{
		"tests.rs": `
fn t() {
    // allowed-test-sleep: this excuses nothing now
    do_work();
}
`,
	})
	if err == nil {
		t.Fatal("expected orphan-directive failure, got success")
	}
	if !strings.Contains(err.Error(), "unused") {
		t.Errorf("expected an unused-directive message, got: %s", err.Error())
	}
}

func TestTestSleep_DirectiveInsideProductionCodeIsNotTracked(t *testing.T) {
	// A directive outside any test region is not in jurisdiction, so it is neither
	// honored nor reported as an orphan: it's just an ordinary comment.
	res, err := runTestSleepOn(t, map[string]string{
		"prod.rs": `
pub fn run() {
    // allowed-test-sleep: stray comment in production code
    do_work();
}
`,
	})
	if err != nil {
		t.Fatalf("a stray directive in production code must not fail the check, got: %v", err)
	}
	if !strings.Contains(res.Message, "no fixed sleeps in test code") {
		t.Errorf("unexpected message: %s", res.Message)
	}
}

func TestTestSleep_UnderTestsDirIsTestCode(t *testing.T) {
	_, err := runTestSleepOn(t, map[string]string{
		"indexing/tests/external_drive_fixture.rs": `
fn setup() {
    std::thread::sleep(Duration::from_secs(2));
}
`,
	})
	if err == nil {
		t.Fatal("a file under a /tests/ dir is test code and its sleep must be flagged")
	}
}

func TestTestSleep_TestSupportFileIsTestCode(t *testing.T) {
	_, err := runTestSleepOn(t, map[string]string{
		"file_viewer/search_cancel_test_support.rs": `
fn park() {
    tokio::time::sleep(Duration::from_millis(10)).await;
}
`,
	})
	if err == nil {
		t.Fatal("a *test_support*.rs file is test code and its sleep must be flagged")
	}
}

func TestTestSleep_CleanTestFilePasses(t *testing.T) {
	res, err := runTestSleepOn(t, map[string]string{
		"tests.rs": `
fn t() {
    crate::test_support::wait_until(Duration::from_secs(5), "the work to land", || done());
}
`,
	})
	if err != nil {
		t.Fatalf("a test file that waits on a condition must pass, got: %v", err)
	}
	if !strings.Contains(res.Message, "no fixed sleeps") {
		t.Errorf("unexpected message: %s", res.Message)
	}
}
