//! Tracks which TCC-protected paths Cmdr currently can't access.
//!
//! See `tcc_paths.rs` for the path predicate. This module owns the runtime
//! state (a `RwLock<HashSet<PathBuf>>`), exposes a small API for callers
//! that observe `PermissionDenied`, and runs an `NSApplicationDidBecomeActive`
//! observer that re-probes the set whenever the user returns to Cmdr.
//! That's how the UI feels "live" after the user grants permission in
//! System Settings, without polling.
//!
//! Public flow:
//!
//! 1. Capture sites (indexer scanner, listing IPC) call `record_denial(path)` when they hit
//!    `PermissionDenied` on a path that passes `tcc_paths::is_potentially_tcc_restricted`. The path
//!    enters the set.
//! 2. Successful listings call `clear_path(path)` to drop entries that have become accessible.
//! 3. On `NSApplicationDidBecomeActive`, the observer fires `reprobe_all_async()` which spawns a
//!    blocking task that runs `read_dir` against each known restricted path. Paths that now succeed
//!    are cleared. Paths still denied stay in the set.
//! 4. Any change emits a debounced `restricted-paths-changed` event carrying the full sorted set.
//!    The frontend store hydrates initially via `get_restricted_paths()` and patches via the event
//!    afterwards.
//!
//! On non-macOS, the predicate always returns `false` so the set never
//! gains entries and the observer is a no-op (or compiled out).

pub mod tcc_paths;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

const EVENT_NAME: &str = "restricted-paths-changed";
const DEBOUNCE_MS: u64 = 150;

static STATE: OnceLock<RwLock<HashSet<PathBuf>>> = OnceLock::new();
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static EMIT_GENERATION: AtomicU64 = AtomicU64::new(0);

fn state() -> &'static RwLock<HashSet<PathBuf>> {
    STATE.get_or_init(|| RwLock::new(HashSet::new()))
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestrictedPathsChangedPayload {
    /// Absolute path strings, sorted alphabetically for a stable diff on
    /// the frontend.
    pub paths: Vec<String>,
}

/// Stash the AppHandle so capture-site callers don't need to thread one
/// through. Also installs the `NSApplicationDidBecomeActive` observer
/// (macOS only).
pub fn init(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());

    #[cfg(target_os = "macos")]
    install_did_become_active_observer();
}

/// Add `path` to the restricted set. No-op if the path is already in the
/// set or doesn't pass the TCC-restricted predicate. Schedules a debounced
/// emit on a state change.
pub fn record_denial(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if !tcc_paths::is_potentially_tcc_restricted(path) && !tcc_paths::is_network_volume_path(path) {
        return;
    }
    let buf = path.to_path_buf();
    {
        let mut guard = state().write().expect("restricted_paths state poisoned");
        if !guard.insert(buf) {
            return; // Already in the set; no change.
        }
    }
    log::debug!(target: "restricted_paths", "recorded denial: {}", path.display());
    schedule_emit();
}

/// Remove `path` from the restricted set, if present. Schedules a
/// debounced emit on a state change.
pub fn clear_path(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let removed = {
        let mut guard = state().write().expect("restricted_paths state poisoned");
        guard.remove(path)
    };
    if removed {
        log::debug!(target: "restricted_paths", "cleared: {}", path.display());
        schedule_emit();
    }
}

/// Return a stable snapshot of the current restricted set, sorted.
pub fn snapshot() -> Vec<String> {
    let guard = state().read().expect("restricted_paths state poisoned");
    let mut paths: Vec<String> = guard.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    paths.sort();
    paths
}

/// Re-probe every path in the restricted set with a cheap `read_dir`. Any
/// path that now opens successfully is cleared. Runs on a blocking task,
/// so it's safe to call from the main thread (the observer block does so).
///
/// macOS-only because the only caller (the `NSApplicationDidBecomeActive`
/// observer in `install_did_become_active_observer` below) is itself macOS
/// only. Other platforms have no equivalent re-probe trigger today, so the
/// function would just be dead code there (and `#![deny(unused)]` would fail
/// the Linux build).
#[cfg(target_os = "macos")]
pub fn reprobe_all_async() {
    let paths_to_probe = {
        let guard = state().read().expect("restricted_paths state poisoned");
        guard.iter().cloned().collect::<Vec<_>>()
    };
    if paths_to_probe.is_empty() {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut to_clear = Vec::new();
        for path in &paths_to_probe {
            match std::fs::read_dir(path) {
                Ok(_) => to_clear.push(path.clone()),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => { /* still denied */ }
                Err(_) => {
                    // NotFound, broken symlink, etc.: clear so the set
                    // doesn't grow forever with stale entries.
                    to_clear.push(path.clone());
                }
            }
        }
        for path in to_clear {
            clear_path(path);
        }
    });
}

fn schedule_emit() {
    let generation = EMIT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = match APP_HANDLE.get() {
        Some(a) => a.clone(),
        None => return, // init() hasn't run yet; first emit will go via bootstrap query.
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
        if EMIT_GENERATION.load(Ordering::SeqCst) != generation {
            return; // Superseded by a later schedule.
        }
        let payload = RestrictedPathsChangedPayload { paths: snapshot() };
        let _ = app.emit(EVENT_NAME, payload);
    });
}

#[cfg(target_os = "macos")]
fn install_did_become_active_observer() {
    use block2::RcBlock;
    use objc2_app_kit::{NSApplicationDidBecomeActiveNotification, NSWorkspace};
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;

    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }
    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    let block = RcBlock::new(move |_n: NonNull<NSNotification>| {
        reprobe_all_async();
    });
    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            // NSApplication notification (posted on the app itself, not NSWorkspace).
            // NotificationCenter still routes by name, so this works via
            // the NSWorkspace center too, but for symmetry with the rest
            // of our observers we use the workspace center. The
            // app-became-active name is rebroadcast through the workspace
            // center on macOS 12+.
            Some(NSApplicationDidBecomeActiveNotification),
            None,
            None,
            &block,
        );
    }
    log::debug!(target: "restricted_paths", "installed NSApplicationDidBecomeActive observer");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        let mut guard = state().write().unwrap();
        guard.clear();
    }

    /// Bypass the TCC predicate filter for in-process tests by inserting
    /// directly. The predicate is unit-tested separately in `tcc_paths`.
    fn force_insert(path: &str) {
        let mut guard = state().write().unwrap();
        guard.insert(PathBuf::from(path));
    }

    #[test]
    fn snapshot_is_sorted_and_dedup() {
        reset();
        force_insert("/b");
        force_insert("/a");
        force_insert("/a"); // dup
        force_insert("/c");
        let s = snapshot();
        assert_eq!(s, vec!["/a", "/b", "/c"]);
    }

    #[test]
    fn clear_removes_only_named() {
        reset();
        force_insert("/x");
        force_insert("/y");
        clear_path("/x");
        let s = snapshot();
        assert_eq!(s, vec!["/y"]);
    }

    #[test]
    fn clear_missing_is_noop() {
        reset();
        force_insert("/x");
        clear_path("/never-there");
        let s = snapshot();
        assert_eq!(s, vec!["/x"]);
    }

    #[test]
    fn record_denial_filters_by_predicate() {
        // Non-matching path: ignored.
        reset();
        record_denial("/etc/passwd");
        let s = snapshot();
        assert!(s.is_empty(), "expected non-matching path to be ignored, got {s:?}");
    }
}
