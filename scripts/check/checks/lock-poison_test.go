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
	seedAppFixtureWorkspace(t, root)
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
	return runLockPoisonIn(t, root)
}

// runLockPoisonIn runs the check against an already-populated fixture repo, for
// the tests that need to seed an allowlist alongside the sources.
func runLockPoisonIn(t *testing.T, root string) (CheckResult, error) {
	t.Helper()
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

func TestLockPoison_FlagsOrphanedOptOut(t *testing.T) {
	_, err := runLockPoisonOn(t, map[string]string{
		"state.rs": `
fn get(state: &State) -> u32 {
    // allowed-lock-poison: stale, the bare unwrap below was since migrated
    *state.value.lock_ignore_poison()
}
`,
	})
	if err == nil {
		t.Fatal("expected orphaned opt-out violation, got success")
	}
	if !strings.Contains(err.Error(), "unused") || !strings.Contains(err.Error(), "state.rs:3") {
		t.Errorf("expected unused-directive report at state.rs:3, got: %s", err.Error())
	}
}

func TestLockPoison_IgnoresOrphanInsideTestMod(t *testing.T) {
	// Test mods are skipped by the check entirely, so a directive there is
	// outside its jurisdiction (test code may use bare unwraps freely).
	res, err := runLockPoisonOn(t, map[string]string{
		"state.rs": `
fn get() -> u32 { 0 }

#[cfg(test)]
mod tests {
    fn helper(state: &State) -> u32 {
        // allowed-lock-poison: leftover from a refactor
        *state.value.lock().unwrap()
    }
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for directive inside test mod, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_SkipsFilesUnderATestsDir(t *testing.T) {
	// A themed test module under a `tests/` directory is test code just like a
	// `tests.rs` is, so bare `.lock().unwrap()` is fine there too.
	res, err := runLockPoisonOn(t, map[string]string{
		"indexing/reconcile/tests/routing.rs": `
#[test]
fn t() {
    let g = STATE.lock().unwrap();
}
`,
	})
	if err != nil {
		t.Fatalf("expected success on a file under a tests/ dir, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

// --- The swallow lane: a lock result discarded without recording intent ---

// seedLockPoisonAllowlist writes the swallow-lane allowlist into a fixture repo,
// so a test can exercise the suppress / ratchet / shrink-wrap behavior.
func seedLockPoisonAllowlist(t *testing.T, root string, files map[string]int) {
	t.Helper()
	list := lockPoisonAllowlist{Files: files}
	path := filepath.Join(root, "scripts", "check", "checks", "lock-poison-allowlist.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := writeJSONAllowlist(path, list); err != nil {
		t.Fatalf("seed allowlist: %v", err)
	}
}

func TestLockPoison_WarnsOnIfLetOkSwallow(t *testing.T) {
	// The real bug: `if let Ok(guard) = cache().lock()` skips the block on
	// poison, and the caller sees an empty recents list with no clue why.
	res, err := runLockPoisonOn(t, map[string]string{
		"recents.rs": `
fn entries() -> Vec<Entry> {
    let mut out = Vec::new();
    if let Ok(guard) = cache().lock() {
        out.extend(guard.iter().cloned());
    }
    out
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "recents.rs:4") {
		t.Errorf("expected the swallowing `if let Ok` at recents.rs:4, got: %s", res.Message)
	}
}

func TestLockPoison_WarnsOnMatchErrEarlyReturn(t *testing.T) {
	// The real bug: on poison the watcher thread returns and never watches
	// again for the rest of the session.
	res, err := runLockPoisonOn(t, map[string]string{
		"watcher.rs": `
fn check_for_mount_changes() {
    let mut known_guard = match known.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    known_guard.clear();
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "watcher.rs:3") {
		t.Errorf("expected the swallowing `match` at watcher.rs:3, got: %s", res.Message)
	}
}

func TestLockPoison_FindsErrArmWellBelowTheMatch(t *testing.T) {
	// The Err arm can sit many lines under the `match`, so the classifier reads
	// the whole match block rather than peeking at the next line or two.
	res, err := runLockPoisonOn(t, map[string]string{
		"deep.rs": `
fn drain() {
    let guard = match STATE.lock() {
        Ok(g) => {
            trace!("took the state lock");
            g
        }
        Err(_) => {
            warn!("state lock poisoned");
            return;
        }
    };
    guard.drain();
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "deep.rs:3") {
		t.Errorf("expected the swallowing `match` at deep.rs:3, got: %s", res.Message)
	}
}

func TestLockPoison_WarnsOnLetElseSwallow(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"registry.rs": `
fn clear_all() {
    let Ok(mut reg) = INDEX_REGISTRY.lock() else {
        return;
    };
    reg.clear();
}

fn peek() -> bool {
    let Ok(reg) = INDEX_REGISTRY.lock() else { return false };
    reg.is_empty()
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	for _, want := range []string{"registry.rs:3", "registry.rs:10"} {
		if !strings.Contains(res.Message, want) {
			t.Errorf("expected the swallowing let-else at %s, got: %s", want, res.Message)
		}
	}
}

func TestLockPoison_WarnsOnLockOk(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"cache.rs": `
fn lookup(key: &str) -> Option<String> {
    let cache = share_cache().lock().ok()?;
    cache.get(key).cloned()
}

fn handle() -> Option<Handle> {
    MCP_HANDLE.lock().ok().and_then(|mut guard| guard.take())
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	for _, want := range []string{"cache.rs:3", "cache.rs:8"} {
		if !strings.Contains(res.Message, want) {
			t.Errorf("expected the swallowing `.ok()` at %s, got: %s", want, res.Message)
		}
	}
}

func TestLockPoison_AcceptsRecoveringAndPropagatingHandlers(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"handled.rs": `
fn recovered() {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.clear();
}

fn propagated() -> Result<(), IndexError> {
    let Ok(reg) = INDEX_REGISTRY.lock() else {
        return Err(IndexError::RegistryUnavailable);
    };
    reg.flush()
}

fn aborted() {
    let guard = match MACHINE.lock() {
        Ok(g) => g,
        Err(_) => panic!("state machine poisoned: torn invariant"),
    };
    guard.step();
}

fn wildcard_recovers() {
    let mut guard = match STATE.lock() {
        Ok(g) => g,
        _ => STATE.lock_ignore_poison(),
    };
    guard.clear();
}

fn else_branch_recovers(state: &State) {
    if let Ok(mut g) = state.entries.lock() {
        g.clear();
    } else {
        state.entries.lock_ignore_poison().clear();
    }
}
`,
	})
	if err != nil {
		t.Fatalf("expected success on handlers that record intent, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_WarnsOnElseBranchThatSubstitutesEmptyData(t *testing.T) {
	// An `else` arm is not intent on its own: handing back an empty Vec is the
	// silently-emptied-list bug with extra steps.
	res, err := runLockPoisonOn(t, map[string]string{
		"status.rs": `
fn removed_ids() -> Vec<String> {
    if let Ok(mut cache) = STATUS_CACHE.write() {
        cache.drain().collect()
    } else {
        Vec::new()
    }
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "status.rs:3") {
		t.Errorf("expected the empty-substituting else at status.rs:3, got: %s", res.Message)
	}
}

func TestLockPoison_SwallowLaneIgnoresTokioAndTryLock(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"async.rs": `
async fn run(state: &State) {
    let guard = state.async_mutex.lock().await;
    if let Ok(g) = state.entries.try_lock() {
        drop(g);
    }
    let mut buf = [0u8; 8];
    if let Ok(n) = reader.read(&mut buf) {
        drop(n);
    }
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for tokio/try_lock/io shapes, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_SwallowLaneHonorsOptOut(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"opt.rs": `
fn f(state: &State) {
    // allowed-lock-poison: best-effort breadcrumb, losing one is fine
    if let Ok(mut g) = state.entries.lock() {
        g.push(1);
    }
    if let Ok(mut g) = state.other.lock() { // allowed-lock-poison: same
        g.push(1);
    }
}
`,
	})
	if err != nil {
		t.Fatalf("expected success with opt-out comments, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_SwallowLaneSkipsTestMods(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"thing.rs": `
pub fn thing() {}

#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        if let Ok(g) = STATE.lock() {
            assert!(g.is_empty());
        }
    }
}
`,
		"other_test.rs": `
fn helper() {
    let Ok(g) = STATE.lock() else { return };
    drop(g);
}
`,
	})
	if err != nil {
		t.Fatalf("expected success for swallows in test code, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_AllowlistSuppressesKnownSwallows(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	body := `
fn a(state: &State) {
    if let Ok(mut g) = state.entries.lock() {
        g.push(1);
    }
    if let Ok(mut g) = state.other.lock() {
        g.push(2);
    }
}
`
	if err := os.WriteFile(filepath.Join(srcDir, "known.rs"), []byte(body), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	seedLockPoisonAllowlist(t, root, map[string]int{"apps/desktop/src-tauri/src/known.rs": 2})

	res, err := runLockPoisonIn(t, root)
	if err != nil {
		t.Fatalf("expected success for allowlisted swallows, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLockPoison_AllowlistRatchetsDownAndWarnsOnGrowth(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	shrunk := `
fn a(state: &State) {
    if let Ok(mut g) = state.entries.lock() {
        g.push(1);
    }
}
`
	if err := os.WriteFile(filepath.Join(srcDir, "shrunk.rs"), []byte(shrunk), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	grown := `
fn b(state: &State) {
    if let Ok(mut g) = state.entries.lock() {
        g.push(1);
    }
    if let Ok(mut g) = state.other.lock() {
        g.push(2);
    }
}
`
	if err := os.WriteFile(filepath.Join(srcDir, "grown.rs"), []byte(grown), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	seedLockPoisonAllowlist(t, root, map[string]int{
		"apps/desktop/src-tauri/src/shrunk.rs": 3,
		"apps/desktop/src-tauri/src/grown.rs":  1,
		"apps/desktop/src-tauri/src/gone.rs":   4,
	})

	res, err := runLockPoisonIn(t, root)
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning for the grown file, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "grown.rs:6") {
		t.Errorf("expected the site over budget at grown.rs:6, got: %s", res.Message)
	}
	list := loadLockPoisonAllowlist(root)
	if got := list.Files["apps/desktop/src-tauri/src/shrunk.rs"]; got != 1 {
		t.Errorf("expected shrunk.rs ratcheted 3 → 1, got %d", got)
	}
	if _, ok := list.Files["apps/desktop/src-tauri/src/gone.rs"]; ok {
		t.Error("expected the entry for the deleted file to be dropped")
	}
	if got := list.Files["apps/desktop/src-tauri/src/grown.rs"]; got != 1 {
		t.Errorf("expected grown.rs to keep its contract at 1, got %d", got)
	}
}

func TestLockPoison_HardViolationsStillFailAndNameTheSwallowsToo(t *testing.T) {
	_, err := runLockPoisonOn(t, map[string]string{
		"mixed.rs": `
fn f(state: &State) {
    let g = state.entries.lock().unwrap();
    if let Ok(mut o) = state.other.lock() {
        o.push(1);
    }
}
`,
	})
	if err == nil {
		t.Fatal("expected the bare unwrap to fail the check")
	}
	if !strings.Contains(err.Error(), "mixed.rs:3") {
		t.Errorf("expected the bare unwrap at mixed.rs:3, got: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "mixed.rs:4") {
		t.Errorf("expected the swallow at mixed.rs:4 to be reported alongside, got: %s", err.Error())
	}
}

func TestLockPoison_ReadsPastCommentsAroundTheErrArm(t *testing.T) {
	// A comment between the arms must not hide the `Err` arm, and prose in it
	// must not read as recorded intent. Both would silently un-flag a site.
	res, err := runLockPoisonOn(t, map[string]string{
		"state.rs": `
fn enrich(&self, event: &mut Progress) {
    let stats = match self.estimator.lock() {
        Ok(mut est) => est.update(EtaSample {
            now,
            phase: event.phase,
        }),
        // Poisoned mutex (another thread panicked). We could panic! here, but
        // progress events are advisory.
        Err(_) => return,
    };
    event.eta_seconds = stats.eta_seconds;
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "state.rs:3") {
		t.Errorf("expected the swallowing `match` at state.rs:3, got: %s", res.Message)
	}
}

func TestLockPoison_BracesInStringsAndCommentsDoNotDerailTheParser(t *testing.T) {
	res, err := runLockPoisonOn(t, map[string]string{
		"fmt.rs": `
fn describe(state: &State) -> String {
    if let Ok(g) = state.entries.lock() {
        // A stray { in a comment, and a lifetime like &'a str.
        return format!("{ {}", g.len());
    }
    String::new()
}
`,
	})
	if err != nil {
		t.Fatalf("expected a warning, not a hard failure, got: %v", err)
	}
	if res.Code != ResultWarning {
		t.Fatalf("expected ResultWarning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "fmt.rs:3") {
		t.Errorf("expected the swallowing `if let Ok` at fmt.rs:3, got: %s", res.Message)
	}
}
