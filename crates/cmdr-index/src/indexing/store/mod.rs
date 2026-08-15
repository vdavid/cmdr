//! SQLite store for the drive index.
//!
//! One DB file per indexed volume. Uses WAL mode for concurrent reads.
//! All writes go through a dedicated writer thread (see `writer.rs`).
//!
//! This file is the hub: the row and status types, the `IndexStore` handle
//! itself, and the submodules that carry everything else. The DDL, pragmas, and
//! `meta` keys are in `schema.rs`, the typed failures in `errors.rs`, path↔id
//! resolution in `paths.rs`, and the `platform_case` folding in `collation.rs`;
//! the `impl IndexStore` block is split by concern across `connection.rs`,
//! `entries.rs`, `dir_stats.rs`, and `meta.rs`.
//!
//! ## Schema v2: integer-keyed parent-child tree
//!
//! Entries use an integer primary key (`id`) with a `parent_id` foreign key.
//! The `name` column uses `COLLATE platform_case`, a custom collation registered
//! at connection init that matches the filesystem's case/normalization rules:
//! - **macOS**: case-insensitive + NFD normalization (matching APFS)
//! - **Linux**: binary comparison (matching ext4/btrfs)

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

// ── Submodules ───────────────────────────────────────────────────────
//
// The `impl IndexStore` block lives across `connection` / `entries` /
// `dir_stats` / `meta` (grouped by concern); each is `impl IndexStore { … }`
// over the struct defined below and pulls shared items in via `use super::*`.
// The four leaf layers below it hold no `IndexStore` methods at all.
mod connection;
mod dir_stats;
mod entries;
mod meta;

mod collation;
mod errors;
mod paths;
mod schema;

// Not part of the `impl IndexStore` split: a compact in-memory PROJECTION of the
// directory rows, built from `for_each_directory`. It lives beside that query
// because the two are designed for each other.
mod dir_tree;
pub(crate) use dir_tree::{ARENA_FULL, DirTree};

pub(in crate::indexing) use meta::now_unix;

pub use collation::{normalize_for_comparison, register_platform_case_collation};
pub use errors::{IndexFailure, IndexStoreError, UnreadableCause};
pub(crate) use paths::resolve_path_under;
pub use paths::{resolve_path, resolve_scan_root};
pub use schema::ROOT_ID;
pub(crate) use schema::{
    CURRENT_EPOCH_KEY, EXCLUSION_POLICY_KEY, LEDGER_HEAL_KEY, SYSTEM_DIR_EXCLUSIONS_KEY, USER_DISABLED_KEY,
    USER_ENABLED_KEY,
};
// Store-internal, reached by the submodules above through their `use super::*`.
#[cfg(test)]
use collation::platform_case_compare;
use paths::reconstruct_path;
#[cfg(test)]
use paths::reconstruct_path_from_map;
#[cfg(test)]
use schema::{ROOT_PARENT_ID, reset_schema};
use schema::{SCHEMA_VERSION, apply_pragmas, create_tables};

// ── Types ────────────────────────────────────────────────────────────

/// Dir stats keyed by path string. Used at the IPC boundary and by
/// the IPC boundary (frontend expects path-keyed dir stats).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DirStats {
    /// The directory these totals describe, absolute.
    pub path: String,
    /// Logical bytes over the whole subtree (what the files claim to be).
    pub recursive_size: u64,
    /// Physical bytes over the whole subtree (what they occupy on disk),
    /// post-dedup for hard links and clones.
    pub recursive_physical_size: u64,
    /// Files anywhere under the directory.
    pub recursive_file_count: u64,
    /// Directories anywhere under it, itself excluded.
    pub recursive_dir_count: u64,
    /// `true` if any descendant entry (or direct child) is a symlink.
    /// Used by the UI to surface "size omits symlinked content" hints.
    pub recursive_has_symlinks: bool,
    /// `true` while the indexer still has unprocessed writes affecting this
    /// directory or a descendant (a big delete/copy in flight). The frontend
    /// shows a "size updating" hourglass so the number isn't read as settled.
    /// Sourced from the in-memory `pending_sizes` tracker at build time, not the
    /// DB. See `indexing/read/pending_sizes.rs`.
    pub recursive_size_pending: bool,
    /// Whether `recursive_size` is an exact total (`true`) or a lower bound
    /// (`false`), derived backend-side from the subtree's `min_subtree_epoch`
    /// (`> 0` ⇒ exact). The FE renders an exact size when `true`, a `≥` lower
    /// bound (or `—` when size is 0) when `false`. Raw epochs never cross IPC.
    /// See the "Honest sizes" model in `indexing/DETAILS.md`.
    pub recursive_size_complete: bool,
    /// Whether the (exact) `recursive_size` was computed at an older volume epoch
    /// than the current one (accurate-but-stale). Only meaningful when
    /// `recursive_size_complete` is `true`; drives the muted "stale" treatment.
    pub recursive_size_stale: bool,
}

