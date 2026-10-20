//! The recursive copy-scan helper (`scan_recursive`) plus the inherent bodies
//! for the `scan_for_copy` family of `Volume` methods (`scan_for_copy_impl`,
//! `scan_for_copy_batch_impl`, `scan_for_conflicts_impl`), which the trait
//! methods in `volume_impl` delegate to.

use super::SmbVolume;
use super::links::stat_through_link;
use super::mapping::{filetime_to_unix_secs, map_smb_error};
use super::session::SessionLease;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::scan_walk::{conflicts_against, fold_batch};
use cmdr_fs::volume::{BatchScanResult, CopyScanResult, ScanBoundary, ScanConflict, SourceItemInfo, VolumeError};
use log::{debug, warn};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

/// What one top-level path of a batch scan turned out to be.
enum StatOutcome {
    /// The volume root: always a directory, never stat'ed.
    Root,
    /// smb2 FileInfo: carries `is_directory` and `size`.
    Entry(smb2::client::tree::FileInfo),
}

impl SmbVolume {
    /// Recursively scans an SMB path, returning file/dir counts and total bytes.
    ///
    /// ❗ Every entry passes `boundary`, and a directory passes it BEFORE its
    /// listing goes out. A share that has spun its disks down answers a listing in
    /// seconds, so a boundary on the far side of one is a Cancel the user waits
    /// out. Every read (the stat, the listing) goes through
    /// `boundary.through_outages`, so a share that drops mid-walk is waited out
    /// by the scan's owner and the walk carries on at the level it was on.
    ///
    /// `caller_path` is `smb_path` as the CALLER spelled it: every entry reaches
    /// the boundary under it (`ScanRecorder`'s path contract), since the
    /// share-relative form is this backend's own.
    pub(super) fn scan_recursive<'a>(
        &'a self,
        smb_path: &'a str,
        caller_path: &'a Path,
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut result = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                // SMB exposes no hardlinks, so the source footprint always
                // equals the write footprint. Kept in lockstep with
                // `total_bytes` at every accumulation site below.
                dedup_bytes: 0,
                // Root path is always a directory; the file branch below
                // overwrites this to `false`. Subdirectory recursions also
                // return `true`; only the leaf file branch sets `false`.
                top_level_is_directory: true,
                // The stat below fills it; the share root has no stat and no
                // date to keep.
                top_level_modified_at: None,
                top_level_created_at: None,
            };

            // Before the stat: a cancel that landed while the previous subtree
            // was walking shouldn't buy one more round trip.
            boundary.check().await?;

            // Stat to determine if this is a file or directory
            if smb_path.is_empty() {
                // Root is always a directory, scan its contents
            } else {
                // The lease is inside the read: a share that dropped fails it
                // too, and a run after the wait takes the new session's.
                let info = boundary
                    .through_outages(caller_path, || async {
                        let SessionLease {
                            tree,
                            mut conn,
                            generation,
                        } = self.clone_session().await?;
                        let r = stat_through_link(&tree, &mut conn, smb_path).await;
                        self.handle_smb_result("scan_for_copy(stat)", smb_path, generation, r)
                    })
                    .await?;
                result.top_level_modified_at = filetime_to_unix_secs(info.modified);
                result.top_level_created_at = filetime_to_unix_secs(info.created);

                if !info.is_directory {
                    result.file_count = 1;
                    result.total_bytes = info.size;
                    result.dedup_bytes = info.size;
                    result.top_level_is_directory = false;
                    boundary.file(caller_path, info.size).await?;
                    return Ok(result);
                }
            }

            // It's a directory: list and recurse
            result.dir_count += 1;
            boundary.dir(caller_path).await?;
            let display_path = self.to_display_path(smb_path);
            // A listing a dropped share cut off runs again once the scan's
            // owner has waited it out; nothing below counts until it answers.
            let entries = boundary
                .through_outages(caller_path, || self.list_directory_impl(Path::new(&display_path)))
                .await?;

            for entry in &entries {
                let child_smb = if smb_path.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{}/{}", smb_path, entry.name)
                };

                let child_caller = caller_path.join(&entry.name);
                if entry.is_directory {
                    let sub = self.scan_recursive(&child_smb, &child_caller, boundary).await?;
                    result.file_count += sub.file_count;
                    result.dir_count += sub.dir_count;
                    result.total_bytes += sub.total_bytes;
                    result.dedup_bytes += sub.dedup_bytes;
                } else {
                    result.file_count += 1;
                    result.total_bytes += entry.size.unwrap_or(0);
                    result.dedup_bytes += entry.size.unwrap_or(0);
                    boundary.file(&child_caller, entry.size.unwrap_or(0)).await?;
                }
            }

            Ok(result)
        })
    }

    /// Inherent body for the `scan_for_copy` trait method (thin delegator in `volume_impl`).
    pub(super) fn scan_for_copy_impl<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path)?;

            debug!(
                "SmbVolume::scan_for_copy: share={:?}, path={:?}",
                LogPrivate::volume(&self.inner.share_name),
                LogPrivate::path(&smb_path)
            );

            // Nothing to report to and nobody to answer to: the single-path
            // trait method hands this body neither. The batch method below is the
            // one the scan preview drives and the one a person can cancel.
            let boundary = ScanBoundary::silent();
            self.scan_recursive(&smb_path, path, &boundary).await
        })
    }

    /// Inherent body for the `scan_for_copy_batch` trait method (thin delegator in `volume_impl`).
    pub(super) fn scan_for_copy_batch_impl<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // One boundary for the whole call: every branch below feeds it, so the
            // counts the caller sees climb across oracle hits, pipelined stats,
            // and recursion alike, and every branch answers the same Cancel.
            // Fast paths: empty / single. Empty returns zeroes; single falls
            // through to the recursive scanner so we don't pay the cost of the
            // batch machinery for one path.
            if paths.is_empty() {
                return Ok(BatchScanResult {
                    aggregate: CopyScanResult {
                        file_count: 0,
                        dir_count: 0,
                        total_bytes: 0,
                        dedup_bytes: 0,
                        top_level_is_directory: false,
                        top_level_modified_at: None,
                        top_level_created_at: None,
                    },
                    per_path: Vec::new(),
                    files: None,
                });
            }
            if paths.len() == 1 {
                let smb_path = self.to_smb_path(&paths[0])?;
                let scan = self.scan_recursive(&smb_path, &paths[0], boundary).await?;
                return Ok(BatchScanResult {
                    aggregate: scan.clone(),
                    per_path: vec![(paths[0].clone(), scan)],
                    files: None,
                });
            }

            // Oracle short-circuit: group inputs by parent and ask the host's
            // `authoritative_listing` for each unique parent. Any path whose
            // parent is watcher-backed gets its size + is_directory from the
            // cached `FileEntry` (no SMB stat). Remaining paths fall through
            // to the pipelined-stat flow below. Decision is per-parent: one
            // call can mix oracle-served paths with pipelined-stat paths.
            //
            // SMB stats are per-path (not per-parent listing), so the grouping
            // here is purely about oracle eligibility; the fallthrough path
            // doesn't need parent grouping itself.
            let mut per_path_results: Vec<Option<CopyScanResult>> = (0..paths.len()).map(|_| None).collect();
            let mut leftover_indices: Vec<usize> = Vec::with_capacity(paths.len());
            {
                use std::collections::HashMap;
                // Cache oracle lookups so two paths sharing a parent only pay
                // one cache scan + clone. Value: indexed-by-name view over the
                // cached entries, or None if the oracle missed for this parent.
                let mut parent_cache: HashMap<PathBuf, Option<Vec<FileEntry>>> = HashMap::new();
                for (idx, path) in paths.iter().enumerate() {
                    let original_parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
                    let entries = parent_cache.entry(original_parent.clone()).or_insert_with(|| {
                        self.inner
                            .host()
                            .listings()
                            .authoritative_listing(&self.inner.volume_id, &original_parent)
                    });

                    let Some(cached_entries) = entries.as_ref() else {
                        leftover_indices.push(idx);
                        continue;
                    };

                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        leftover_indices.push(idx);
                        continue;
                    };

                    let Some(entry) = cached_entries.iter().find(|e| e.name == name) else {
                        // Cache doesn't know this child (stale selection,
                        // encoding mismatch). Fall through to a real stat for
                        // safety rather than reporting it as missing.
                        leftover_indices.push(idx);
                        continue;
                    };

                    if entry.is_directory {
                        // Directories still need a recursive scan to count
                        // descendants. The oracle just told us "this is a
                        // dir without an SMB stat"; recurse to expand it.
                        let smb_path = self.to_smb_path(path)?;
                        let scan = self.scan_recursive(&smb_path, path, boundary).await?;
                        per_path_results[idx] = Some(scan);
                    } else {
                        per_path_results[idx] = Some(CopyScanResult {
                            file_count: 1,
                            dir_count: 0,
                            total_bytes: entry.size.unwrap_or(0),
                            dedup_bytes: entry.size.unwrap_or(0),
                            top_level_is_directory: false,
                            top_level_modified_at: entry.modified_at,
                            top_level_created_at: entry.created_at,
                        });
                        boundary.file(path, entry.size.unwrap_or(0)).await?;
                    }
                }

                if !leftover_indices.is_empty() {
                    debug!(
                        "SmbVolume::scan_for_copy_batch: share={:?}, oracle resolved {}/{} paths; pipelining stats for {}",
                        LogPrivate::volume(&self.inner.share_name),
                        paths.len() - leftover_indices.len(),
                        paths.len(),
                        leftover_indices.len()
                    );
                }
            }

            // All paths resolved via oracle: assemble the result and skip the
            // pipelined-stat machinery entirely.
            if leftover_indices.is_empty() {
                let per_path = per_path_results
                    .into_iter()
                    .enumerate()
                    .map(|(i, slot)| {
                        (
                            paths[i].clone(),
                            slot.expect("oracle path must have populated every index"),
                        )
                    })
                    .collect();
                return Ok(fold_batch(per_path));
            }

            // Pre-compute SMB paths so the pipelined stats can borrow strings
            // that outlive the futures' lifetimes. We compute them for the
            // leftover indices only so an oracle-only path costs zero
            // `to_smb_path` calls below.
            let smb_paths: Vec<(usize, String)> = leftover_indices
                .iter()
                .map(|&idx| Ok((idx, self.to_smb_path(&paths[idx])?)))
                .collect::<Result<_, VolumeError>>()?;

            debug!(
                "SmbVolume::scan_for_copy_batch: share={:?}, {} paths leftover for pipelined stats (oracle handled {})",
                LogPrivate::volume(&self.inner.share_name),
                smb_paths.len(),
                paths.len() - smb_paths.len()
            );

            // N pipelined stats (`stat_leftovers`), one read to the scan's
            // owner: a drop fails it
            // as a whole, nothing in it is counted until it's all in, and a
            // run after the wait sends every stat again on the new session.
            let first_leftover = &paths[smb_paths[0].0];
            let outcomes = boundary
                .through_outages(first_leftover, || self.stat_leftovers(&smb_paths))
                .await?;

            // `per_path_results` is already shaped to the input length and
            // pre-populated with oracle-resolved entries; the pipelined-stat
            // path below only fills the still-None slots.
            // Indices to recurse into after the stat batch finishes.
            let mut dirs_to_recurse: Vec<usize> = Vec::new();
            // Sizes the stat batch resolved, reported to the boundary once the
            // drain is over. ❗ Not inside the loop: parking there would hold
            // every cloned connection in the batch idle for the length of a
            // human pause. The drain is O(top-level paths) and pipelined, so the
            // wait it can add is bounded; the recursion after it is the long part
            // and asks per entry.
            let mut files_from_stats: Vec<(usize, u64)> = Vec::new();

            for (idx, outcome) in outcomes {
                match outcome {
                    StatOutcome::Root => {
                        // Root path → always a directory, recurse later.
                        dirs_to_recurse.push(idx);
                    }
                    StatOutcome::Entry(info) => {
                        if info.is_directory {
                            dirs_to_recurse.push(idx);
                        } else {
                            per_path_results[idx] = Some(CopyScanResult {
                                file_count: 1,
                                dir_count: 0,
                                total_bytes: info.size,
                                dedup_bytes: info.size,
                                top_level_is_directory: false,
                                top_level_modified_at: filetime_to_unix_secs(info.modified),
                                top_level_created_at: filetime_to_unix_secs(info.created),
                            });
                            files_from_stats.push((idx, info.size));
                        }
                    }
                }
            }

            for (idx, size) in files_from_stats {
                boundary.file(&paths[idx], size).await?;
            }

            // Recurse sequentially into each discovered directory. Per-dir
            // recursion still serializes on listing + child stats; that's a
            // future "Fix 5" (pipelined directory recursion). For the 100 ×
            // tiny-file scenario all sources are files, so this loop is never
            // entered.
            // `smb_paths` is `Vec<(idx, String)>` keyed by the leftover index;
            // build a lookup so dir-recursion can find each path by its
            // original input index.
            let smb_path_by_idx: std::collections::HashMap<usize, &str> =
                smb_paths.iter().map(|(i, s)| (*i, s.as_str())).collect();
            for idx in dirs_to_recurse {
                let smb_path = smb_path_by_idx
                    .get(&idx)
                    .expect("dirs_to_recurse only carries indices from the leftover stat batch");
                let scan = self.scan_recursive(smb_path, &paths[idx], boundary).await?;
                per_path_results[idx] = Some(scan);
            }

            // Fold per-path into aggregate + per_path vec (in input order).
            let per_path = per_path_results
                .into_iter()
                .enumerate()
                .map(|(i, slot)| {
                    (
                        paths[i].clone(),
                        slot.expect("every input path must have a result by this point"),
                    )
                })
                .collect();

            Ok(fold_batch(per_path))
        })
    }

    /// Stats every leftover top-level path of a batch scan at once: one
    /// `Connection` clone per path, all off one `clone_session`, so every stat
    /// runs on the session its tree belongs to and no lock is held across any
    /// of them. An empty path (the volume root) skips the stat: the root is
    /// always a directory.
    ///
    /// Answers each path's outcome by its index in the batch, or the first
    /// stat's failure. Counts nothing, so the batch can send it again after a
    /// wait (`ScanBoundary::through_outages`).
    async fn stat_leftovers(&self, smb_paths: &[(usize, String)]) -> Result<Vec<(usize, StatOutcome)>, VolumeError> {
        use futures_util::StreamExt;
        use futures_util::stream::FuturesUnordered;

        let SessionLease {
            tree: tree_arc,
            conn: session_conn,
            generation,
        } = self.clone_session().await?;

        // The path rides along with the result: a refusal has to name the path
        // it was about, and `map_smb_error` puts that name in the payload the
        // frontend renders.
        type StatFuture = Pin<Box<dyn Future<Output = (usize, String, Result<StatOutcome, smb2::Error>)> + Send>>;
        let mut stat_futs: FuturesUnordered<StatFuture> = FuturesUnordered::new();

        for (idx, smb_path) in smb_paths {
            let idx = *idx;
            if smb_path.is_empty() {
                stat_futs.push(Box::pin(std::future::ready((
                    idx,
                    String::new(),
                    Ok(StatOutcome::Root),
                ))));
                continue;
            }
            // All clones multiplex over the single SMB session.
            let conn = session_conn.clone();
            let tree = Arc::clone(&tree_arc);
            let path_owned = smb_path.clone();
            stat_futs.push(Box::pin(async move {
                let mut conn = conn;
                let r = stat_through_link(&tree, &mut conn, &path_owned).await;
                (idx, path_owned, r.map(StatOutcome::Entry))
            }));
        }

        let mut outcomes = Vec::with_capacity(smb_paths.len());
        while let Some((idx, smb_path, result)) = stat_futs.next().await {
            match result {
                Ok(outcome) => outcomes.push((idx, outcome)),
                Err(e) => {
                    // Every stat ran on the lease's session, so the error
                    // funnel judges it against that one.
                    let kind = e.kind();
                    let ends = self.inner.note_smb_error(generation, &e);
                    warn!(
                        "SmbVolume::scan_for_copy_batch(share={:?}): backend=smb2, error_kind={:?}, nt_status={:?}, detail={:?}, ends_the_session={ends}",
                        LogPrivate::volume(&self.inner.share_name),
                        kind,
                        e.status(),
                        cmdr_fs::log_detail::LogDetail(&e.to_string())
                    );
                    return Err(map_smb_error(e, &self.to_display_path(&smb_path)));
                }
            }
        }
        Ok(outcomes)
    }

    /// Inherent body for the `scan_for_conflicts` trait method (thin delegator in `volume_impl`).
    pub(super) fn scan_for_conflicts_impl<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // The listing is this backend's own (cache-aware); the matching is
            // the shared one, so every backend hands a conflict dialog the same
            // shape.
            let entries = match self.list_directory_impl(dest_path).await {
                Ok(entries) => entries,
                // ❗ The trait's contract: a destination the paste is about to
                // create holds nothing, so nothing clashes. Reporting the
                // server's `STATUS_OBJECT_NAME_NOT_FOUND` instead would refuse
                // the whole copy preview.
                Err(VolumeError::NotFound(_)) => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            Ok(conflicts_against(source_items, &entries))
        })
    }
}
