//! `search_only` leaf enumeration for the operation log (M2e).
//!
//! A same-FS move or a trash reverses a whole subtree with ONE rename-back /
//! restore, so the pipeline records ONE top-level `rollback_unit` row (see
//! `journal.rs`). But leaf search ("when did I trash `dog.jpg`") still needs the
//! descendants, so this module enumerates the subtree's leaves from the DRIVE
//! INDEX (zero extra filesystem I/O — the tree is already indexed) and records
//! them as `search_only` rows beneath the top-level unit (D-granularity).
//!
//! Two hard rules from the plan, both load-bearing for honesty:
//!
//! - **Enumerate BEFORE the OS mutation.** The index reconciler prunes the subtree
//!   the moment it sees the trash/rename FSEvent, so a finalize-time read would
//!   find the rows already gone and wrongly stamp `full` over a miss. Callers buffer
//!   the [`BufferedLeaves`] before the syscall.
//! - **Persist only AFTER the item succeeds.** Trash / same-FS move process per
//!   top-level item with partial failure, so persisting at enumeration time would
//!   leave `search_only` leaves for a subtree that was never trashed — and search
//!   has no per-item outcome filter, so "when did I trash `dog.jpg`" would return a
//!   trash that never happened. Callers call [`persist_and_note`] inside the
//!   item's success arm.
//!
//! `search_coverage = full` is gated on the subtree being PRESENT and CURRENT (the
//! index eventually-consistent, so a just-downloaded-then-trashed file can enumerate
//! a stale set) AND the volume index being `Live`; otherwise it downgrades to
//! `top_level_only` with a typed reason so the honest gap stays distinguishable.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::indexing::freshness::Freshness;
use crate::indexing::store::{IndexStore, resolve_path};
use crate::indexing::{get_freshness, get_read_pool_for, index_read_path, is_active};
use crate::operation_log::types::{EntryType, SearchCoverage, SearchCoverageReason};

/// The per-operation cap on `search_only` leaves enumerated for one top-level
/// trash / same-FS-move item. Because the enumeration is synchronous BEFORE a
/// sub-second rename, a 1M-file folder would otherwise pay a 1M-row index read
/// before the mutation — disproportionate. Over the cap, the op records the
/// top-level `rollback_unit` row only and downgrades coverage to `capped`;
/// rollback is unaffected (the top-level row is the undo unit regardless).
///
/// Initial value benchmark-tuned (see `docs/notes/operation-log-capture-bench.md`).
/// Cheap to change: it only bounds search completeness, never correctness.
pub(super) const SEARCH_LEAF_CAP: usize = 50_000;

/// One enumerated descendant leaf, path RELATIVE to the subtree root. The caller
/// rebases it onto the source and dest roots when persisting.
#[derive(Debug, Clone)]
pub(super) struct Leaf {
    pub rel: PathBuf,
    pub entry_type: EntryType,
    pub size: Option<i64>,
    pub mtime: Option<i64>,
}

/// The buffered result of enumerating a subtree BEFORE its mutation: the leaves to
/// persist on success, plus the coverage verdict (worst-case reason when
/// `top_level_only`). `Full` with an empty `leaves` means a leaf-less subtree that
/// IS fully covered (rare); a downgrade always carries an empty `leaves`.
#[derive(Debug)]
pub(super) struct BufferedLeaves {
    pub coverage: SearchCoverage,
    pub reason: Option<SearchCoverageReason>,
    pub leaves: Vec<Leaf>,
}

impl BufferedLeaves {
    /// A `top_level_only` downgrade with the given honest reason and no leaves.
    fn downgraded(reason: SearchCoverageReason) -> Self {
        Self {
            coverage: SearchCoverage::TopLevelOnly,
            reason: Some(reason),
            leaves: Vec::new(),
        }
    }

    /// Full coverage with the enumerated leaves.
    fn full(leaves: Vec<Leaf>) -> Self {
        Self {
            coverage: SearchCoverage::Full,
            reason: None,
            leaves,
        }
    }
}

