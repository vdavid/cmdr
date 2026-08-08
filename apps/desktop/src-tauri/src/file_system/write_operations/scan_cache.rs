//! Scan-preview caching: the in-flight scan-preview state, the cached scan
//! results, the per-file `FileInfo` / `ScanResult` carriers, and the TTL
//! safety-net for the result cache.
//!
//! These types are owned here but re-exported from `lifecycle/state.rs`, so existing
//! `state::FileInfo` / `state::ScanResult` / `state::CachedScanResult` paths
//! keep resolving.
//!
//! `SCAN_PREVIEW_RESULTS` itself is private to this module: every insert, read,
//! and take goes through a function here, so the coherence canary and the
//! request binding can't be bypassed by a new call site writing the map.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};

use crate::file_system::volume::CopyScanResult;

// ============================================================================
// Scan preview state
// ============================================================================

/// State for a scan preview operation.
pub(super) struct ScanPreviewState {
    pub cancelled: AtomicBool,
    pub progress_interval: Duration,
}

/// Cached result from a completed scan preview.
///
/// ❌ The fields are PRIVATE and stay that way: build one through
/// `from_local_walk` or `from_volume_batch`. The two production walks emit
/// genuinely different shapes, and a hand-written literal is how a test invents
/// a third one that production has never once produced — which is exactly how
/// the empty-`per_path` bug got certified by a suite full of fully-populated
/// fixtures.
pub(super) struct CachedScanResult {
    /// The top-level paths the preview was asked to walk. A `preview_id` proves
    /// the frontend once asked for a scan; it proves nothing about WHICH scan,
    /// so `take_cached_scan_result` refuses an entry whose sources don't match
    /// the operation's own. Without this, an operation on `/b` would happily
    /// act on a cached preview of `/a`, and on delete that has no rollback.
    sources: Vec<PathBuf>,
    files: Vec<FileInfo>,
    dirs: Vec<PathBuf>,
    file_count: usize,
    /// Write footprint (un-dedup'd). See `CopyScanResult::total_bytes`.
    total_bytes: u64,
    /// `du`-equivalent source footprint (hardlinks counted once). See
    /// `CopyScanResult::dedup_bytes`.
    dedup_bytes: u64,
    /// Per-source-path scan results: one `CopyScanResult` per top-level source,
    /// carrying its type and totals. The copy pipeline's cached branch rebuilds
    /// `source_hints` from it rather than probing `is_directory` per path
    /// (which on MTP each list the parent dir), so a source missing from here
    /// costs a round trip and a source wrongly absent costs a wrong answer.
    per_path: Vec<(PathBuf, CopyScanResult)>,
    /// Compressed-size estimate for a compress-mode local scan, carried so the
    /// recovery path (`get_scan_preview_totals`) can hand it back when the FE
    /// missed the complete event. `None` for copy/move and remote scans.
    estimated_compressed_bytes: Option<super::types::CompressedSizeEstimate>,
    /// When this result was inserted into `SCAN_PREVIEW_RESULTS`. Drives the
    /// TTL safety net (`prune_expired_scan_results`): a forgetful caller that
    /// never consumes the cache (dialog dismissed, op never started) can't leak
    /// tens of thousands of `FileInfo` unbounded — entries older than
    /// `SCAN_RESULT_TTL` are evicted on the next insert.
    inserted_at: Instant,
}

