//! Reading a volume's index database into a [`SearchIndex`].
//!
//! The cost here is I/O latency, not compute, so the read is split across rowid
//! ranges rather than made a tighter loop. Why, and what the split costs in transient
//! memory: `search/DETAILS.md` § "Loading in parallel".

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;

use super::{ArenaBuilder, SearchIndex};
use crate::index_host::index;
use crate::pluralize::pluralize_with;
use cmdr_index::ReadPool;
use cmdr_index::store::{IndexStore, ROOT_ID};

// The arena names its root by its own constant; this is where it has to agree with
// the database's.
const _: () = assert!(super::ARENA_ROOT_ID == ROOT_ID);

/// Rows between cancellation checks during load.
const CANCEL_CHECK_INTERVAL: usize = 100_000;

/// Most workers a parallel load will use.
///
/// The bottleneck is the SSD, not the CPU: one thread issuing serial 4 KiB `pread`s
/// against a cold index never builds enough queue depth to reach the drive's real
/// throughput (measured: ~14-26 MB/s effective against a drive that does GB/s,
/// `ERR-S76V3`, 2026-09-19). Eight readers is where queue depth stops being the
/// limit; more would only multiply the transient merge cost below.
const MAX_LOAD_WORKERS: usize = 8;

/// Below this many rows a load stays single-threaded: splitting a small index costs
/// more in connections and segment merging than the scan itself takes.
const PARALLEL_LOAD_THRESHOLD: usize = 50_000;

/// How many ranges the last load split into. Test-only, and the reason it exists is
/// that a test asserting a parallel load matches a serial one proves nothing if the
/// fixture quietly took the single-worker path.
#[cfg(test)]
static LAST_LOAD_WORKERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// ── Index loading ────────────────────────────────────────────────────

/// How many rows to reserve for, without a full `COUNT(*)`.
///
/// **Gotcha**: `SELECT COUNT(*)` is not the cheap b-tree count it looks like. SQLite
/// serves it by fully traversing the smallest index (`idx_inode` here), which is warm-
/// cache cheap and cold-cache seconds of I/O, all to size a `Vec`. The aggregator has
/// already counted the same rows into `dir_stats` for the scan root, so one indexed
/// row read answers it. `COUNT(*)` stays as the fallback for an index whose root
/// rollup hasn't been written yet (a first scan still running).
fn row_count_estimate(conn: &rusqlite::Connection) -> usize {
    if let Ok(Some(stats)) = IndexStore::get_dir_stats_by_id(conn, ROOT_ID) {
        let total = stats.recursive_file_count.saturating_add(stats.recursive_dir_count);
        if total > 0 {
            return usize::try_from(total).unwrap_or(usize::MAX);
        }
    }
    conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get::<_, i64>(0))
        .map(|n| n.max(0) as usize)
        .unwrap_or(0)
}

