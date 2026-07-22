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
//! Two tiers, because two kinds of pending work have different lifetimes.
//!
//! **Transient set** (the `paths` set) — fast per-event marks:
//!
//! - **Mark** (live event loop): every dir whose recursive size is about to
//!   change is inserted, along with all its ancestors. We're handed exactly
//!   that set already — it's the `pending_paths` the loop drains into
//!   `index-dir-updated` — so marking rides the same data that drives the UI
//!   refresh ("flag exactly what we refresh").
//! - **Clear** (writer thread): the transient set is cleared wholesale when the
//!   writer's queue drains to empty. An empty queue means there is no
//!   unprocessed writer work, so the set is correct to empty. This is
//!   self-healing: even if marking ever missed or over-marked, every time the
//!   writer catches up the state resets to truth. There's no per-entry
//!   increment/decrement to leak, so the "stuck hourglass forever" failure class
//!   doesn't exist.
//!
//! **Held-roots set** (the `held_roots` set) — coalesced rescan scopes:
//!
//! - A detached `MustScanSubDirs` reconcile runs for seconds to minutes while
//!   the writer queue oscillates empty, so the transient set's drain-clear would
//!   wipe its hourglass long before it finishes. `hold(root)` / `release(root)`
//!   track a small set of rescan ROOT paths (no ancestor expansion) that survive
//!   writer drains; the reconciler holds at queue time and releases at every
//!   exit. `is_pending(path)` treats a held root's whole chain as pending in
//!   BOTH directions (below).
//! - Why roots + a query-time prefix test instead of expanding ancestors into
//!   the set: overlapping rescans (`/a/b` and `/a/c`) share ancestor rows, so
//!   expanding would either strip `/a` while one is in flight or leak it forever.
//!   Holding only roots keeps release exact and needs no refcounting.
//!
//! **Read** (`DirStats` build in `lifecycle/state.rs`): a single `is_pending` test per
//! directory, carried to the frontend on `DirStats.recursive_size_pending`.
//!
//! **Per-volume routing (both tiers).** Marks, holds, releases, and the
//! writer-drain clear all target the OWNING volume's tracker via
//! `get_pending_sizes_for(volume_id)` (root → the `PENDING_SIZES` global,
//! non-root → the registry instance). A root-only `get_pending_sizes()` from a
//! non-root writer would wipe root's hourglass on a non-root drain AND never
//! clear its own — so the volume id is threaded through `queue_must_scan_sub_dirs`
//! and the writer loop rather than defaulting to root.
//!
//! The tradeoff is coarse granularity: during a storm every touched ancestor
//! stays flagged until the writer fully drains (transient) or the rescan
//! completes (held), then they clear. For the target scenario (mass delete, "is
//! it settled yet?") that's the right granularity — it answers exactly that
//! question. The hourglass's role in the wider size-integrity story, and the
//! release-before-emit completion sequence, are in `indexing/DETAILS.md`
//! § "The dir_stats ledger".

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
use crate::indexing::firmlinks;
use crate::indexing::paths::path_prefix;
use crate::indexing::state::ROOT_VOLUME_ID;

/// In-memory set of directory paths with unprocessed index writes in flight.
///
/// Paths are stored normalized (via [`firmlinks::normalize_path`]) so a query
/// matches regardless of how the caller navigated to the path (firmlink alias,
/// `/tmp` vs `/private/tmp`, etc.).
pub(crate) struct PendingSizes {
    /// Per-event marks, cleared wholesale when the writer queue drains.
    paths: Mutex<HashSet<String>>,
    /// Rescan ROOT paths held for the lifetime of a detached `MustScanSubDirs`
    /// reconcile. Never ancestor-expanded; the drain-clear leaves them alone.
    held_roots: Mutex<HashSet<String>>,
}