/// Enumerate a subtree's `search_only` leaves from the drive index, applying the
/// Live gate (the volume's index must be active AND `Fresh`), path resolution, the
/// epoch/coverage gate, and the leaf cap. Called BEFORE the OS mutation; the caller
/// persists the result only after the item succeeds. `volume_id` is the index
/// volume (the local record points pass `"root"`); `abs_source` is the subtree
/// root's absolute path.
pub(super) fn enumerate_subtree_for_search(volume_id: &str, abs_source: &Path, cap: usize) -> BufferedLeaves {
    #[cfg(test)]
    if let Some(canned) = test_hook::take(abs_source) {
        return canned;
    }

    // Live gate: never trust index enumeration for a scanning / stale / unindexed
    // volume — a downgrade is the honest verdict.
    if !is_active(volume_id) || get_freshness(volume_id) != Some(Freshness::Fresh) {
        return BufferedLeaves::downgraded(SearchCoverageReason::VolumeNotLive);
    }
    let Some(pool) = get_read_pool_for(volume_id) else {
        return BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent);
    };
    let Some(index_path) = index_read_path(volume_id, &abs_source.to_string_lossy()) else {
        return BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent);
    };
    pool.with_conn(|conn| match resolve_path(conn, &index_path) {
        Ok(Some(root_id)) => enumerate_from_id(conn, root_id, cap),
        // The subtree isn't in the index (never scanned, or already pruned).
        _ => BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent),
    })
    .unwrap_or_else(|_| BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent))
}

/// The enumeration core: given a resolved subtree root id, apply the epoch gate,
/// then walk descendants with the cap. Split out (path resolution + the Live gate
/// stay in the wrapper) so the coverage honesty is unit-tested against a hand-built
/// index connection.
fn enumerate_from_id(conn: &Connection, root_id: i64, cap: usize) -> BufferedLeaves {
    // Coverage gate: the subtree must be fully covered (`min_subtree_epoch > 0`,
    // i.e. no descendant was never listed) AND current (`== current_epoch`, i.e.
    // no stale listing). Anything else downgrades to `index_stale` — the honest
    // "present but not trustworthy" verdict.
    let current = IndexStore::read_current_epoch(conn).unwrap_or(0);
    let min_subtree = IndexStore::get_dir_stats_by_id(conn, root_id)
        .ok()
        .flatten()
        .map(|s| s.min_subtree_epoch)
        .unwrap_or(0);
    if min_subtree == 0 || min_subtree < current {
        return BufferedLeaves::downgraded(SearchCoverageReason::IndexStale);
    }

    // Walk descendants depth-first, reading at most `cap + 1` rows TOTAL: each dir
    // is read with a `LIMIT` sized to the remaining budget + 1, so a 1M-child folder
    // pays a bounded read before the sub-second mutation (never the whole subtree).
    // Over the cap ⇒ `capped` (top-level only); a read error mid-walk ⇒ `index_stale`
    // (never claim full over a partial read).
    let mut leaves = Vec::new();
    let mut stack = vec![(root_id, PathBuf::new())];
    while let Some((dir_id, rel)) = stack.pop() {
        let remaining = cap - leaves.len();
        // Clamp to the i64 LIMIT range: an uncapped `cap` (usize::MAX) would
        // overflow the cast, so cap the read at i64::MAX (effectively unbounded).
        let limit = (remaining.min(i64::MAX as usize) as i64).saturating_add(1);
        let children = match IndexStore::list_children_on_limited(dir_id, conn, limit) {
            Ok(c) => c,
            Err(_) => return BufferedLeaves::downgraded(SearchCoverageReason::IndexStale),
        };
        // More children than the remaining budget ⇒ over cap. (`remaining + 1`
        // rows came back, so the subtree exceeds `cap`.)
        if children.len() > remaining {
            return BufferedLeaves::downgraded(SearchCoverageReason::Capped);
        }
        for child in children {
            let child_rel = rel.join(&child.name);
            let entry_type = if child.is_directory {
                EntryType::Dir
            } else {
                EntryType::File
            };
            leaves.push(Leaf {
                rel: child_rel.clone(),
                entry_type,
                size: child.logical_size.map(|s| s as i64),
                mtime: child.modified_at.map(|m| m as i64),
            });
            if child.is_directory {
                stack.push((child.id, child_rel));
            }
        }
    }
    BufferedLeaves::full(leaves)
}

/// Persist the buffered `search_only` leaves (ONLY call this after the top-level
/// item's mutation succeeded) and note any coverage downgrade on the op. Each leaf
/// is rebased onto the source root (`abs_source`) and, when known, the dest root
/// (`top_level_dest` — the in-trash location, or the moved-to path); a `None` dest
/// records the leaf source-only (still searchable). A `Full` verdict notes nothing
/// (the op opens `Full`); a downgrade notes worst-wins.
pub(super) fn persist_and_note(
    op_id: &str,
    abs_source: &Path,
    top_level_dest: Option<&Path>,
    buffered: &BufferedLeaves,
) {
    for leaf in &buffered.leaves {
        let source = abs_source.join(&leaf.rel);
        let dest = top_level_dest.map(|d| d.join(&leaf.rel));
        super::journal::record_local_search_leaf(op_id, leaf.entry_type, &source, dest.as_deref(), leaf.size, leaf.mtime);
    }
    if buffered.coverage != SearchCoverage::Full {
        crate::operation_log::journal_note_coverage(op_id, buffered.coverage, buffered.reason);
    }
}

