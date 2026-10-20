//! The write engine's drive-index questions, answered by the app's index
//! (`index_host::index()`, reached per call).
//!
//! The `search_leaves` read is the honest-coverage half of the engine's
//! `journal_search.rs`: the engine decides WHEN to enumerate (before the
//! mutation) and what to persist (only after the item succeeds); this decides
//! what the index can vouch for.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::index_host::index;
use cmdr_index::store::{DirStats, IndexStore, resolve_path};
use cmdr_operation_log::types::{EntryType, SearchCoverageReason};
use cmdr_operations::KnownSize;
use cmdr_operations::host::{BufferedLeaves, ExpectedTotals, Leaf, OperationIndex};

/// The app's drive index, asked per call: it's built in `setup()` and its
/// volumes come and go, so nothing is captured at construction.
pub(super) struct AppOperationIndex;

impl OperationIndex for AppOperationIndex {
    fn expected_totals(&self, sources: &[PathBuf]) -> Option<ExpectedTotals> {
        index().expected_totals(sources).map(|totals| ExpectedTotals {
            files: totals.files,
            bytes: totals.bytes,
        })
    }

    fn recursive_size(&self, path: &Path) -> Option<KnownSize> {
        index()
            .dir_stats(&path.to_string_lossy())
            .ok()
            .flatten()
            .and_then(|stats| prompt_size(&stats))
    }

    /// Applies the Live gate (the volume's index must be active AND `Fresh`),
    /// path resolution, the epoch/coverage gate, and the leaf cap.
    fn search_leaves(&self, volume_id: &str, abs_source: &Path, cap: usize) -> BufferedLeaves {
        // Live gate: never trust index enumeration for a scanning / stale / unindexed
        // volume — a downgrade is the honest verdict.
        if !index().is_fresh(volume_id) {
            return BufferedLeaves::downgraded(SearchCoverageReason::VolumeNotLive);
        }
        let Some(pool) = index().read_pool(volume_id) else {
            return BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent);
        };
        let Some(index_path) = index().read_path(volume_id, &abs_source.to_string_lossy()) else {
            return BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent);
        };
        pool.with_conn(|conn| match resolve_path(conn, &index_path) {
            Ok(Some(root_id)) => enumerate_from_id(conn, root_id, cap),
            // The subtree isn't in the index (never scanned, or already pruned).
            _ => BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent),
        })
        .unwrap_or_else(|_| BufferedLeaves::downgraded(SearchCoverageReason::IndexAbsent))
    }
}