impl CachedScanResult {
    /// The LOCAL `std::fs` walk's shape (`run_scan_preview`): a per-file
    /// `FileInfo` list, the directories it found, one `CopyScanResult` per
    /// top-level source, and possibly a compressed-size estimate.
    ///
    /// `file_count` is derived from `files`, and `inserted_at` is stamped here:
    /// a caller passing either is one more thing to get wrong.
    pub(super) fn from_local_walk(
        sources: Vec<PathBuf>,
        files: Vec<FileInfo>,
        dirs: Vec<PathBuf>,
        total_bytes: u64,
        dedup_bytes: u64,
        per_path: Vec<(PathBuf, CopyScanResult)>,
        estimated_compressed_bytes: Option<super::types::CompressedSizeEstimate>,
    ) -> Self {
        // Stronger than `insert_scan_result`'s generic canary, because this
        // constructor knows which shape it's building: a local walk that found
        // files walked at least one source, so it recorded at least one.
        debug_assert!(
            files.is_empty() || !per_path.is_empty(),
            "a local walk that found {} files must record a per-source result",
            files.len()
        );
        Self {
            sources,
            file_count: files.len(),
            files,
            dirs,
            total_bytes,
            dedup_bytes,
            per_path,
            estimated_compressed_bytes,
            inserted_at: Instant::now(),
        }
    }

    /// The VOLUME batch scan's shape (`run_volume_scan_preview`, SMB / MTP): no
    /// per-file list at all (consumers read `per_path`, and a delete that needs
    /// paths recurses itself), aggregate counters, and never an estimate
    /// (remote sources don't sample).
    pub(super) fn from_volume_batch(
        sources: Vec<PathBuf>,
        file_count: usize,
        total_bytes: u64,
        dedup_bytes: u64,
        per_path: Vec<(PathBuf, CopyScanResult)>,
    ) -> Self {
        Self {
            sources,
            files: Vec::new(),
            dirs: Vec::new(),
            file_count,
            total_bytes,
            dedup_bytes,
            per_path,
            estimated_compressed_bytes: None,
            inserted_at: Instant::now(),
        }
    }
}

/// How long a cached scan result lives before the TTL safety net evicts it.
/// The normal lifecycle frees results far sooner (`take_cached_scan_result` at
/// op start, or `release_scan_preview` on dialog teardown); this only catches
/// the case where neither fires.
pub(super) const SCAN_RESULT_TTL: Duration = Duration::from_secs(300);

/// Returns the preview ids in `entries` whose `inserted_at` is older than
/// `ttl` relative to `now`. Pure so it's unit-testable without touching the
/// global cache. Callers remove the returned ids under the write lock.
pub(super) fn expired_scan_result_ids<'a>(
    entries: impl IntoIterator<Item = (&'a String, Instant)>,
    now: Instant,
    ttl: Duration,
) -> Vec<String> {
    entries
        .into_iter()
        .filter(|(_, inserted_at)| now.duration_since(*inserted_at) > ttl)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Evicts cache entries older than `SCAN_RESULT_TTL`, then inserts `result`
/// under `preview_id`. The single choke point for `SCAN_PREVIEW_RESULTS`
/// inserts so the TTL sweep can't be forgotten by a new call site, and the one
/// place the coherence canary below can see every entry.
pub(super) fn insert_scan_result(preview_id: String, result: CachedScanResult) {
    // The incoherent shape: a completed walk that counted files but recorded no
    // per-source result. Downstream that reads as "no information", and a copy
    // driver turns "no information" into a confident `is_directory: false` — the
    // exact lie that streamed a directory as a file and let a failed copy
    // recursively delete a merged destination. One-directional on purpose: a
    // volume batch legitimately caches an empty `files` list with a populated
    // `per_path`, never the reverse.
    if result.file_count > 0 && result.per_path.is_empty() {
        log::warn!(
            target: "cmdr_lib::write_operations",
            "scan preview {} completed a walk of {} files with no per-source results; downstream hints will be empty",
            preview_id,
            result.file_count
        );
        debug_assert!(
            false,
            "a completed walk with file_count > 0 must carry per_path entries (preview {preview_id})"
        );
    }
    if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
        let now = Instant::now();
        let expired = expired_scan_result_ids(cache.iter().map(|(k, v)| (k, v.inserted_at)), now, SCAN_RESULT_TTL);
        for id in expired {
            cache.remove(&id);
        }
        cache.insert(preview_id, result);
    }
}