impl PendingSizes {
    pub(crate) fn new() -> Self {
        Self {
            paths: Mutex::new(HashSet::new()),
            held_roots: Mutex::new(HashSet::new()),
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

    /// Hold `root` (a rescan root path) for the lifetime of its detached
    /// reconcile. Normalized so `is_pending` matches regardless of firmlink
    /// aliasing. A set insert, so re-holding an already-held root is a no-op.
    pub(crate) fn hold(&self, root: &str) {
        let normalized = firmlinks::normalize_path(root);
        self.held_roots.lock_ignore_poison().insert(normalized);
    }

    /// Release a previously-held rescan root. A set remove, so releasing an
    /// unheld (or already-released) root is a harmless no-op.
    pub(crate) fn release(&self, root: &str) {
        let normalized = firmlinks::normalize_path(root);
        self.held_roots.lock_ignore_poison().remove(&normalized);
    }

    /// Whether `path` (normalized) has unprocessed index writes in flight —
    /// either it's in the transient set, or it's related to a held rescan root
    /// in EITHER direction: an ancestor-or-equal of the root (its aggregate
    /// includes the subtree being rewritten) or a descendant of it (its own rows
    /// are being rewritten). The held set is bounded by `pending_rescans` (a
    /// handful), so the linear scan is trivial.
    pub(crate) fn is_pending(&self, path: &str) -> bool {
        let normalized = firmlinks::normalize_path(path);
        if self.paths.lock_ignore_poison().contains(&normalized) {
            return true;
        }
        self.held_roots.lock_ignore_poison().iter().any(|root| {
            normalized == *root
                || path_prefix::is_strict_descendant(&normalized, root)
                || path_prefix::is_strict_descendant(root, &normalized)
        })
    }

    /// Drop the transient marks. Called when the writer queue drains to empty.
    /// Leaves `held_roots` alone: a rescan's hourglass must outlive the writer
    /// oscillating empty mid-walk (that's the whole point of the held tier).
    pub(crate) fn clear(&self) {
        self.paths.lock_ignore_poison().clear();
    }

    /// Number of transient marks. Test-only observability.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.paths.lock_ignore_poison().len()
    }

    /// Number of held rescan roots. Test-only observability.
    #[cfg(test)]
    pub(crate) fn held_len(&self) -> usize {
        self.held_roots.lock_ignore_poison().len()
    }
}

/// The root volume's pending-size tracker, installed/cleared in lockstep with
/// `READ_POOL` (see `read/enrichment.rs` and the lifecycle sites in `lifecycle/state.rs`).
/// `None` whenever root indexing isn't running, so reads return "not pending".
/// The root `IndexInstance` shares this same `Arc`. Non-root volumes' trackers
/// live in their `IndexInstance` (see `crate::indexing::state::get_instance_pending_sizes`).
pub(crate) static PENDING_SIZES: LazyLock<Mutex<Option<Arc<PendingSizes>>>> = LazyLock::new(|| Mutex::new(None));

/// Tests that touch `PENDING_SIZES` must hold this lock to avoid races with
/// parallel test threads (mirrors `READ_POOL_TEST_MUTEX`).
#[cfg(test)]
pub(crate) static PENDING_SIZES_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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
        crate::indexing::state::get_instance_pending_sizes(volume_id)
    }
}

/// Install the root volume's tracker into the global fast handle. No-op for
/// non-root volumes: their tracker is owned by the `IndexInstance` directly.
pub(crate) fn install_pending_sizes(volume_id: &str, tracker: Arc<PendingSizes>) {
    if volume_id == ROOT_VOLUME_ID {
        *PENDING_SIZES.lock_ignore_poison() = Some(tracker);
    }
}

