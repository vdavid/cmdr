package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runLockPoisonOn writes the supplied files into a temporary repo layout
// (rooted at the conventional `apps/desktop/src-tauri/src/` path) and runs the
// check against it, returning the result and any error.
func runLockPoisonOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	for rel, body := range files {
		full := filepath.Join(srcDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunLockPoison(&CheckContext{RootDir: root})
}

func TestLockPoison_FlagsBareUnwrap(t *testing.T) {
	_, err := runLockPoisonOn(t, map[string]string{
		"cache.rs": `
fn read_all(state: &State) {
    let guard = state.entries.lock().unwrap();
    let r = state.config.read().unwrap();
    let mut w = state.config.write().unwrap();
}
`,
	})
	if err == nil {
		t.Fatal("expected violations for bare unwrap, got success")
	}
	if !strings.Contains(err.Error(), "cache.rs:3") {
		t.Errorf("expected lock().unwrap() at cache.rs:3, got: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "cache.rs:4") {
		t.Errorf("expected read().unwrap() at cache.rs:4, got: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "cache.rs:5") {
		t.Errorf("expected write().unwrap() at cache.rs:5, got: %s", err.Error())
	}
}

func TestLockPoison_FlagsNonPoisonExpect(t *testing.T) {
	_, err := runLockPoisonOn(t, map[string]string{
		"open_with.rs": `
fn touch(state: &State) {
    let c = state.cache.lock().expect("open_with cache");
    let mut t = state.tree.write().expect("write tree");
}
`,
	})
	if err == nil {
		t.Fatal("expected violations for non-poison expect, got success")
	}
	if !strings.Contains(err.Error(), "open_with.rs:3") {
		t.Errorf("expected lock().expect(non-poison) at open_with.rs:3, got: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "open_with.rs:4") {
		t.Errorf("expected write().expect(non-poison) at open_with.rs:4, got: %s", err.Error())
	}
}

func TestLockPoison_AllowsPoisonExpect(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"index.rs": `
fn reindex(state: &State) {
    let g = state.machine.lock().expect("INDEXING lock poisoned: half-applied batch");
    let r = state.machine.read().expect("state machine RwLock poisoned mid-transition");
    let w = state.machine.write().expect("poisoned: torn invariant");
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for poison-named expect, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_AllowsIgnorePoisonHelpers(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"store.rs": `
fn use_store(state: &State) {
    let g = state.entries.lock_ignore_poison();
    let r = state.config.read_ignore_poison();
    let mut w = state.config.write_ignore_poison();
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for ignore-poison helpers, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_DoesNotFlagTokioAwait(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"async.rs": `
async fn run(state: &State) {
    let guard = state.async_mutex.lock().await;
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for tokio .lock().await, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_DoesNotFlagIoReadWriteWithArgs(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"io.rs": `
fn copy(reader: &mut impl Read, writer: &mut impl Write) {
    let mut buf = [0u8; 1024];
    let n = reader.read(&mut buf).unwrap();
    writer.write(&buf[..n]).unwrap();
    writer.write_all(&buf[..n]).unwrap();
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for io read/write with args, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_DoesNotFlagTryLock(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"try.rs": `
fn poll(state: &State) {
    let g = state.entries.try_lock().unwrap();
    let r = state.config.try_read().unwrap();
    let w = state.config.try_write().unwrap();
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for try_lock/try_read/try_write, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_HonorsOptOutOnPreviousLine(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"foo.rs": `
fn f(state: &State) {
    // allowed-lock-poison: nothing panics under this lock, proven by construction
    let g = state.entries.lock().unwrap();
}
`,
	})
	if err != nil {
		t.Fatalf("expected success with opt-out on previous line, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_HonorsTrailingOptOut(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"foo.rs": `
fn f(state: &State) {
    let g = state.entries.lock().unwrap(); // allowed-lock-poison: see notes
}
`,
	})
	if err != nil {
		t.Fatalf("expected success with trailing opt-out, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_SkipsDedicatedTestFiles(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"foo_test.rs": `
#[test]
fn t() {
    let g = STATE.lock().unwrap();
}
`,
		"bar_tests.rs": `
fn _x() { let _ = X.read().unwrap(); }
`,
		"tests.rs": `
fn _x() { let _ = X.write().unwrap(); }
`,
	})
	if err != nil {
		t.Fatalf("expected success on dedicated test files, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_SkipsInFileCfgTestMod(t *testing.T) {
	// Deliberate deviation from error-string-match: bare `.lock().unwrap()`
	// inside an in-file `#[cfg(test)]` mod is fine. A poisoned lock in a test
	// means the test already panicked; aborting there is harmless.
	res, err := runLockPoisonOn(t, map[string]string{
		"thing.rs": `
pub fn thing() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let nested = || {
            let g = STATE.lock().unwrap();
        };
        let r = X.read().unwrap();
    }
}
`,
	})
	if err != nil {
		t.Fatalf("expected success: bare unwrap inside #[cfg(test)] mod should not be flagged, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_FlagsAfterCfgTestModCloses(t *testing.T) {
	// Brace-depth tracking must resume scanning once the #[cfg(test)] mod
	// closes; a violation in production code after the mod is still flagged.
	_, err := runLockPoisonOn(t, map[string]string{
		"thing.rs": `
#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let g = STATE.lock().unwrap();
    }
}

fn prod(state: &State) {
    let g = state.entries.lock().unwrap();
}
`,
	})
	if err == nil {
		t.Fatal("expected the production-code violation after the test mod to be flagged")
	}
	if strings.Contains(err.Error(), ":6") {
		t.Errorf("the in-mod site at line 6 must NOT be flagged, got: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "thing.rs:11") {
		t.Errorf("expected the prod site at thing.rs:11, got: %s", err.Error())
	}
}

func TestLockPoison_IgnoresComments(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"foo.rs": `
// Don't write ` + "`" + `state.lock().unwrap()` + "`" + ` in prod code.
//! ` + "`" + `x.read().unwrap()` + "`" + ` is banned too.
fn ok() {}
`,
	})
	if err != nil {
		t.Fatalf("expected success when only comments mention the patterns, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_PassesOnAllowedForms(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"clean.rs": `
fn clean(state: &State) {
    let g = state.entries.lock_ignore_poison();
    let r = state.config.read_ignore_poison();
    let w = state.machine.write().expect("state machine poisoned: torn invariant");
    let a = state.async_mutex.lock().await;
}
`,
	})
	if err != nil {
		t.Fatalf("expected success on file using only allowed forms, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "1 Rust file scanned") {
		t.Errorf("expected scanned count in success message, got: %s", res.Message)
	}
}