/// Tries to get cached scan results for a preview, removing them from cache.
///
/// `requested_sources` is the selection the OPERATION was asked to act on. A
/// cached entry that describes a different set is not a cache hit: the entry is
/// dropped, a warn names both lists, and the caller falls through to its own
/// fresh scan. Order doesn't matter (it's a frontend detail, and `per_path` is
/// already order-rebuilt), so the comparison is set-wise — and deliberately
/// literal: if the frontend can hand back a path that differs only by a
/// trailing separator, that belongs fixed at the IPC edge, not softened here
/// into another belief.
pub(super) fn take_cached_scan_result(preview_id: &str, requested_sources: &[PathBuf]) -> Option<ScanResult> {
    let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() else {
        return None;
    };
    let cached = cache.remove(preview_id)?;
    if !same_path_set(&cached.sources, requested_sources) {
        log::warn!(
            target: "cmdr_lib::write_operations",
            "scan preview {} describes {:?}, but the operation asked for {:?}; ignoring the cache and rescanning",
            preview_id,
            cached.sources,
            requested_sources
        );
        return None;
    }
    Some(ScanResult {
        files: cached.files,
        dirs: cached.dirs,
        file_count: cached.file_count,
        total_bytes: cached.total_bytes,
        dedup_bytes: cached.dedup_bytes,
        per_path: cached.per_path,
    })
}

/// Whether two path lists hold the same paths, ignoring order and duplicates.
fn same_path_set(a: &[PathBuf], b: &[PathBuf]) -> bool {
    let left: HashSet<&Path> = a.iter().map(PathBuf::as_path).collect();
    let right: HashSet<&Path> = b.iter().map(PathBuf::as_path).collect();
    left == right
}

/// Reads the cached totals for `preview_id` without consuming the entry. Keeps
/// the lock and the map behind the module wall for `get_scan_preview_totals`,
/// the compress dialog's size estimate.
pub(super) fn cached_scan_totals(preview_id: &str) -> Option<super::types::ScanPreviewTotals> {
    let cache = SCAN_PREVIEW_RESULTS.read().ok()?;
    let cached = cache.get(preview_id)?;
    Some(super::types::ScanPreviewTotals {
        files_total: cached.file_count,
        dirs_total: cached.dirs.len(),
        bytes_total: cached.total_bytes,
        dedup_bytes_total: cached.dedup_bytes,
        estimated_compressed_bytes: cached.estimated_compressed_bytes.clone(),
    })
}

/// Seeds an entry that deliberately fails `insert_scan_result`'s coherence
/// canary, for the fixtures that pin what downstream does when a half-empty
/// preview reaches it anyway. The canary is a `debug_assert!`, so in a release
/// build an incoherent entry still lands in the cache and the drivers still
/// have to handle it without lying; these fixtures are that defense's only
/// proof. ❌ Not a general-purpose seeder: everything else goes through
/// `insert_scan_result`.
#[cfg(test)]
pub(crate) fn seed_incoherent_scan_result_for_test(
    preview_id: String,
    sources: Vec<PathBuf>,
    file_count: usize,
    total_bytes: u64,
) {
    let result = CachedScanResult {
        sources,
        files: Vec::new(),
        dirs: Vec::new(),
        file_count,
        total_bytes,
        dedup_bytes: total_bytes,
        per_path: Vec::new(),
        estimated_compressed_bytes: None,
        inserted_at: Instant::now(),
    };
    if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
        cache.insert(preview_id, result);
    }
}

/// Drops the cached scan result for `preview_id`, if any. Called on dialog
/// teardown (`release_scan_preview`) so a result that finished scanning but was
/// never consumed by a started op doesn't linger until quit.
pub(super) fn release_scan_result(preview_id: &str) {
    if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
        cache.remove(preview_id);
    }
}