/// Read the rows with `lo <= id < hi` into a segment.
///
/// `ORDER BY id` is free (`id` is the rowid, so this is the table's own scan order)
/// and says out loud that [`SearchIndex::index_of_id`]'s binary search depends on it.
fn load_range(
    pool: &ReadPool,
    lo: i64,
    hi: Option<i64>,
    reserve_rows: usize,
    cancel: &AtomicBool,
) -> Result<ArenaBuilder, String> {
    pool.with_conn(|conn: &rusqlite::Connection| {
        const AVG_NAME_BYTES: usize = 20;
        const NAMES_ARENA_CEILING: usize = 512 * 1024 * 1024; // 512 MiB

        let mut segment = ArenaBuilder::with_capacity(
            reserve_rows,
            reserve_rows.saturating_mul(AVG_NAME_BYTES).min(NAMES_ARENA_CEILING),
        );

        let sql = match hi {
            Some(_) => {
                "SELECT id, parent_id, name, is_directory, logical_size, modified_at \
                 FROM entries WHERE id >= ?1 AND id < ?2 ORDER BY id"
            }
            None => {
                "SELECT id, parent_id, name, is_directory, logical_size, modified_at \
                 FROM entries WHERE id >= ?1 ORDER BY id"
            }
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("Prepare failed: {e}"))?;
        let mut rows = match hi {
            Some(hi) => stmt.query(rusqlite::params![lo, hi]),
            None => stmt.query(rusqlite::params![lo]),
        }
        .map_err(|e| format!("Query failed: {e}"))?;

        let mut row_count = 0usize;
        while let Some(row) = rows.next().map_err(|e| format!("Row read failed: {e}"))? {
            if row_count.is_multiple_of(CANCEL_CHECK_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err("Load cancelled".to_string());
            }

            let id: i64 = row.get(0).map_err(|e| format!("{e}"))?;
            let parent_id: i64 = row.get(1).map_err(|e| format!("{e}"))?;
            // Borrow directly from SQLite's internal buffer via ValueRef: zero heap allocations.
            let name_ref = row.get_ref(2).map_err(|e| format!("{e}"))?;
            let name_str = name_ref.as_str().map_err(|e| format!("{e}"))?;
            let is_directory: bool = row.get(3).map_err(|e| format!("{e}"))?;
            let logical_size: Option<u64> = row.get(4).map_err(|e| format!("{e}"))?;
            let modified_at: Option<u64> = row.get(5).map_err(|e| format!("{e}"))?;
            segment.push(id, parent_id, name_str, is_directory, logical_size, modified_at);
            row_count += 1;
        }

        Ok(segment)
    })?
}

/// Append every row created since `index` was read: the rows with an id past its
/// highest. Returns how many it appended.
///
/// This is what keeps a warm arena honest after a walk wrote rows behind it,
/// without the full read and arena-sized peak a rebuild costs. Where the rows go
/// (in place, or one copy with fresh room) is [`SearchIndex::append_created`]'s call.
///
/// **Why that is every created row.** Ids come from the volume writer's one counter,
/// which only climbs within a process, so a row created after the arena's read has
/// an id past everything it read. The arena and the coverage token lean on the same
/// property, with the same edge across a restart (`cmdr-index`'s `read/DETAILS.md`
/// § "The freshness token"), and a restart drops every arena anyway.
///
/// **What it doesn't carry**: rows deleted or updated since the read. Those stay as
/// the arena read them, the same staleness background indexing leaves in any warm
/// arena; `search/DETAILS.md` § "Decision 12" says why that is enough for a coverage
/// answer.
pub(crate) fn catch_up_search_index(index: &mut Arc<SearchIndex>, pool: &ReadPool) -> Result<usize, String> {
    let after = index.last_id().unwrap_or(0);
    let created = load_range(pool, after + 1, None, 0, &AtomicBool::new(false))?;
    Ok(SearchIndex::append_created(index, created))
}

/// Split `[min_id, max_id]` into at most `workers` contiguous rowid ranges.
///
/// Ranges are by id, not by row, so a sparse id space (heavy churn deletes rows and
/// never reuses their rowids) gives uneven segments. That costs some parallelism and
/// nothing else: the merge is ordered by range, so the result is identical either way.
fn rowid_ranges(min_id: i64, max_id: i64, workers: usize) -> Vec<(i64, Option<i64>)> {
    if workers <= 1 || max_id <= min_id {
        return vec![(min_id, None)];
    }
    let span = (max_id - min_id + 1) as u128;
    let step = (span / workers as u128).max(1) as i64;
    let mut ranges = Vec::with_capacity(workers);
    let mut lo = min_id;
    for _ in 0..workers - 1 {
        let hi = lo.saturating_add(step);
        if hi > max_id {
            break;
        }
        ranges.push((lo, Some(hi)));
        lo = hi;
    }
    ranges.push((lo, None)); // the last range is open-ended, so a row past max_id can't be lost
    ranges
}

