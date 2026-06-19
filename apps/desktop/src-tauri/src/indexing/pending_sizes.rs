//! Tracks directories with unprocessed index writes in flight.
//!
//! When the user deletes or adds many files at once, the writer needs seconds
//! to minutes to propagate the recursive `dir_stats` deltas up every ancestor
//! chain. During that window the sizes shown in the file list are stale. This
//! tracker lets the UI mark those directories with a "size updating" hourglass
//! so the numbers aren't presented as settled truth while they're still moving.
//!
//! ## How it stays correct
//!
//! - **Mark** (live event loop): every dir whose recursive size is about to
//!   change is inserted, along with all its ancestors. We're handed exactly
//!   that set already — it's the `pending_paths` the loop drains into
//!   `index-dir-updated` — so marking rides the same data that drives the UI
//!   refresh ("flag exactly what we refresh").
//! - **Clear** (writer thread): cleared wholesale when the writer's queue
//!   drains to empty. An empty queue means there is no unprocessed work, so the
//!   set is correct to empty. This is self-healing: even if marking ever missed
//!   or over-marked, every time the writer catches up the state resets to truth.
//!   There's no per-entry increment/decrement to leak, so the "stuck hourglass
//!   forever" failure class doesn't exist.
//! - **Read** (`DirStats` build in `state.rs`): a single membership test per
//!   directory, carried to the frontend on `DirStats.recursive_size_pending`.
//!
//! The tradeoff is coarse granularity: during a storm every touched ancestor
//! stays flagged until the writer fully drains, then they clear together. For
//! the target scenario (mass delete, "is it settled yet?") that's the right
//! granularity — it answers exactly that question.

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex};

use super::firmlinks;
use super::state::ROOT_VOLUME_ID;
use crate::ignore_poison::IgnorePoison;

/// In-memory set of directory paths with unprocessed index writes in flight.
///
/// Paths are stored normalized (via [`firmlinks::normalize_path`]) so a query
/// matches regardless of how the caller navigated to the path (firmlink alias,
/// `/tmp` vs `/private/tmp`, etc.).
pub(crate) struct PendingSizes {
    paths: Mutex<HashSet<String>>,
}

impl PendingSizes {
    pub(crate) fn new() -> Self {
        Self {
            paths: Mutex::new(HashSet::new()),
        }
    }

    /// Normalize `path` and insert it AND every ancestor directory.
    ///
    /// Centralizing the ancestor expansion here means callers can pass whatever
    /// affected dirs they have — a full ancestor chain (normal events) or a
    /// single parent (rename pre-pass) — and the membership test is correct for
    /// any ancestor row shown in the UI.
    pub(crate) fn mark(&self, path: &str) {
        let normalized = firmlinks::normalize_path(path);
        let mut guard = match self.paths.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut cur = normalized.as_str();
        loop {
            guard.insert(cur.to_string());
            match cur.rfind('/') {
                // Parent is the root "/": insert it and stop.
                Some(0) => {
                    guard.insert("/".to_string());
                    break;
                }
                Some(pos) => cur = &cur[..pos],
                // No slash at all (not an absolute path). Nothing more to walk.
                None => break,
            }
        }
    }

    /// Whether `path` (normalized) has unprocessed index writes in flight.
    pub(crate) fn is_pending(&self, path: &str) -> bool {
        let normalized = firmlinks::normalize_path(path);
        let guard = match self.paths.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.contains(&normalized)
    }

    /// Drop all pending paths. Called when the writer queue drains to empty.
    pub(crate) fn clear(&self) {
        let mut guard = match self.paths.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.clear();
    }

    /// Number of tracked paths. Test-only observability.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.paths.lock().map(|g| g.len()).unwrap_or(0)
    }
}

/// The root volume's pending-size tracker, installed/cleared in lockstep with
/// `READ_POOL` (see `enrichment.rs` and the lifecycle sites in `state.rs`).
/// `None` whenever root indexing isn't running, so reads return "not pending".
/// The root `IndexInstance` shares this same `Arc`. Non-root volumes' trackers
/// live in their `IndexInstance` (see `super::state::get_instance_pending_sizes`).
pub(super) static PENDING_SIZES: LazyLock<Mutex<Option<Arc<PendingSizes>>>> = LazyLock::new(|| Mutex::new(None));

/// Tests that touch `PENDING_SIZES` must hold this lock to avoid races with
/// parallel test threads (mirrors `READ_POOL_TEST_MUTEX`).
#[cfg(test)]
pub(super) static PENDING_SIZES_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Clone the root tracker `Arc`, if installed. Lock held for nanoseconds.
pub(crate) fn get_pending_sizes() -> Option<Arc<PendingSizes>> {
    PENDING_SIZES.lock().ok()?.as_ref().cloned()
}