/// Global cache for scan preview states.
pub(super) static SCAN_PREVIEW_STATE: LazyLock<RwLock<HashMap<String, Arc<ScanPreviewState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Global cache for completed scan preview results. ❌ Private on purpose, and
/// it stays that way: every entry has to pass `insert_scan_result`'s coherence
/// canary, and every read has to go through `take_cached_scan_result`'s
/// request binding. A `pub(super)` static is a choke point anyone can walk
/// around.
static SCAN_PREVIEW_RESULTS: LazyLock<RwLock<HashMap<String, CachedScanResult>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// ============================================================================
// FileInfo (used for scanning and sorting)
// ============================================================================

/// File info collected during scan (used for sorting).
#[derive(Debug, Clone)]
pub(super) struct FileInfo {
    pub path: PathBuf,
    /// Parent of the original source (used to compute relative path for destination)
    pub source_root: PathBuf,
    pub size: u64,
    /// Bytes this entry contributes to operation progress. Equals `size` for
    /// the first occurrence of an inode in the scan; `0` for subsequent
    /// hardlink pairs to the same inode. Active-phase counters (delete,
    /// trash, copy, move) sum this so the bar denominator (`total_bytes`,
    /// also dedup'd at scan time) and the numerator (`bytes_done`) stay in
    /// agreement. Without this split, a hardlink-heavy tree like cargo's
    /// `target/` overshoots — 81.6 GB delete numerator against a 59.84 GB
    /// scan denominator on a real-world repro.
    ///
    /// Set per call site: scan sets it from inode tracking; sites that build
    /// `FileInfo` without inode info (the oracle path in `walk_cached_entries`,
    /// MTP synthesis) fall back to `size` and accept the documented
    /// cross-boundary overshoot (see write_operations CLAUDE.md gotcha).
    pub progress_bytes: u64,
    pub modified: u64, // Unix timestamp in seconds
    pub created: u64,  // Unix timestamp in seconds
    pub is_symlink: bool,
}

impl FileInfo {
    /// Construct a `FileInfo` from filesystem metadata, treating it as the
    /// first observation of its inode (`progress_bytes == size`). Use
    /// `with_progress_bytes` to override when the scan-side inode tracker
    /// has already seen this inode.
    pub fn new(path: PathBuf, source_root: PathBuf, metadata: &std::fs::Metadata) -> Self {
        use std::time::UNIX_EPOCH;
        let size = metadata.len();
        Self {
            path,
            source_root,
            size,
            progress_bytes: size,
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            created: metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            is_symlink: metadata.is_symlink(),
        }
    }

    /// Override `progress_bytes` (typically with `0`) when the scan-side
    /// inode tracker reports this file shares an inode with a previously-seen
    /// `FileInfo`. Keeps `size` (the actual file size) intact for sites that
    /// need it (sorting, conflict checks).
    #[must_use]
    pub fn with_progress_bytes(mut self, progress_bytes: u64) -> Self {
        self.progress_bytes = progress_bytes;
        self
    }

