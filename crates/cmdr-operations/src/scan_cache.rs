//! Scan-preview state: one map from `preview_id` to everything the app knows
//! about that preview, the per-file `FileInfo` / `ScanResult` carriers, and the
//! TTL safety-net over settled entries.
//!
//! These types are owned here but re-exported from `lifecycle/state.rs`, so existing
//! `state::FileInfo` / `state::ScanResult` / `state::CachedScanResult` paths
//! keep resolving.
//!
//! The map is a private field of [`ScanPreviews`], one per engine: every
//! insert, read, and take goes through a method here, so the coherence canary
//! and the request binding can't be bypassed by a new call site writing the
//! map.
//!
//! ## One map, because "the preview is gone" is ambiguous
//!
//! A preview is either in flight or settled, and a settled one carries WHY it
//! settled: complete (with its `CachedScanResult`), errored (with the typed
//! failure its operation reports), or cancelled. Splitting in-flight state from results would leave a window
//! where a worker has dropped its in-flight entry but not yet published its
//! result, and an operation waiting on that preview cannot tell that window
//! apart from "evicted", "errored", or "never existed". One entry, replaced in
//! place, makes the publication atomic.
//!
//! ## Claims
//!
//! An operation that intends to consume a preview CLAIMS it, exactly one owner
//! per preview (`claim_preview`). The claim is what lets the progress bridge
//! forward the walk's counts under the operation's id, what exempts the result
//! from TTL eviction while its owner waits behind a busy lane, and what stops a
//! dialog teardown from freeing a result an operation is about to read.

use std::path::{Path, PathBuf};

use super::transfer_tree::{NodeId, TreeModel, TreeOrder, built, from_local_scan};
use super::types::ScanPreviewPurpose;
use cmdr_fs::volume::{CopyScanResult, ScannedFile};

#[path = "scan_cache_previews.rs"]
mod previews;
pub(super) use previews::{PreviewClaim, ScanOutcome, ScanPreviewState, ScanPreviews};
#[cfg(test)]
pub(super) use previews::{SCAN_RESULT_TTL, expired_scan_result_ids};

/// Cached result from a completed scan preview.
///
/// ❌ The fields are PRIVATE and stay that way: build one through
/// `from_local_walk` or `from_volume_batch`. The two production walks emit
/// genuinely different shapes, and a hand-written literal is how a test invents
/// a third one that production has never once produced — which is exactly how
/// the empty-`per_path` bug got certified by a suite full of fully-populated
/// fixtures.
pub(super) struct CachedScanResult {
    /// What the preview was walked for. A walk keeps what its dialog needs, so
    /// one purpose's file list isn't another's: only a delete's stops at a
    /// mount and counts a selected link as the link, and the local delete
    /// walker removes the list as is (`ScanPreviews::take_cached_delete_scan`).
    purpose: ScanPreviewPurpose,
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
    /// Every file's size and date from a volume scan whose backend keeps them
    /// (S3, `BatchScanResult::files`), for a cost estimate. `None` otherwise.
    scanned_files: Option<Vec<ScannedFile>>,
    /// The transfer's live tree, built by a `ScanPreviewPurpose::Transfer`
    /// walk and handed to the operation that consumes this preview, the way
    /// `per_path` is. `None` for a delete's or a compress's preview.
    tree: Option<TreeModel>,
}

impl CachedScanResult {
    /// Carries a volume scan's kept files along, for `cached_cost_facts`.
    pub(super) fn keeping_files(self, scanned_files: Option<Vec<ScannedFile>>) -> Self {
        Self { scanned_files, ..self }
    }

    /// Carries the transfer tree a transfer's preview built.
    pub(super) fn with_tree(self, tree: Option<TreeModel>) -> Self {
        Self { tree, ..self }
    }

    /// The LOCAL `std::fs` walk's shape (`run_scan_preview`): a per-file
    /// `FileInfo` list, the directories it found, one `CopyScanResult` per
    /// top-level source, and possibly a compressed-size estimate.
    ///
    /// `file_count` is derived from `files`: a caller passing it is one more
    /// thing to get wrong.
    #[allow(
        clippy::too_many_arguments,
        reason = "one argument per field of the walk's shape; a struct for them would be this struct again"
    )]
    pub(super) fn from_local_walk(
        purpose: ScanPreviewPurpose,
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
            purpose,
            sources,
            file_count: files.len(),
            files,
            dirs,
            total_bytes,
            dedup_bytes,
            per_path,
            estimated_compressed_bytes,
            scanned_files: None,
            tree: None,
        }
    }

    /// The VOLUME batch scan's shape (`run_volume_scan_preview`, SMB / MTP): no
    /// per-file list at all (consumers read `per_path`, and a delete that needs
    /// paths recurses itself), aggregate counters, and never an estimate
    /// (remote sources don't sample).
    pub(super) fn from_volume_batch(
        purpose: ScanPreviewPurpose,
        sources: Vec<PathBuf>,
        file_count: usize,
        total_bytes: u64,
        dedup_bytes: u64,
        per_path: Vec<(PathBuf, CopyScanResult)>,
    ) -> Self {
        Self {
            purpose,
            sources,
            files: Vec::new(),
            dirs: Vec::new(),
            file_count,
            total_bytes,
            dedup_bytes,
            per_path,
            estimated_compressed_bytes: None,
            scanned_files: None,
            tree: None,
        }
    }
}