/// Clone a specific volume's tracker. Routes root to `PENDING_SIZES`, every
/// other volume to its `IndexInstance` in the registry.
pub(crate) fn get_pending_sizes_for(volume_id: &str) -> Option<Arc<PendingSizes>> {
    if volume_id == ROOT_VOLUME_ID {
        get_pending_sizes()
    } else {
        super::state::get_instance_pending_sizes(volume_id)
    }
}

/// Install the root volume's tracker into the global fast handle. No-op for
/// non-root volumes: their tracker is owned by the `IndexInstance` directly.
pub(super) fn install_pending_sizes(volume_id: &str, tracker: Arc<PendingSizes>) {
    if volume_id == ROOT_VOLUME_ID {
        *PENDING_SIZES.lock_ignore_poison() = Some(tracker);
    }
}

/// Clear the root volume's global tracker (on stop/clear). Non-root volumes'
/// trackers drop with their `IndexInstance`.
pub(super) fn uninstall_pending_sizes(volume_id: &str) {
    if volume_id == ROOT_VOLUME_ID {
        *PENDING_SIZES.lock_ignore_poison() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use synthetic paths under `/aaa` that `firmlinks::normalize_path` leaves
    // unchanged on both macOS and Linux (not `/tmp|/var|/etc`, not under
    // `/System/Volumes/Data`), so assertions are deterministic cross-platform.

    #[test]
    fn mark_then_query_is_pending() {
        let t = PendingSizes::new();
        assert!(!t.is_pending("/aaa/bbb"));
        t.mark("/aaa/bbb");
        assert!(t.is_pending("/aaa/bbb"));
        assert!(!t.is_pending("/aaa/ccc"));
    }

    #[test]
    fn mark_flags_every_ancestor() {
        let t = PendingSizes::new();
        t.mark("/aaa/bbb/ccc/ddd");
        // The path itself and each ancestor dir are pending.
        assert!(t.is_pending("/aaa/bbb/ccc/ddd"));
        assert!(t.is_pending("/aaa/bbb/ccc"));
        assert!(t.is_pending("/aaa/bbb"));
        assert!(t.is_pending("/aaa"));
        assert!(t.is_pending("/"));
        // An unrelated sibling is not.
        assert!(!t.is_pending("/aaa/bbb/ccc/eee"));
    }

    #[test]
    fn clear_empties_everything() {
        let t = PendingSizes::new();
        t.mark("/aaa/bbb/ccc");
        t.mark("/zzz");
        assert!(t.len() > 0);
        t.clear();
        assert_eq!(t.len(), 0);
        assert!(!t.is_pending("/aaa/bbb/ccc"));
        assert!(!t.is_pending("/aaa"));
    }

    #[test]
    fn marking_is_idempotent_across_overlapping_chains() {
        let t = PendingSizes::new();
        t.mark("/aaa/bbb/ccc");
        let after_first = t.len();
        // Marking a sibling under the same ancestors adds only the new leaf nodes;
        // shared ancestors dedup in the set.
        t.mark("/aaa/bbb/ddd");
        assert!(t.is_pending("/aaa/bbb/ccc"));
        assert!(t.is_pending("/aaa/bbb/ddd"));
        // /aaa/bbb/ddd + (shared /aaa/bbb, /aaa, / already present) => +1 only.
        assert_eq!(t.len(), after_first + 1);
    }

    #[test]
    fn normalization_is_symmetric_for_marked_paths() {
        // `mark` and `is_pending` both normalize, so any path normalizes to the
        // same key on read as it did on write. On macOS `/tmp` → `/private/tmp`;
        // on Linux it's unchanged. Either way the round-trip matches.
        let t = PendingSizes::new();
        t.mark("/tmp/aaa/bbb");
        assert!(t.is_pending("/tmp/aaa/bbb"));
        assert!(t.is_pending("/tmp/aaa"));
    }

    #[test]
    fn global_get_returns_none_when_uninstalled() {
        let _guard = PENDING_SIZES_TEST_MUTEX.lock().unwrap();
        *PENDING_SIZES.lock().unwrap() = None;
        assert!(get_pending_sizes().is_none());
    }

    #[test]
    fn global_install_and_clear_roundtrip() {
        let _guard = PENDING_SIZES_TEST_MUTEX.lock().unwrap();
        *PENDING_SIZES.lock().unwrap() = Some(Arc::new(PendingSizes::new()));
        let tracker = get_pending_sizes().expect("installed");
        tracker.mark("/aaa/bbb");
        assert!(tracker.is_pending("/aaa/bbb"));
        *PENDING_SIZES.lock().unwrap() = None;
        assert!(get_pending_sizes().is_none());
    }
}