/// Dir stats keyed by entry ID. Used internally by the integer-keyed store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirStatsById {
    /// The `entries` row these totals belong to.
    pub entry_id: i64,
    /// Logical bytes over the whole subtree.
    pub recursive_logical_size: u64,
    /// Physical bytes over the whole subtree, post-dedup.
    pub recursive_physical_size: u64,
    /// Files anywhere under the directory.
    pub recursive_file_count: u64,
    /// Directories anywhere under it, itself excluded.
    pub recursive_dir_count: u64,
    /// `true` if the directory's subtree (including direct children) contains
    /// any symlink entries. Aggregated bottom-up alongside size totals.
    pub recursive_has_symlinks: bool,
    /// Coverage + freshness for this directory's whole subtree, as one integer:
    /// `min` over `{this dir's listed_epoch}` ∪ `{each child dir's
    /// min_subtree_epoch}`. `0` means some directory in the subtree was never
    /// listed (size is a lower bound); `> 0` means the subtree is fully covered
    /// and the value is the oldest listing epoch in it. Rolled up bottom-up by
    /// the aggregator (a separate agent's milestone); stays at its `0` default
    /// until then. See the "Honest sizes" model in `indexing/DETAILS.md`.
    pub min_subtree_epoch: u64,
}

/// A row from the integer-keyed `entries` table. Used as the primary entry
/// type by the scanner (with pre-assigned IDs) and the integer-keyed store API.
#[derive(Debug, Clone)]
pub struct EntryRow {
    /// The row's own id, assigned by the scanner before the insert.
    pub id: i64,
    /// The directory this entry sits in, or [`ROOT_ID`] for a volume root.
    pub parent_id: i64,
    /// The file or directory name, one path component.
    pub name: String,
    /// Whether it's a directory.
    pub is_directory: bool,
    /// Whether it's a symlink. Symlinked content is never followed or counted,
    /// which is what `recursive_has_symlinks` warns about.
    pub is_symlink: bool,
    /// Logical bytes, `None` for a directory or an entry we couldn't stat.
    pub logical_size: Option<u64>,
    /// Physical bytes (allocated blocks), same `None` rule.
    pub physical_size: Option<u64>,
    /// Modification time, seconds since the epoch.
    pub modified_at: Option<u64>,
    /// The filesystem's inode, when the volume's are trustworthy. What makes a
    /// rename detectable as a rename rather than a delete plus an add.
    pub inode: Option<u64>,
}

/// Mutable context held during a network (SMB/MTP) scan for assigning parent IDs.
///
/// Maintains a `HashMap<PathBuf, i64>` mapping directory paths to their
/// pre-assigned entry IDs. The `network_scanner`'s serial BFS looks up each
/// entry's parent path in this map to get its `parent_id`, assigns a fresh `id`
/// from `next_id`, and (if the entry is a directory) inserts its own mapping. The
/// LOCAL scanner does NOT use this — it carries `parent_id` through its parallel
/// walk, so it never builds a whole-volume path map.
pub(crate) struct ScanContext {
    /// Map from directory absolute path to its assigned entry ID.
    pub dir_ids: std::collections::HashMap<PathBuf, i64>,
    /// Shared ID counter. Atomically incremented to allocate unique IDs.
    /// Owned by `IndexWriter`, shared with all scanners and the writer thread.
    next_id: Arc<AtomicI64>,
}

impl ScanContext {
    /// Create a new scan context, seeding the map with the root's entry ID.
    ///
    /// `next_id` is the shared atomic counter from `IndexWriter`, the single
    /// source of truth for ID allocation. `is_volume_root` selects root handling;
    /// see [`resolve_scan_root`].
    pub fn new(
        conn: &Connection,
        root: &Path,
        is_volume_root: bool,
        next_id: Arc<AtomicI64>,
    ) -> Result<Self, IndexStoreError> {
        let root_id = resolve_scan_root(conn, root, is_volume_root)?;
        let mut dir_ids = std::collections::HashMap::new();
        dir_ids.insert(root.to_path_buf(), root_id);
        Ok(Self { dir_ids, next_id })
    }