/// What a folder's index stats can vouch for, read the way the listing reads
/// them (`getDirSizeDisplayState`), because the conflict prompt shows it to a
/// person deciding whether to replace that folder:
/// - a write in flight under it (`pending`): the number is moving, maybe down,
///   so nothing (the listing's hourglass, which drops its `≥` too);
/// - a partial count (`!complete`): a floor, or nothing when nothing is
///   counted yet (the listing's `<dir>`);
/// - a complete count from an older epoch (`stale`): the tree may have changed
///   either way since, so nothing;
/// - otherwise the exact total.
fn prompt_size(stats: &DirStats) -> Option<KnownSize> {
    let bytes = stats.recursive_size;
    if stats.recursive_size_pending {
        None
    } else if !stats.recursive_size_complete {
        (bytes > 0).then_some(KnownSize::AtLeast { bytes })
    } else if stats.recursive_size_stale {
        None
    } else {
        Some(KnownSize::Exact { bytes })
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_index::store::{IndexStore, ROOT_ID};
    use cmdr_operation_log::types::SearchCoverage;
    use cmdr_operations::testing::SEARCH_LEAF_CAP;

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
            &[cmdr_index::store::DirStatsById {
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

    fn stats(size: u64, complete: bool, stale: bool, pending: bool) -> DirStats {
        DirStats {
            path: "/photos".to_string(),
            recursive_size: size,
            recursive_physical_size: size,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            recursive_size_pending: pending,
            recursive_size_pending_changes_in: None,
            recursive_size_complete: complete,
            recursive_size_stale: stale,
        }
    }

    /// A folder mid-first-index holds 40 GB but the index has only counted 12 KB
    /// of it. Handing the conflict prompt that 12 KB as exact makes an Overwrite
    /// that deletes the whole tree look like replacing a tiny folder (#528).
    #[test]
    fn an_incomplete_folder_size_is_never_reported_as_exact() {
        assert_eq!(
            prompt_size(&stats(12_288, false, false, false)),
            Some(KnownSize::AtLeast { bytes: 12_288 })
        );
    }

    #[test]
    fn a_complete_settled_folder_size_is_exact() {
        assert_eq!(
            prompt_size(&stats(12_288, true, false, false)),
            Some(KnownSize::Exact { bytes: 12_288 })
        );
    }

    /// The listing's `<dir>`: a partial count with nothing counted yet says
    /// nothing, ❌ never "at least 0 bytes".
    #[test]
    fn an_incomplete_folder_with_nothing_counted_is_unknown() {
        assert_eq!(prompt_size(&stats(0, false, false, false)), None);
    }

    /// A complete count from an older epoch can be off in either direction, and
    /// a write in flight can move the number down, so neither is a floor.
    #[test]
    fn a_stale_or_pending_folder_size_is_unknown() {
        assert_eq!(prompt_size(&stats(12_288, true, true, false)), None);
        assert_eq!(prompt_size(&stats(12_288, true, false, true)), None);
        assert_eq!(prompt_size(&stats(12_288, false, false, true)), None);
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

    /// The case a phased first index makes ROUTINE rather than rare: a subtree
    /// whose ancestors are listed but whose depths haven't been walked yet, so
    /// `min_subtree_epoch` is `0` and the index holds a LOWER BOUND on its leaves.
    /// Stamping `full` over that would record a search scope missing every file the
    /// walker hasn't reached, silently.
    ///
    /// ⚠️ Distinct from the stale case above (`0 < min < current`), and it has to
    /// stay distinct: a volume is now partially covered for minutes at a time on
    /// every first run.
    #[test]
    fn a_partially_covered_subtree_downgrades_rather_than_claiming_full() {
        let (_dir, _conn, photos_id) = index_with_subtree(3, false);
        // What a walk that hasn't got here yet leaves: rows, and no coverage claim.
        {
            let db = _dir.path().join("index.sqlite");
            let wconn = IndexStore::open_write_connection(&db).expect("write conn");
            IndexStore::upsert_dir_stats_by_id(
                &wconn,
                &[cmdr_index::store::DirStatsById {
                    entry_id: photos_id,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 3,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                }],
            )
            .expect("upsert dir stats");
        }
        let conn = IndexStore::open_read_connection(&_dir.path().join("index.sqlite")).expect("read conn");
        let buffered = enumerate_from_id(&conn, photos_id, SEARCH_LEAF_CAP);
        assert_eq!(
            buffered.coverage,
            SearchCoverage::TopLevelOnly,
            "a subtree nothing has walked to the bottom of must never claim full coverage"
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
            &[cmdr_index::store::DirStatsById {
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

    /// With no fresh index for the volume, the adapter vouches for nothing: no
    /// totals, no size, and a `volume_not_live` downgrade, never `full`.
    #[test]
    fn an_unindexed_volume_answers_uncovered() {
        let volume_id = "test://operations-host/index/unindexed";
        let path = Path::new("/nowhere/indexed/photos");
        let index = AppOperationIndex;

        assert_eq!(index.expected_totals(&[path.to_path_buf()]), None);
        assert_eq!(index.recursive_size(path), None);
        let buffered = index.search_leaves(volume_id, path, SEARCH_LEAF_CAP);
        assert_eq!(buffered.coverage, SearchCoverage::TopLevelOnly);
        assert_eq!(buffered.reason, Some(SearchCoverageReason::VolumeNotLive));
        assert!(buffered.leaves.is_empty());
    }
}