/// What a cost estimate reads off a settled preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanCostFacts {
    /// How many files the preview counted.
    pub files: usize,
    /// How many folders it counted.
    pub dirs: usize,
    /// How many bytes those files hold.
    pub bytes: u64,
    /// Every file's size, and its date where the scan saw one: a local walk's
    /// sizes (undated: a local file's date bills nothing), or an S3 scan's
    /// sizes and upload dates. `None` when the scan kept no per-file list.
    pub per_file: Option<Vec<ScannedFile>>,
    /// How many of the selected items are folders. The estimate needs the
    /// shape of the selection, not only its totals: each selected item is
    /// stat'd and its name probed at the destination, and only what lands
    /// inside a folder the operation makes skips the no-overwrite check.
    pub selected_folders: usize,
    /// The size of each selected item that's a file, in selection order.
    pub selected_file_sizes: Vec<u64>,
}

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
    /// cross-boundary overshoot (see `crates/cmdr-operations/DETAILS.md`).
    pub progress_bytes: u64,
    pub is_symlink: bool,
    /// This file's row in the transfer's live tree, set when the tree is built
    /// from the walk (`ScanResult::order_by_transfer_tree`), so the driver reports
    /// against it without looking a path up. `None` outside a transfer.
    pub tree_node: Option<NodeId>,
}

impl FileInfo {
    /// Construct a `FileInfo` from filesystem metadata, treating it as the
    /// first observation of its inode (`progress_bytes == size`). Use
    /// `with_progress_bytes` to override when the scan-side inode tracker
    /// has already seen this inode.
    pub(crate) fn new(path: PathBuf, source_root: PathBuf, metadata: &std::fs::Metadata) -> Self {
        let size = metadata.len();
        Self {
            path,
            source_root,
            size,
            progress_bytes: size,
            is_symlink: metadata.is_symlink(),
            tree_node: None,
        }
    }

    /// Override `progress_bytes` (typically with `0`) when the scan-side
    /// inode tracker reports this file shares an inode with a previously-seen
    /// `FileInfo`. Keeps `size` (the actual file size) intact for sites that
    /// need it (conflict checks, file-size limits).
    #[must_use]
    pub(crate) fn with_progress_bytes(mut self, progress_bytes: u64) -> Self {
        self.progress_bytes = progress_bytes;
        self
    }

    /// Compute the destination path for this file given the destination root.
    pub(crate) fn dest_path(&self, destination: &Path) -> PathBuf {
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
    /// The transfer tree a transfer's preview built, `None` when this result
    /// came from anywhere else.
    pub tree: Option<TreeModel>,
}

impl ScanResult {
    /// The transfer's live tree for what this local walk found, with `files`
    /// put in its order: the order the copy starts them in, so the rows below
    /// the active one really are next. The tree is the preview's, or built here
    /// from `files` and `dirs`, every file learning its node.
    ///
    /// ❗ A tree bug (`transfer_tree::built`) answers `None` and leaves `files`
    /// in the walk's order with no nodes: the copy a tree-less transfer makes.
    /// Nothing is reordered until every rank is known, so a panic midway can't
    /// leave the list half sorted.
    pub(super) fn order_by_transfer_tree(&mut self, sources: &[PathBuf], order: &TreeOrder) -> Option<TreeModel> {
        let preview_tree = self.tree.take();
        let (files, dirs) = (&self.files, &self.dirs);
        let ordered = built(order, "ordering a local walk", || {
            let (tree, nodes): (TreeModel, Vec<Option<NodeId>>) = match preview_tree {
                Some(tree) => (tree, files.iter().map(|file| file.tree_node).collect()),
                None => from_local_scan(
                    sources,
                    dirs,
                    files.iter().map(|file| (file.path.as_path(), file.size)),
                    order,
                )?,
            };
            let ranks = tree.tree_order_ranks();
            // Every walked file has a node; one without would go last, not nowhere.
            let keys: Vec<u32> = nodes
                .iter()
                .map(|node| node.map_or(u32::MAX, |node| ranks.of(node)))
                .collect();
            Some((tree, nodes, keys))
        })
        .flatten();
        let Some((tree, nodes, keys)) = ordered else {
            log::warn!(
                target: "cmdr_operations",
                "transfer tree: none for this copy, so its files go in the walk's order"
            );
            for file in &mut self.files {
                file.tree_node = None;
            }
            return None;
        };
        let mut keyed: Vec<(u32, FileInfo)> = keys
            .into_iter()
            .zip(self.files.drain(..))
            .zip(nodes)
            .map(|((key, mut file), node)| {
                file.tree_node = node;
                (key, file)
            })
            .collect();
        keyed.sort_by_key(|(key, _)| *key);
        self.files = keyed.into_iter().map(|(_, file)| file).collect();
        Some(tree)
    }
}

#[cfg(test)]
#[path = "scan_cache_tests.rs"]
mod cache_binding_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt::Instant;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

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
            is_symlink: false,
            tree_node: None,
        }
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