    /// Allocate the next entry ID and advance the counter.
    pub fn alloc_id(&mut self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register a directory path with its assigned ID, so children can
    /// look up their parent_id.
    pub fn register_dir(&mut self, path: PathBuf, id: i64) {
        self.dir_ids.insert(path, id);
    }

    /// Look up the parent_id for an entry given its parent's absolute path.
    pub fn lookup_parent(&self, parent_path: &Path) -> Option<i64> {
        self.dir_ids.get(parent_path).copied()
    }
}

/// What a volume's persisted `meta` says about its last completed walk. Read off
/// disk, so it survives a restart and answers for a volume that isn't indexing
/// right now.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    /// The schema the database was written with. A mismatch rebuilds rather than
    /// migrates; the index is a disposable cache.
    pub schema_version: Option<String>,
    /// Where the volume was mounted when it was last walked.
    pub volume_path: Option<String>,
    /// When the last walk finished. Every value here is TEXT, because `meta` is
    /// a string key-value table.
    pub scan_completed_at: Option<String>,
    /// How long that walk took.
    pub scan_duration_ms: Option<String>,
    /// How many entries it recorded, the tier-1 progress denominator.
    pub total_entries: Option<String>,
    /// The previous completed scan's summed post-dedup physical bytes (TEXT, like
    /// every meta value). Surfaced for symmetry with `total_entries` and for
    /// debugging; not on the tier-1 critical path.
    pub total_physical_bytes: Option<String>,
    /// The last filesystem event the watcher had applied when the walk finished.
    /// Replay resumes from here, which is how a restart doesn't lose history.
    pub last_event_id: Option<String>,
}

/// The previous completed scan's persisted calibration, read from `meta`.
///
/// All fields are `Option` because a first-ever scan (or a DB rebuilt after a
/// schema bump / `clear_index`) has none of these keys yet. The numerator-side
/// live counters are compared against `total_entries` (tier-1 denominator) and
/// `total_physical_bytes` (tier-2 cap tuning); `scan_duration_ms` seeds the ETA.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScanCalibration {
    pub total_entries: Option<u64>,
    pub total_physical_bytes: Option<u64>,
    pub scan_duration_ms: Option<u64>,
}

impl ScanCalibration {
    /// Nothing recorded at all, so this bucket can't calibrate anything.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Which WALK produced a calibration sample. The two walks differ by roughly 5x
/// in wall clock on the same volume (a parallel truncate-and-rebuild vs a serial
/// per-directory diff), so a timing from one is a bad ETA seed for the other:
/// they're stored and read in separate `meta` buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanCalibrationKind {
    /// The walk that truncates the index and rebuilds it (a first scan or a full
    /// rebuild), local guarded walker or network trait scan.
    FullWalk,
    /// The rescan-in-place that diffs every directory against the index and
    /// writes only the changes.
    ChangeCheck,
}

impl ScanCalibrationKind {
    /// This kind's `meta` key for a calibration field, for example
    /// `scan_duration_ms_change_check`.
    pub fn meta_key(self, base: &str) -> String {
        let suffix = match self {
            Self::FullWalk => "full_walk",
            Self::ChangeCheck => "change_check",
        };
        format!("{base}_{suffix}")
    }
}

/// Every calibration bucket recorded on one index DB.
///
/// `any` holds the unsuffixed keys, which every completed scan writes whatever
/// its kind, so it's the last completed scan's numbers. It's the fallback rung:
/// a stale-but-present timing from the other walk beats showing no estimate at
/// all (and a DB predating the per-kind keys only has this one).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScanCalibrationSet {
    pub full_walk: ScanCalibration,
    pub change_check: ScanCalibration,
    pub any: ScanCalibration,
}

impl ScanCalibrationSet {
    /// The calibration to seed the ETA and the percent denominator for a run of
    /// this kind: the same kind's own numbers when it has any, else the last
    /// completed scan of either kind, else nothing (the caller falls back to the
    /// rough, untimed tier).
    pub fn for_kind(&self, kind: ScanCalibrationKind) -> ScanCalibration {
        let same_kind = match kind {
            ScanCalibrationKind::FullWalk => self.full_walk,
            ScanCalibrationKind::ChangeCheck => self.change_check,
        };
        if same_kind.is_empty() { self.any } else { same_kind }
    }
}

// ── IndexStore ───────────────────────────────────────────────────────

/// Read-oriented handle to the index database.
///
/// Holds a single read connection (WAL allows concurrent reads from any thread).
/// Write operations use a separate connection obtained via [`IndexStore::open_write_connection`].
pub struct IndexStore {
    db_path: PathBuf,
    read_conn: Connection,
}

/// Runs `f` inside a SQLite savepoint. Releases on success, rolls back on error.
///
/// SAFETY: `name` is interpolated into SQL. Only pass hardcoded string literals.
fn with_savepoint<F, T>(conn: &Connection, name: &str, f: F) -> Result<T, IndexStoreError>
where
    F: FnOnce(&Connection) -> Result<T, IndexStoreError>,
{
    conn.execute_batch(&format!("SAVEPOINT {name}"))?;
    match f(conn) {
        Ok(val) => {
            conn.execute_batch(&format!("RELEASE {name}"))?;
            Ok(val)
        }
        Err(e) => {
            // `ROLLBACK TO` undoes the work but LEAVES the savepoint open, and with
            // it the implicit transaction it started — so the `RELEASE` is
            // load-bearing, not tidiness: without it one failed write parks this
            // connection in an open transaction holding the write lock, and every
            // other connection sees `database is locked` from then on.
            // Both are silenced: the savepoint may already be gone, or the
            // connection may be in an error state.
            let _ = conn.execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"));
            Err(e)
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