    /// Get extension for sorting (lowercase, empty string if none).
    pub fn extension(&self) -> String {
        self.path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    /// Get filename for sorting (lowercase).
    pub fn name_lower(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    /// Compute the destination path for this file given the destination root.
    pub fn dest_path(&self, destination: &Path) -> PathBuf {
        // Strip source_root from path to get relative path, then join with destination
        if let Ok(relative) = self.path.strip_prefix(&self.source_root) {
            destination.join(relative)
        } else {
            // Fallback: just use the filename
            destination.join(self.path.file_name().unwrap_or_default())
        }
    }
}

/// Information about files to be processed.
pub(super) struct ScanResult {
    pub files: Vec<FileInfo>,
    /// For deletion: in reverse order, deepest first.
    pub dirs: Vec<PathBuf>,
    /// Not including directories.
    pub file_count: usize,
    /// Write footprint (un-dedup'd): every file at full size. Copy's
    /// disk-space check and active-phase bar use this. See
    /// `CopyScanResult::total_bytes`.
    pub total_bytes: u64,
    /// `du`-equivalent source footprint (hardlinks counted once). Delete's
    /// active phase uses this; the Copy dialog shows it as context. See
    /// `CopyScanResult::dedup_bytes`.
    pub dedup_bytes: u64,
    /// Per-source-path scan results, populated by volume scan previews so the
    /// copy pipeline can seed `source_hints` without re-statting. Empty for
    /// local-FS scans.
    pub per_path: Vec<(PathBuf, CopyScanResult)>,
}

#[cfg(test)]
#[path = "scan_cache_tests.rs"]
mod cache_binding_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    // ---- expired_scan_result_ids (TTL safety net) ----

    #[test]
    fn expired_scan_result_ids_returns_only_stale_entries() {
        let now = Instant::now();
        let ttl = Duration::from_secs(300);
        let fresh = now - Duration::from_secs(10);
        let stale = now - Duration::from_secs(400);
        let fresh_id = String::from("fresh");
        let stale_id = String::from("stale");
        let entries = vec![(&fresh_id, fresh), (&stale_id, stale)];

        let expired = expired_scan_result_ids(entries, now, ttl);

        assert_eq!(expired, vec![String::from("stale")]);
    }

    #[test]
    fn expired_scan_result_ids_empty_when_all_fresh() {
        let now = Instant::now();
        let ttl = Duration::from_secs(300);
        let a = String::from("a");
        let b = String::from("b");
        let entries = vec![(&a, now), (&b, now - Duration::from_secs(299))];

        let expired = expired_scan_result_ids(entries, now, ttl);

        assert!(expired.is_empty());
    }

    #[test]
    fn expired_scan_result_ids_boundary_is_strictly_greater_than_ttl() {
        // Exactly at the TTL is NOT expired; one tick past it is.
        let now = Instant::now();
        let ttl = Duration::from_secs(300);
        let at_ttl = String::from("at-ttl");
        let past_ttl = String::from("past-ttl");
        let entries = vec![
            (&at_ttl, now - Duration::from_secs(300)),
            (&past_ttl, now - Duration::from_secs(301)),
        ];

        let expired = expired_scan_result_ids(entries, now, ttl);

        assert_eq!(expired, vec![String::from("past-ttl")]);
    }

    // ---- FileInfo derived sort keys ----

    fn make_file_info(path: &str, source_root: &str) -> FileInfo {
        FileInfo {
            path: PathBuf::from(path),
            source_root: PathBuf::from(source_root),
            size: 0,
            progress_bytes: 0,
            modified: 0,
            created: 0,
            is_symlink: false,
        }
    }

    #[test]
    fn extension_is_lowercased() {
        // Kills: replace extension → String::new() / → "xyzzy".
        assert_eq!(make_file_info("/x/Photo.JPG", "/x").extension(), "jpg");
        assert_eq!(make_file_info("/x/archive.TAR.GZ", "/x").extension(), "gz");
    }

    #[test]
    fn extension_is_empty_for_no_extension() {
        assert_eq!(make_file_info("/x/README", "/x").extension(), "");
    }

    #[test]
    fn name_lower_is_lowercased_filename_only() {
        // Kills: replace name_lower → String::new() / → "xyzzy".
        assert_eq!(make_file_info("/x/y/Foo.Bar", "/x").name_lower(), "foo.bar");
    }

    #[test]
    fn dest_path_preserves_relative_layout_under_destination_root() {
        // Kills: replace dest_path → Default::default().
        let info = make_file_info("/src/dir/sub/leaf.txt", "/src");
        assert_eq!(
            info.dest_path(Path::new("/dst")),
            PathBuf::from("/dst/dir/sub/leaf.txt")
        );
    }

    #[test]
    fn dest_path_falls_back_to_filename_when_prefix_does_not_match() {
        // The fallback branch: when strip_prefix fails, just place the file
        // by name at the destination root.
        let info = make_file_info("/elsewhere/file.bin", "/different-root");
        assert_eq!(info.dest_path(Path::new("/dst")), PathBuf::from("/dst/file.bin"));
    }
}