/// Test seam: a per-path canned [`BufferedLeaves`], so the trash / same-FS-move
/// wiring can be exercised (persist-after-success, the failed-item honesty, the
/// capped-but-still-rollbackable case) without standing up a live drive index +
/// registry. Production never installs a hook.
#[cfg(test)]
pub(super) mod test_hook {
    use super::BufferedLeaves;
    use std::cell::RefCell;
    use std::path::Path;

    type Hook = Box<dyn FnMut(&Path) -> Option<BufferedLeaves>>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// Install a hook mapping a subtree-root path to a canned enumeration result
    /// (`None` ⇒ fall through to the real index path for that call).
    pub(crate) fn install(hook: impl FnMut(&Path) -> Option<BufferedLeaves> + 'static) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
    }

    pub(crate) fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }

    pub(super) fn take(path: &Path) -> Option<BufferedLeaves> {
        HOOK.with(|h| h.borrow_mut().as_mut().and_then(|hook| hook(path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{IndexStore, ROOT_ID};

    /// Build a temp index DB, insert a subtree under root, and stamp epochs. Returns
    /// the read connection for the enumeration core. `stale` makes the subtree's
    /// `min_subtree_epoch` lag the current epoch (the honest "present but not
    /// current" case).
    fn index_with_subtree(leaf_count: usize, stale: bool) -> (tempfile::TempDir, Connection, i64) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("index.sqlite");
        let _store = IndexStore::open(&db).expect("open index");
        let wconn = IndexStore::open_write_connection(&db).expect("write conn");

        // A subtree: root/photos/{f0..fN}. `photos` is a dir under ROOT_ID.
        let photos_id = IndexStore::insert_entry_v2(&wconn, ROOT_ID, "photos", true, false, None, None, None, None)
            .expect("insert photos");
        for i in 0..leaf_count {
            IndexStore::insert_entry_v2(
                &wconn,
                photos_id,
                &format!("f{i}.jpg"),
                false,
                false,
                Some(10),
                Some(10),
                Some(1_700_000_000),
                None,
            )
            .expect("insert leaf");
        }

        // Epochs: seed then bump so `current == 2`, letting the stale case set
        // `min_subtree_epoch = 1` (present but < current — the distinct stale path,
        // not the `== 0` never-listed one).
        let _ = IndexStore::seed_current_epoch(&wconn).expect("seed epoch");
        let current = IndexStore::bump_current_epoch(&wconn).expect("bump epoch");
        let sub_epoch = if stale { current - 1 } else { current };
        IndexStore::upsert_dir_stats_by_id(
            &wconn,
            &[crate::indexing::store::DirStatsById {
                entry_id: photos_id,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: leaf_count as u64,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: sub_epoch,
            }],
        )
        .expect("upsert dir stats");

        let rconn = IndexStore::open_read_connection(&db).expect("read conn");
        (dir, rconn, photos_id)
    }

    #[test]
    fn full_coverage_enumerates_every_leaf_from_the_index() {
        let (_dir, conn, photos_id) = index_with_subtree(3, false);
        let buffered = enumerate_from_id(&conn, photos_id, SEARCH_LEAF_CAP);
        assert_eq!(buffered.coverage, SearchCoverage::Full);
        assert_eq!(buffered.reason, None);
        assert_eq!(buffered.leaves.len(), 3, "all three leaves enumerated from the index");
        assert!(buffered.leaves.iter().all(|l| l.entry_type == EntryType::File));
    }

    #[test]
    fn a_stale_subtree_downgrades_to_top_level_only_never_full() {
        // Pre-fix (gating only on presence) this would have wrongly stamped `full`
        // over a stale leaf set — the silent-gap the coverage-honesty rule closes.
        let (_dir, conn, photos_id) = index_with_subtree(3, true);
        let buffered = enumerate_from_id(&conn, photos_id, SEARCH_LEAF_CAP);
        assert_eq!(
            buffered.coverage,
            SearchCoverage::TopLevelOnly,
            "a stale subtree must never claim full coverage"
        );
        assert_eq!(buffered.reason, Some(SearchCoverageReason::IndexStale));
        assert!(buffered.leaves.is_empty(), "a downgrade records no search leaves");
    }

    #[test]
    fn over_cap_downgrades_with_the_capped_reason() {
        // 5 leaves against a cap of 4 ⇒ over cap ⇒ top-level only, `capped`.
        let (_dir, conn, photos_id) = index_with_subtree(5, false);
        let buffered = enumerate_from_id(&conn, photos_id, 4);
        assert_eq!(buffered.coverage, SearchCoverage::TopLevelOnly);
        assert_eq!(
            buffered.reason,
            Some(SearchCoverageReason::Capped),
            "over-cap is `capped`, distinct from stale/absent"
        );
        assert!(buffered.leaves.is_empty());
    }

    #[test]
    fn exactly_cap_leaves_still_counts_as_full() {
        // The cap is exclusive-over: exactly `cap` leaves is full coverage.
        let (_dir, conn, photos_id) = index_with_subtree(4, false);
        let buffered = enumerate_from_id(&conn, photos_id, 4);
        assert_eq!(buffered.coverage, SearchCoverage::Full);
        assert_eq!(buffered.leaves.len(), 4);
    }

    /// Build an index whose `photos` subtree holds `leaf_count` flat leaves in ONE
    /// transaction (fast enough for the 1M bench), current epoch stamped.
    fn index_with_n_leaves(leaf_count: usize) -> (tempfile::TempDir, Connection, i64) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("index.sqlite");
        let _store = IndexStore::open(&db).expect("open index");
        let mut wconn = IndexStore::open_write_connection(&db).expect("write conn");
        let photos_id = IndexStore::insert_entry_v2(&wconn, ROOT_ID, "photos", true, false, None, None, None, None)
            .expect("insert photos");
        {
            let tx = wconn.transaction().expect("tx");
            for i in 0..leaf_count {
                IndexStore::insert_entry_v2(
                    &tx,
                    photos_id,
                    &format!("f{i}.jpg"),
                    false,
                    false,
                    Some(10),
                    Some(10),
                    Some(1_700_000_000),
                    None,
                )
                .expect("insert leaf");
            }
            tx.commit().expect("commit");
        }
        let current = IndexStore::seed_current_epoch(&wconn).expect("seed");
        IndexStore::upsert_dir_stats_by_id(
            &wconn,
            &[crate::indexing::store::DirStatsById {
                entry_id: photos_id,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: leaf_count as u64,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: current,
            }],
        )
        .expect("stats");
        let rconn = IndexStore::open_read_connection(&db).expect("read conn");
        (dir, rconn, photos_id)
    }

    /// BENCHMARK (ignored — run with `--run-ignored` to collect numbers): the
    /// synchronous enumeration latency vs subtree size (1k / 10k / 100k / 1M).
    /// The cap read is `LIMIT cap + 1`, so the synchronous cost the mutation waits
    /// on is bounded regardless of the true subtree size. Results feed the cap
    /// tuning in `docs/notes/operation-log-capture-bench.md`.
    #[test]
    #[ignore = "benchmark; run explicitly to collect numbers"]
    #[allow(clippy::print_stdout, reason = "a benchmark prints its measurements")]
    fn bench_enumeration_latency_by_subtree_size() {
        // Sizes kept ≤ 100k so one run fits the harness slow-timeout; the uncapped
        // curve is linear (~1.2 µs/leaf) so 1M extrapolates to ~1.2 s — exactly the
        // disproportionate pre-mutation cost the cap prevents. Capped(50k) reads at
        // most cap+1 rows (the `LIMIT` bound), so it plateaus once n > cap and is the
        // ceiling for ALL larger subtrees, 1M included.
        for &n in &[1_000usize, 10_000, 100_000] {
            let (_dir, conn, root) = index_with_n_leaves(n);
            let t0 = std::time::Instant::now();
            let full = enumerate_from_id(&conn, root, usize::MAX);
            let uncapped = t0.elapsed();
            assert_eq!(full.leaves.len(), n);
            let t1 = std::time::Instant::now();
            let capped = enumerate_from_id(&conn, root, SEARCH_LEAF_CAP);
            let capped_t = t1.elapsed();
            println!(
                "enumerate n={n:>7}: uncapped={uncapped:?} capped(50k)={capped_t:?} (capped coverage={:?})",
                capped.coverage
            );
        }
    }
}