/// Clear the root volume's global tracker (on stop/clear). Non-root volumes'
/// trackers drop with their `IndexInstance`.
pub(crate) fn uninstall_pending_sizes(volume_id: &str) {
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
    fn held_root_is_pending_in_both_directions() {
        let t = PendingSizes::new();
        t.hold("/aaa/bbb/ccc");
        // The held root itself.
        assert!(t.is_pending("/aaa/bbb/ccc"));
        // Ancestors of the held root: their aggregate includes the rewritten subtree.
        assert!(t.is_pending("/aaa/bbb"));
        assert!(t.is_pending("/aaa"));
        assert!(t.is_pending("/"));
        // Descendants of the held root: their own rows are being rewritten.
        assert!(t.is_pending("/aaa/bbb/ccc/ddd"));
        assert!(t.is_pending("/aaa/bbb/ccc/ddd/eee"));
        // A component-sibling that only shares a byte prefix is NOT pending.
        assert!(!t.is_pending("/aaa/bbb/cccX"));
        // An unrelated sibling subtree is not.
        assert!(!t.is_pending("/aaa/bbb/zzz"));
    }

    #[test]
    fn writer_drain_clear_keeps_holds() {
        let t = PendingSizes::new();
        t.mark("/aaa/bbb/ccc");
        t.hold("/aaa/rescan");
        // A writer-drain clear wipes only the transient marks.
        t.clear();
        assert_eq!(t.len(), 0, "transient marks cleared");
        assert!(!t.is_pending("/aaa/bbb/ccc"), "transient mark gone");
        // The held root and its chain survive the drain.
        assert!(t.is_pending("/aaa/rescan"), "held root survives the drain");
        assert!(t.is_pending("/aaa"), "held root ancestor survives the drain");
        assert_eq!(t.held_len(), 1);
    }

    #[test]
    fn release_drops_the_hold() {
        let t = PendingSizes::new();
        t.hold("/aaa/rescan");
        assert!(t.is_pending("/aaa/rescan"));
        t.release("/aaa/rescan");
        assert!(!t.is_pending("/aaa/rescan"));
        assert!(!t.is_pending("/aaa"));
        assert_eq!(t.held_len(), 0);
        // Releasing an unheld root is a harmless no-op.
        t.release("/aaa/never-held");
        assert_eq!(t.held_len(), 0);
    }

    #[test]
    fn overlapping_rescans_release_independently() {
        // Two sibling rescans under a shared ancestor `/aaa`. Releasing one must
        // NOT strip `/aaa`'s pendingness while the other is in flight — the exact
        // failure that expanding ancestors into the held set would cause.
        let t = PendingSizes::new();
        t.hold("/aaa/bbb");
        t.hold("/aaa/ccc");
        assert!(t.is_pending("/aaa"), "shared ancestor pending while both held");
        // Finish /aaa/bbb.
        t.release("/aaa/bbb");
        assert!(!t.is_pending("/aaa/bbb"), "the finished rescan's own chain clears");
        assert!(t.is_pending("/aaa/ccc"), "the in-flight rescan stays pending");
        assert!(
            t.is_pending("/aaa"),
            "shared ancestor still pending via the in-flight rescan"
        );
        // Finish /aaa/ccc: now `/aaa` clears too.
        t.release("/aaa/ccc");
        assert!(!t.is_pending("/aaa"));
        assert_eq!(t.held_len(), 0);
    }

    #[test]
    fn hold_is_idempotent() {
        // Re-holding an already-held root (e.g. a storm re-queue of the active
        // path) is a no-op; one release clears it.
        let t = PendingSizes::new();
        t.hold("/aaa/rescan");
        t.hold("/aaa/rescan");
        assert_eq!(t.held_len(), 1);
        t.release("/aaa/rescan");
        assert!(!t.is_pending("/aaa/rescan"));
        assert_eq!(t.held_len(), 0);
    }

    #[test]
    fn get_pending_sizes_for_routes_per_volume() {
        // Pins the cross-volume routing the writer-drain clear and hold/release
        // rely on: a non-root volume id must NOT resolve to the root tracker.
        let _guard = PENDING_SIZES_TEST_MUTEX.lock().unwrap();
        let root_tracker = Arc::new(PendingSizes::new());
        *PENDING_SIZES.lock().unwrap() = Some(Arc::clone(&root_tracker));
        // Root id routes to the installed root tracker.
        let via_root = get_pending_sizes_for(ROOT_VOLUME_ID).expect("root tracker installed");
        assert!(Arc::ptr_eq(&via_root, &root_tracker));
        // A non-root id with no registered instance resolves to None, never to root.
        assert!(
            get_pending_sizes_for("smb://no-such-volume").is_none(),
            "a non-root id must not fall through to the root tracker"
        );
        *PENDING_SIZES.lock().unwrap() = None;
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