/// Load all entries from the index DB into an in-memory `SearchIndex`.
///
/// `name_folded` is NOT loaded: the search pattern is normalized instead
/// (NFD on macOS) to avoid ~5.1M extra String allocations and ~300 MB of memory.
///
/// The read is split across [`MAX_LOAD_WORKERS`] rowid ranges, each on its own
/// thread-local connection, because the cost is I/O latency rather than compute and
/// one thread cannot give the drive enough queue depth to answer faster.
pub(crate) fn load_search_index(pool: &ReadPool, cancel: &AtomicBool) -> Result<SearchIndex, String> {
    let t = std::time::Instant::now();
    let generation = index().search_generation();

    let (estimate, min_id, max_id) = pool.with_conn(|conn: &rusqlite::Connection| {
        let estimate = row_count_estimate(conn);
        // Both are O(1) on a rowid table: SQLite walks to the leftmost/rightmost leaf.
        let bounds: (Option<i64>, Option<i64>) = conn
            .query_row("SELECT MIN(id), MAX(id) FROM entries", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap_or((None, None));
        (estimate, bounds.0.unwrap_or(1), bounds.1.unwrap_or(0))
    })?;

    let workers = if estimate < PARALLEL_LOAD_THRESHOLD {
        1
    } else {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(MAX_LOAD_WORKERS)
    };
    let ranges = rowid_ranges(min_id, max_id, workers);
    let reserve_per_worker = estimate.div_ceil(ranges.len().max(1));

    // `collect` on an indexed parallel iterator preserves order, which the merge
    // depends on: segment k holds a strictly lower rowid range than segment k+1, so
    // concatenating them in order keeps `entries` sorted by id.
    let segments: Vec<ArenaBuilder> = if ranges.len() == 1 {
        vec![load_range(pool, ranges[0].0, ranges[0].1, estimate, cancel)?]
    } else {
        ranges
            .par_iter()
            .map(|&(lo, hi)| load_range(pool, lo, hi, reserve_per_worker, cancel))
            .collect::<Result<Vec<_>, String>>()?
    };

    #[cfg(test)]
    LAST_LOAD_WORKERS.store(ranges.len(), Ordering::Relaxed);

    let index = ArenaBuilder::merge(segments, generation);

    log::debug!(
        "Search index loaded: {}, generation {generation}, {} worker(s), took {:?}",
        pluralize_with(index.len() as u64, "entry", "entries"),
        ranges.len(),
        t.elapsed()
    );
    Ok(index)
}

#[cfg(test)]
mod tests {
    use cmdr_index::ReadPool;
    use cmdr_index::store::{IndexStore, ROOT_ID};

    use super::*;

    // ── The parallel load ────────────────────────────────────────────

    /// Build an index with enough rows to cross [`PARALLEL_LOAD_THRESHOLD`], so the
    /// load really does split into ranges rather than quietly taking the single-worker
    /// path the rest of the suite exercises.
    fn store_with_rows(dir: &std::path::Path, rows: usize) -> std::path::PathBuf {
        let db_path = dir.join("parallel-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..rows {
            // A mix of the shapes the sentinel encoding has to carry across a merge.
            let size = if i % 7 == 0 { None } else { Some(i as u64) };
            let modified = if i % 11 == 0 {
                None
            } else {
                Some(1_700_000_000 + i as u64)
            };
            IndexStore::insert_entry_v2(
                &conn,
                ROOT_ID,
                &format!("file-{i:07}.txt"),
                i % 13 == 0,
                false,
                size,
                size,
                modified,
                None,
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
        db_path
    }

    /// `index_of_id` is a binary search, so a load that returned rows out of order
    /// would answer wrong rather than slow. The parallel path merges per-range
    /// segments, which is exactly where the order could be lost.
    #[test]
    fn entries_are_sorted_by_id() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = store_with_rows(dir.path(), PARALLEL_LOAD_THRESHOLD + 5_000);

        let pool = ReadPool::new(db_path).unwrap();
        let index = load_search_index(&pool, &AtomicBool::new(false)).unwrap();

        assert!(
            (1..index.len()).all(|i| index.id(i - 1) < index.id(i)),
            "the merged arena must stay strictly ascending by id"
        );
        for row in 0..index.len() {
            let id = index.id(row);
            assert_eq!(
                index.index_of_id(id).map(|i| index.id(i)),
                Some(id),
                "every row must be findable by its own id"
            );
        }
        assert_eq!(index.index_of_id(-1), None, "an absent id finds nothing");
    }

    /// The whole point of the segment merge is that it changes nothing. Every row's
    /// name, parent, kind, size, and mtime must come out of a multi-worker load
    /// exactly as they do out of a single-worker one.
    #[test]
    fn a_parallel_load_matches_a_single_worker_load_row_for_row() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let rows = PARALLEL_LOAD_THRESHOLD + 5_000;
        let db_path = store_with_rows(dir.path(), rows);
        let pool = ReadPool::new(db_path).unwrap();

        let parallel = load_search_index(&pool, &AtomicBool::new(false)).unwrap();
        // The same read, forced onto one range, is the reference.
        let reference = {
            load_range(&pool, 1, None, 0, &AtomicBool::new(false))
                .unwrap()
                .finish(0)
        };

        assert!(
            LAST_LOAD_WORKERS.load(Ordering::Relaxed) > 1,
            "the fixture must actually take the parallel path, or this test proves nothing"
        );
        assert!(parallel.len() > rows, "the fixture should be past the threshold");
        assert_eq!(parallel.len(), reference.len());
        for i in 0..parallel.len() {
            let id = parallel.id(i);
            assert_eq!(id, reference.id(i));
            assert_eq!(parallel.parent_id(i), reference.parent_id(i));
            assert_eq!(parallel.is_directory(i), reference.is_directory(i));
            assert_eq!(
                parallel.size(i),
                reference.size(i),
                "size, sentinel and all, on id {id}"
            );
            assert_eq!(parallel.modified_at(i), reference.modified_at(i), "mtime on id {id}");
            assert_eq!(
                parallel.name(i),
                reference.name(i),
                "the rebased name offset must still address the right bytes"
            );
        }
    }

    // ── Catching up ──────────────────────────────────────────────────

    /// Rows written after `store_with_rows`, the way a walk writes behind a warm arena.
    fn write_more_rows(db_path: &std::path::Path, from: usize, rows: usize) {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        for i in from..from + rows {
            IndexStore::insert_entry_v2(
                &conn,
                ROOT_ID,
                &format!("later-{i:07}.txt"),
                false,
                false,
                Some(i as u64),
                Some(i as u64),
                None,
                None,
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
    }

    /// A caught-up arena has to be indistinguishable from one loaded fresh, name
    /// offsets above all: the appended rows address a names buffer that grew under
    /// them.
    fn assert_same_rows(caught_up: &SearchIndex, fresh: &SearchIndex) {
        assert_eq!(caught_up.len(), fresh.len());
        for i in 0..caught_up.len() {
            let id = caught_up.id(i);
            assert_eq!(id, fresh.id(i));
            assert_eq!(caught_up.parent_id(i), fresh.parent_id(i));
            assert_eq!(caught_up.size(i), fresh.size(i), "size on id {id}");
            assert_eq!(caught_up.name(i), fresh.name(i), "name on id {id}");
        }
    }

    #[test]
    fn a_caught_up_arena_matches_a_fresh_load_and_grew_where_it_stood() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = store_with_rows(dir.path(), 500);
        let pool = ReadPool::new(db_path.clone()).unwrap();
        let mut arena = Arc::new(load_search_index(&pool, &AtomicBool::new(false)).unwrap());
        let rows_at = arena.rows_address();

        write_more_rows(&db_path, 500, 40);
        let appended = catch_up_search_index(&mut arena, &pool).unwrap();

        assert_eq!(appended, 40, "exactly the rows created since the read");
        assert_same_rows(&arena, &load_search_index(&pool, &AtomicBool::new(false)).unwrap());
        assert_eq!(
            arena.rows_address(),
            rows_at,
            "an arena with room takes the rows in place, no second copy"
        );
        assert_eq!(
            catch_up_search_index(&mut arena, &pool).unwrap(),
            0,
            "and a second catch-up has nothing left to read"
        );
    }

    #[test]
    fn a_shared_arena_is_copied_and_the_reader_keeps_its_own() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = store_with_rows(dir.path(), 500);
        let pool = ReadPool::new(db_path.clone()).unwrap();
        let mut arena = Arc::new(load_search_index(&pool, &AtomicBool::new(false)).unwrap());
        // A search still reading the arena it was handed.
        let reader = Arc::clone(&arena);

        write_more_rows(&db_path, 500, 40);
        catch_up_search_index(&mut arena, &pool).unwrap();

        assert_eq!(reader.len(), 501, "the reader's arena doesn't move under it");
        assert!(!Arc::ptr_eq(&arena, &reader), "the catch-up went into a copy");
        assert_same_rows(&arena, &load_search_index(&pool, &AtomicBool::new(false)).unwrap());
    }

    #[test]
    fn an_arena_out_of_room_grows_once_and_keeps_room_after() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = store_with_rows(dir.path(), 500);
        let pool = ReadPool::new(db_path.clone()).unwrap();
        let mut arena = Arc::new(load_search_index(&pool, &AtomicBool::new(false)).unwrap());

        // More than the spare room a small arena keeps.
        let past_the_room = cmdr_search::arena::spare_rows(arena.len()) + 100;
        write_more_rows(&db_path, 500, past_the_room);
        catch_up_search_index(&mut arena, &pool).unwrap();

        assert_same_rows(&arena, &load_search_index(&pool, &AtomicBool::new(false)).unwrap());
        assert!(
            arena.spare_rows_left() >= cmdr_search::arena::spare_rows(arena.len()),
            "a grown arena keeps room for the next catch-up"
        );
    }

    /// Ranges must tile the id space with no gap and no overlap, or the merge either
    /// loses rows or double-counts them. The last one is open-ended on purpose: a row
    /// written after `MAX(id)` was read still has to land.
    #[test]
    fn rowid_ranges_tile_the_id_space() {
        let ranges = rowid_ranges(1, 1000, 4);
        assert_eq!(ranges.len(), 4);
        assert_eq!(ranges[0].0, 1);
        assert_eq!(ranges.last().unwrap().1, None, "the last range stays open-ended");
        for pair in ranges.windows(2) {
            assert_eq!(
                pair[0].1,
                Some(pair[1].0),
                "each range must start exactly where the previous one ended"
            );
        }

        assert_eq!(rowid_ranges(1, 1000, 1).len(), 1, "one worker means one range");
        assert_eq!(rowid_ranges(1, 1, 8).len(), 1, "a one-row index doesn't split");
    }

    /// A cancelled load has to return promptly rather than reading the rest of the
    /// drive into memory first. Every worker watches the same flag.
    #[test]
    fn a_cancelled_load_gives_up() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = store_with_rows(dir.path(), PARALLEL_LOAD_THRESHOLD + 5_000);
        let pool = ReadPool::new(db_path).unwrap();

        let cancelled = AtomicBool::new(true);
        let result = load_search_index(&pool, &cancelled);
        assert!(result.is_err(), "a pre-cancelled load must not build an arena");
    }

    /// The row estimate has to come off `dir_stats` rather than a full `COUNT(*)`
    /// traversal, and it has to stay right when that rollup hasn't been written.
    #[test]
    fn the_row_estimate_survives_a_missing_root_rollup() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = store_with_rows(dir.path(), 100);
        let pool = ReadPool::new(db_path).unwrap();

        // No `dir_stats` row for the scan root yet: a first scan still running.
        let estimate = pool.with_conn(row_count_estimate).unwrap();
        assert!(estimate >= 100, "the COUNT(*) fallback still answers, got {estimate}");

        // The arena is correct either way; the estimate only sizes the allocation.
        let index = load_search_index(&pool, &AtomicBool::new(false)).unwrap();
        assert_eq!(index.len(), 101, "root sentinel plus the 100 rows");
    }
}
