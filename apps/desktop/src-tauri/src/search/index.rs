//! Search index: in-memory entry storage and the arena loader.
//!
//! One `SearchIndex` per volume. The per-volume registry, lifecycle timers, and
//! importance weights live in [`super::volumes`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::index_host::index;
use crate::pluralize::pluralize_with;
use cmdr_index::ReadPool;

#[cfg(test)]
mod memory_tests;

// ── An optional u64 that costs eight bytes ───────────────────────────

/// A `u64` that may be absent, in eight bytes instead of `Option<u64>`'s sixteen.
///
/// **Why it exists.** A `u64` uses every one of its bit patterns, so `Option<u64>` has
/// no niche to hide `None` in and Rust adds a whole discriminant word. Two of those in
/// [`SearchEntry`] were 32 of its 56 bytes, for two values needing 8 each — and the
/// arena holds one entry per file on the volume (6.0 M rows on David's boot disk), so
/// that padding alone was ~97 MB of resident memory whenever someone searched.
///
/// **Why `u64::MAX` can't collide with a real value.** Both values this wraps come out
/// of SQLite `INTEGER` columns, which are SIGNED 64-bit, so the index physically cannot
/// store or return anything above `i64::MAX` — `u64::MAX` is a full bit outside the
/// representable range, not merely an implausible value in it. It's unreachable on the
/// physics too: `u64::MAX` bytes is 16 EiB, twice APFS's own 8 EiB per-file ceiling, and
/// `u64::MAX` seconds is ~5.8 × 10^11 years after 1970.
///
/// ❌ **Don't "simplify" this back to `Option<u64>`.** The 16 bytes it costs are real
/// and the encoding is invisible from outside: nothing can read the sentinel, because
/// [`get`](Self::get) is the only way in.
///
/// **`None` is meaningful in both fields and must survive exactly.** `size` is NULL for
/// every 2nd+ name of a hardlinked inode (the index counts an inode's bytes once, on one
/// name, and stores NULL on the rest — 934,793 of 6.0 M rows on David's disk), so
/// collapsing `None` into `0` would quietly change what folder totals and size filters
/// report on a hardlink-heavy tree. `modified_at` is NULL where the time is unknown.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OptU64(u64);

impl OptU64 {
    /// The absent value.
    pub const NONE: Self = Self(u64::MAX);

    /// Encode an optional value.
    ///
    /// `Some(u64::MAX)` would encode as absent, which is why the type's doc comment
    /// argues at length that no such value can reach here. The `debug_assert` catches a
    /// future caller that finds a way; it compiles out of the shipping build, which
    /// matters because this runs once per row per arena load.
    #[inline]
    pub fn new(value: Option<u64>) -> Self {
        debug_assert!(
            value != Some(u64::MAX),
            "u64::MAX is OptU64's absent marker, so a real u64::MAX would read back as None. \
             SQLite INTEGER columns are signed, so this shouldn't be reachable from the index."
        );
        Self(value.unwrap_or(u64::MAX))
    }

    /// Decode back to an `Option<u64>`. The only way to read one.
    #[inline]
    pub fn get(self) -> Option<u64> {
        (self.0 != u64::MAX).then_some(self.0)
    }
}

/// Prints like the `Option<u64>` it stands for, so a debug dump of an arena row never
/// shows a bare `18446744073709551615` for "unknown".
impl std::fmt::Debug for OptU64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.get(), f)
    }
}

// ── Search entry (in-memory representation) ──────────────────────────

/// One row of the arena, 40 bytes. See [`OptU64`] before adding a field: the arena holds
/// one of these per file on the volume, so a byte here is megabytes of peak footprint,
/// and `search/index/memory_tests.rs` pins the size.
#[derive(Debug)]
pub struct SearchEntry {
    pub id: i64,
    pub parent_id: i64,
    pub name_offset: u32, // byte offset into SearchIndex.names
    pub name_len: u16,    // byte length (max filename 255 chars = up to 765 bytes UTF-8)
    pub is_directory: bool,
    /// Apparent size in bytes, absent for a directory (its recursive size lives in
    /// `dir_stats`) and for every 2nd+ name of a hardlinked inode.
    pub size: OptU64,
    /// Last-modified time in seconds since the Unix epoch, absent where unknown.
    pub modified_at: OptU64,
}

// ── Search index ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SearchIndex {
    pub names: String, // arena: all filenames concatenated
    pub entries: Vec<SearchEntry>,
    pub id_to_index: HashMap<i64, usize>,
    pub generation: u64,
}

impl SearchIndex {
    /// Empty sentinel index used during async load.
    pub fn empty() -> Self {
        Self {
            names: String::new(),
            entries: Vec::new(),
            id_to_index: HashMap::new(),
            generation: 0,
        }
    }

    /// Get the filename for an entry from the arena buffer.
    pub(crate) fn name(&self, entry: &SearchEntry) -> &str {
        &self.names[entry.name_offset as usize..entry.name_offset as usize + entry.name_len as usize]
    }
}

