//! The `scan_for_copy` family's inherent bodies (`scan_for_copy_impl`,
//! `scan_for_copy_batch_with_boundary_impl`, `scan_for_conflicts_impl`), which
//! the trait methods in `mod.rs` delegate to.
//!
//! Split out for the same reason `crates/cmdr-smb/src/volume/scan.rs` is: the oracle-aware batch scan is
//! the single biggest concern in this backend, and it reads as its own subject
//! rather than as one more `Volume` method. A trait impl can't span files, so the
//! trait side stays in `mod.rs` and the work lives here.

use super::super::{BatchScanResult, CopyScanResult, ScanConflict, SourceItemInfo, Volume, VolumeError};
use super::MtpVolume;
use super::mapping::map_mtp_error;
use cmdr_fs::volume::scan_walk::conflicts_against;
use cmdr_fs::volume::{ScanBoundary, ScanStop};

use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::try_get_authoritative_listing;
use log::debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

impl MtpVolume {
    pub(super) fn scan_for_copy_impl<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            debug!(
                "MtpVolume::scan_for_copy: device={}, storage={}, path={}",
                self.device_id, self.storage_id, mtp_path
            );

            self.manager
                .scan_for_copy(&self.device_id, self.storage_id, &mtp_path)
                .await
                .map_err(map_mtp_error)
        })
    }

    /// [`scan_for_copy_impl`](Self::scan_for_copy_impl) that honors `stop`, for
    /// the batch body below. The trait's single-path method hands one in nowhere,
    /// so it can't share this.
    fn scan_subtree_with_stop<'a>(
        &'a self,
        path: &'a Path,
        stop: &'a ScanStop,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);
            self.manager
                .scan_for_copy_with_stop(&self.device_id, self.storage_id, &mtp_path, stop)
                .await
                .map_err(map_mtp_error)
        })
    }

    /// Batch scan with parent-grouping + fresh-listing oracle.
    ///
    /// Decision flow:
    /// 1. Group selected paths by their parent directory (one MTP listing per parent on the cold
    ///    path is the load-bearing optimization: selecting 135 photos in `/DCIM/Camera` should
    ///    produce ONE `list_directory` call, not 135 `get_metadata` calls each of which lists the
    ///    parent).
    /// 2. For each unique parent, ask `try_get_authoritative_listing(volume_id, parent)` first. On hit,
    ///    every child entry's size + `is_directory` comes from the cached `FileEntry`, no MTP I/O.
    ///    On miss, fall through to the existing single `list_directory(parent)` per group.
    ///
    /// The oracle decision is per-parent: different parents in the same call
    /// can resolve different ways (one watched, one cold). On oracle hit no
    /// `list_directory_with_progress` callbacks fire for that parent, so the
    /// FE's scan-preview counter doesn't tick for those entries; the final
    /// `BatchScanResult.aggregate` still reflects them.
    pub(super) fn scan_for_copy_batch_with_boundary_impl<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // ❗ Progress goes down to `list_directory` verbatim
            // (`ScanBoundary::raw_progress` says why a backend must pick ONE
            // reporter), so this body counts into its own `aggregate` and asks the
            // boundary only for the stop.
            let on_progress = boundary.raw_progress();
            let stop = boundary.stop();
            if paths.is_empty() {
                return Ok(BatchScanResult {
                    aggregate: CopyScanResult {
                        file_count: 0,
                        dir_count: 0,
                        total_bytes: 0,
                        dedup_bytes: 0,
                        top_level_is_directory: false,
                    },
                    per_path: Vec::new(),
                });
            }

            // Group paths by parent. Two keys per group:
            //   - `original_parent`: the path the FE/cache uses as a listing key (typically
            //     `/DCIM/Camera`-style absolute). This is what the oracle is looked up against.
            //   - `mtp_parent`: the MTP-relative form used by `list_directory` on the cold-cache fallthrough.
            //     Stored so we don't call `to_mtp_path` twice per group.
            // DEFAULT-OK: a group with no children and no resolved paths is what it is
            // for the one statement between its creation and its first insert.
            #[derive(Default)]
            struct ParentGroup<'p> {
                original_parent: PathBuf,
                mtp_parent: String,
                children: Vec<&'p PathBuf>,
            }
            let mut groups: std::collections::HashMap<PathBuf, ParentGroup<'a>> = std::collections::HashMap::new();
            for path in paths {
                let original_parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
                let entry = groups.entry(original_parent.clone()).or_insert_with(|| {
                    let mtp_path = self.to_mtp_path(path);
                    let mtp_path_buf = PathBuf::from(&mtp_path);
                    let mtp_parent = mtp_path_buf
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ParentGroup {
                        original_parent,
                        mtp_parent,
                        children: Vec::new(),
                    }
                });
                entry.children.push(path);
            }

            debug!(
                "MtpVolume::scan_for_copy_batch: {} paths across {} unique parent dirs",
                paths.len(),
                groups.len()
            );

            // Stage per-path results in a map so the final per_path vec
            // preserves the caller's input order.
            let mut per_path_results: std::collections::HashMap<PathBuf, CopyScanResult> =
                std::collections::HashMap::with_capacity(paths.len());

            let mut aggregate = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                // MTP has no hardlinks: source footprint == write footprint.
                dedup_bytes: 0,
                // Aggregate over multiple paths: not meaningful for a batch.
                top_level_is_directory: false,
            };

            for group in groups.values() {
                // Before the parent listing: on a cold cache that call is the
                // ~17 s of USB round trips this whole backend is shaped around,
                // and a boundary on the far side of it is a Cancel the person
                // waits out.
                boundary.check().await?;

                // Oracle short-circuit: if the parent is watcher-fresh, use
                // the cached listing instead of touching the device. The
                // freshness contract for MTP is volume-level: when this
                // returns `Some`, the device is connected and would forward
                // any change events it sends.
                let cached = try_get_authoritative_listing(&self.volume_id, &group.original_parent);

                // List the parent directory once on cold cache (goes through
                // the listing cache). The MTP listing is what dominates
                // wall-clock on a cold cache (17 s for 1047 entries via USB),
                // so forward `on_progress` to `list_directory_with_progress`
                // (via the trait method) so the scan-preview dialog sees a
                // climbing file count instead of a frozen 0/0/0 spinner. On
                // an oracle hit there's no list, so no progress ticks fire
                // for this parent's children — the final aggregate still
                // includes them.
                let entries = match cached {
                    Some(entries) => {
                        debug!(
                            "MtpVolume::scan_for_copy_batch: oracle hit for parent {} ({} cached entries, {} selected children)",
                            group.original_parent.display(),
                            entries.len(),
                            group.children.len()
                        );
                        entries
                    }
                    None => self.list_directory(Path::new(&group.mtp_parent), on_progress).await?,
                };

                // Index entries by name so each child lookup is O(1). A naive
                // `entries.iter().find(...)` per child is O(n) and the outer
                // loop is also O(n), so 15k photos in /DCIM/Camera turned a
                // single parent listing into ~225M string comparisons (~10 s
                // stall in the scan preview).
                let entries_by_name: std::collections::HashMap<&str, &FileEntry> =
                    entries.iter().map(|e| (e.name.as_str(), e)).collect();

                for child_path in &group.children {
                    boundary.check().await?;
                    let mtp_path = self.to_mtp_path(child_path);
                    let name = Path::new(&mtp_path).file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if let Some(entry) = entries_by_name.get(name).copied() {
                        if entry.is_directory {
                            let scan = self.scan_subtree_with_stop(child_path, &stop).await?;
                            aggregate.file_count += scan.file_count;
                            aggregate.dir_count += scan.dir_count;
                            aggregate.total_bytes += scan.total_bytes;
                            aggregate.dedup_bytes += scan.dedup_bytes;
                            per_path_results.insert((*child_path).clone(), scan);
                        } else {
                            let size = entry.size.unwrap_or(0);
                            aggregate.file_count += 1;
                            aggregate.total_bytes += size;
                            aggregate.dedup_bytes += size;
                            per_path_results.insert(
                                (*child_path).clone(),
                                CopyScanResult {
                                    file_count: 1,
                                    dir_count: 0,
                                    total_bytes: size,
                                    dedup_bytes: size,
                                    top_level_is_directory: false,
                                },
                            );
                        }
                    }
                }
            }

            let per_path = paths
                .iter()
                .filter_map(|p| per_path_results.remove(p).map(|r| (p.clone(), r)))
                .collect();

            Ok(BatchScanResult { aggregate, per_path })
        })
    }

    pub(super) fn scan_for_conflicts_impl<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // The listing is this backend's own (it goes through the listing
            // cache); the matching is the shared one, so every backend hands a
            // conflict dialog the same shape.
            let entries = match self.list_directory(dest_path, None).await {
                Ok(entries) => entries,
                // ❗ The trait's contract: a destination the paste is about to
                // create holds nothing, so nothing clashes. ❌ Not the plain
                // `NotFound` arm every other backend uses. MTP resolves a path
                // through a cache that browsing populates, so a destination
                // nobody has walked to yet fails as a generic `IoError` ("path
                // not in cache"), which is honest: it means "unknown", not
                // "absent". `get_metadata` settles the difference by listing
                // the PARENT. Only a confirmed-absent destination reads as
                // empty; anything else (it is there and the listing failed for
                // its own reason, or the parent can't be read either) stays the
                // caller's to see, so a disconnected device can't pass for an
                // empty folder.
                Err(e) => {
                    return match self.get_metadata(dest_path).await {
                        Err(VolumeError::NotFound(_)) => Ok(Vec::new()),
                        _ => Err(e),
                    };
                }
            };
            Ok(conflicts_against(source_items, &entries))
        })
    }
}