/// Rows between cancellation checks during load.
const CANCEL_CHECK_INTERVAL: usize = 100_000;

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Index loading ────────────────────────────────────────────────────

/// Load all entries from the index DB into an in-memory `SearchIndex`.
///
/// `name_folded` is NOT loaded: the search pattern is normalized instead
/// (NFD on macOS) to avoid ~5.1M extra String allocations and ~300 MB of memory.
pub(crate) fn load_search_index(pool: &ReadPool, cancel: &AtomicBool) -> Result<SearchIndex, String> {
    pool.with_conn(|conn: &rusqlite::Connection| {
        let t = std::time::Instant::now();
        let generation = index().search_generation();

        let sql = "SELECT id, parent_id, name, is_directory, logical_size, modified_at FROM entries";

        let mut stmt = conn.prepare(sql).map_err(|e| format!("Prepare failed: {e}"))?;

        // Phase 1: Load all entries into Vec (sequential writes to contiguous memory)
        // Arena-allocate all filenames into a single String to avoid per-entry heap allocations.
        // Right-size both from the actual row count: a small index used to pay a fixed
        // ~100 MB / 5M-slot worst-case allocation on every load. `COUNT(*)` is a cheap
        // b-tree count, run once. The name arena estimate is clamped so a bogus count
        // can't request gigabytes; the Vec/String still grow if the estimate is low.
        const AVG_NAME_BYTES: usize = 20;
        const NAMES_ARENA_CEILING: usize = 512 * 1024 * 1024; // 512 MiB
        let row_count_estimate: usize = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get::<_, i64>(0))
            .map(|n| n.max(0) as usize)
            .unwrap_or(0);
        let names_capacity = row_count_estimate
            .saturating_mul(AVG_NAME_BYTES)
            .min(NAMES_ARENA_CEILING);
        let mut names = String::with_capacity(names_capacity);
        let mut entries = Vec::with_capacity(row_count_estimate);

        let mut rows = stmt.query([]).map_err(|e| format!("Query failed: {e}"))?;
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
            let name_offset = names.len() as u32;
            let name_len = name_str.len() as u16;
            names.push_str(name_str);
            let is_directory: bool = row.get(3).map_err(|e| format!("{e}"))?;
            let logical_size: Option<u64> = row.get(4).map_err(|e| format!("{e}"))?;
            let modified_at: Option<u64> = row.get(5).map_err(|e| format!("{e}"))?;
            entries.push(SearchEntry {
                id,
                parent_id,
                name_offset,
                name_len,
                is_directory,
                size: OptU64::new(logical_size),
                modified_at: OptU64::new(modified_at),
            });
            row_count += 1;
        }

        // Phase 2: Build id_to_index from completed Vec (sequential reads + HashMap writes)
        let mut id_to_index = HashMap::with_capacity(entries.len());
        for (i, entry) in entries.iter().enumerate() {
            id_to_index.insert(entry.id, i);
        }

        log::debug!(
            "Search index loaded: {}, generation {generation}, took {:?}",
            pluralize_with(row_count as u64, "entry", "entries"),
            t.elapsed()
        );
        Ok(SearchIndex {
            names,
            entries,
            id_to_index,
            generation,
        })
    })?
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use cmdr_index::ReadPool;
    use cmdr_index::store::{IndexStore, ROOT_ID};

    use super::*;

    // ── The sentinel encoding ────────────────────────────────────────

    /// Every value the index can hold must survive the eight-byte encoding unchanged,
    /// `None` included and `0` above all: a zero-byte file is a real thing, and reading
    /// it back as "size unknown" would let it slip through a size filter.
    #[test]
    fn every_representable_value_survives_the_round_trip() {
        assert_eq!(OptU64::new(None).get(), None);
        assert_eq!(OptU64::NONE.get(), None);

        for value in [
            0,                   // an empty file, and a folder modified at the epoch
            1,                   // the smallest non-empty file
            512,                 // one block
            1_700_000_000,       // a plausible mtime
            994_663_481_856,     // the largest file on David's disk (2026-08-06)
            u32::MAX as u64,     // where a 32-bit size would have wrapped
            u32::MAX as u64 + 1, // and just past it
            i64::MAX as u64,     // the largest value a SQLite INTEGER can hold
            u64::MAX - 1,        // and the largest this encoding can hold at all
        ] {
            assert_eq!(
                OptU64::new(Some(value)).get(),
                Some(value),
                "{value} should survive the round trip"
            );
        }
    }

    /// A hardlink-deduped row (NULL `logical_size`) has to read back as `None`, not as
    /// `0`. The index counts an inode's bytes once and stores NULL on every other name,
    /// so collapsing the two would change what folder totals and size filters report on
    /// a hardlink-heavy tree — silently, and only on the rows nobody looks at twice.
    #[test]
    fn a_hardlink_deduped_row_loads_back_as_unknown_size() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Two names for one inode, the way the scanner writes them: the first carries the
        // bytes, the second carries NULL. Plus an empty file, which is NOT the same thing.
        let inode = Some(4_242_424_242);
        IndexStore::insert_entry_v2(
            &conn,
            ROOT_ID,
            "first-link",
            false,
            false,
            Some(4096),
            Some(4096),
            Some(1),
            inode,
        )
        .unwrap();
        IndexStore::insert_entry_v2(&conn, ROOT_ID, "second-link", false, false, None, None, Some(1), inode).unwrap();
        IndexStore::insert_entry_v2(&conn, ROOT_ID, "empty.txt", false, false, Some(0), Some(0), None, None).unwrap();

        let pool = ReadPool::new(db_path).unwrap();
        let index = load_search_index(&pool, &AtomicBool::new(false)).unwrap();
        let size_of = |name: &str| {
            index
                .entries
                .iter()
                .find(|e| index.name(e) == name)
                .unwrap_or_else(|| panic!("{name} should be in the arena"))
                .size
                .get()
        };

        assert_eq!(
            size_of("first-link"),
            Some(4096),
            "the name carrying the bytes keeps them"
        );
        assert_eq!(
            size_of("second-link"),
            None,
            "a deduped name is sizeless, not zero-sized"
        );
        assert_eq!(
            size_of("empty.txt"),
            Some(0),
            "an empty file is zero-sized, not sizeless"
        );
    }

    /// A NULL `modified_at` is "unknown", and must not read back as the epoch: the
    /// ranker treats unknown as oldest-possible, and `sortBy: modified` sorts unknown
    /// keys last rather than pretending they're from 1970.
    #[test]
    fn an_unknown_modified_time_loads_back_as_none() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        IndexStore::insert_entry_v2(
            &conn,
            ROOT_ID,
            "undated.txt",
            false,
            false,
            Some(7),
            Some(7),
            None,
            None,
        )
        .unwrap();
        IndexStore::insert_entry_v2(
            &conn,
            ROOT_ID,
            "epoch.txt",
            false,
            false,
            Some(7),
            Some(7),
            Some(0),
            None,
        )
        .unwrap();

        let pool = ReadPool::new(db_path).unwrap();
        let index = load_search_index(&pool, &AtomicBool::new(false)).unwrap();
        let modified = |name: &str| {
            index
                .entries
                .iter()
                .find(|e| index.name(e) == name)
                .unwrap_or_else(|| panic!("{name} should be in the arena"))
                .modified_at
                .get()
        };

        assert_eq!(modified("undated.txt"), None, "an unknown time stays unknown");
        assert_eq!(modified("epoch.txt"), Some(0), "a real epoch timestamp isn't 'unknown'");
    }

    // ── Integration test: load from real SQLite DB ───────────────────

    #[test]
    fn integration_load_and_search() {
        use super::super::engine::search;
        use super::super::ranking::ImportanceWeights;
        use super::super::types::{PatternType, SearchQuery};

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Insert test entries
        let users_id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
        let alice_id =
            IndexStore::insert_entry_v2(&conn, users_id, "alice", true, false, None, None, None, None).unwrap();
        let _pdf_id = IndexStore::insert_entry_v2(
            &conn,
            alice_id,
            "report.pdf",
            false,
            false,
            Some(1_000_000),
            Some(1_000_000),
            Some(1700000000),
            None,
        )
        .unwrap();
        let _txt_id = IndexStore::insert_entry_v2(
            &conn,
            alice_id,
            "notes.txt",
            false,
            false,
            Some(500),
            Some(500),
            Some(1700000100),
            None,
        )
        .unwrap();

        // Load the index using ReadPool
        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(false);
        let index = load_search_index(&pool, &cancel).unwrap();

        // Root sentinel + 4 entries
        assert_eq!(index.entries.len(), 5);
        assert_eq!(index.id_to_index.len(), 5);

        // Search for PDFs
        let query = SearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            include_path_ids: None,
            count_only: false,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
            sort_by: None,
        };
        let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "report.pdf");
        assert_eq!(result.entries[0].path, "/Users/alice/report.pdf");
    }

    #[test]
    fn load_rightsizes_arena_from_row_count() {
        // A small index must not pre-allocate the ~5M-entry / ~100 MB worst-case
        // arena. Capacity should track the actual row count.
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");

        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(false);
        let index = load_search_index(&pool, &cancel).unwrap();

        // Root sentinel only: before right-sizing this was Vec::with_capacity(5_000_000).
        assert!(
            index.entries.capacity() < 1000,
            "entries capacity {} should track the row count, not the 5M worst case",
            index.entries.capacity()
        );
    }

    #[test]
    fn load_cancellation() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");

        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(true); // Pre-cancelled
        let result = load_search_index(&pool, &cancel);
        // With only the root sentinel, cancellation check happens at row 0, but CANCEL_CHECK_INTERVAL
        // is 100K so the first check is at row 0 (0 % 100K == 0). The load should be cancelled.
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cancelled"));
    }
}
