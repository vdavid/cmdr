//! The recursive copy-scan helper (`scan_recursive`) plus the inherent bodies
//! for the `scan_for_copy` family of `Volume` methods (`scan_for_copy_impl`,
//! `scan_for_copy_batch_impl`, `scan_for_conflicts_impl`), which the trait
//! methods in `volume_impl` delegate to.

use super::*;

impl SmbVolume {
    /// Recursively scans an SMB path, returning file/dir counts and total bytes.
    pub(super) fn scan_recursive<'a>(
        &'a self,
        smb_path: &'a str,
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
            };

            // Stat to determine if this is a file or directory
            if smb_path.is_empty() {
                // Root is always a directory, scan its contents
            } else {
                let info = {
                    let (tree, mut conn) = self.clone_session().await?;
                    let r = tree.stat(&mut conn, smb_path).await;
                    self.handle_smb_result("scan_for_copy(stat)", r)?
                };

                if !info.is_directory {
                    result.file_count = 1;
                    result.total_bytes = info.size;
                    result.dedup_bytes = info.size;
                    result.top_level_is_directory = false;
                    return Ok(result);
                }
            }

            // It's a directory: list and recurse
            result.dir_count += 1;
            let display_path = self.to_display_path(smb_path);
            let entries = self.list_directory_impl(Path::new(&display_path)).await?;

            for entry in &entries {
                let child_smb = if smb_path.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{}/{}", smb_path, entry.name)
                };

                if entry.is_directory {
                    let sub = self.scan_recursive(&child_smb).await?;
                    result.file_count += sub.file_count;
                    result.dir_count += sub.dir_count;
                    result.total_bytes += sub.total_bytes;
                    result.dedup_bytes += sub.dedup_bytes;
                } else {
                    result.file_count += 1;
                    result.total_bytes += entry.size.unwrap_or(0);
                    result.dedup_bytes += entry.size.unwrap_or(0);
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
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::scan_for_copy: share={}, path={:?}",
                self.share_name, smb_path
            );

            self.scan_recursive(&smb_path).await
        })
    }

    /// Inherent body for the `scan_for_copy_batch` trait method (thin delegator in `volume_impl`).
    pub(super) fn scan_for_copy_batch_impl<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
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
                    },
                    per_path: Vec::new(),
                });
            }
            if paths.len() == 1 {
                let smb_path = self.to_smb_path(&paths[0]);
                let scan = self.scan_recursive(&smb_path).await?;
                return Ok(BatchScanResult {
                    aggregate: scan.clone(),
                    per_path: vec![(paths[0].clone(), scan)],
                });
            }

            // Oracle short-circuit: group inputs by parent and ask
            // `try_get_watched_listing` for each unique parent. Any path whose
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
                    let entries = parent_cache
                        .entry(original_parent.clone())
                        .or_insert_with(|| try_get_watched_listing(&self.volume_id, &original_parent));

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
                        let smb_path = self.to_smb_path(path);
                        let scan = self.scan_recursive(&smb_path).await?;
                        per_path_results[idx] = Some(scan);
                    } else {
                        per_path_results[idx] = Some(CopyScanResult {
                            file_count: 1,
                            dir_count: 0,
                            total_bytes: entry.size.unwrap_or(0),
                            dedup_bytes: entry.size.unwrap_or(0),
                            top_level_is_directory: false,
                        });
                    }
                }

                if !leftover_indices.is_empty() {
                    debug!(
                        "SmbVolume::scan_for_copy_batch: share={}, oracle resolved {}/{} paths; pipelining stats for {}",
                        self.share_name,
                        paths.len() - leftover_indices.len(),
                        paths.len(),
                        leftover_indices.len()
                    );
                }
            }

            // All paths resolved via oracle: assemble the result and skip the
            // pipelined-stat machinery entirely.
            if leftover_indices.is_empty() {
                let mut aggregate = CopyScanResult {
                    file_count: 0,
                    dir_count: 0,
                    total_bytes: 0,
                    dedup_bytes: 0,
                    top_level_is_directory: false,
                };
                let mut per_path = Vec::with_capacity(paths.len());
                for (i, slot) in per_path_results.into_iter().enumerate() {
                    let scan = slot.expect("oracle path must have populated every index");
                    aggregate.file_count += scan.file_count;
                    aggregate.dir_count += scan.dir_count;
                    aggregate.total_bytes += scan.total_bytes;
                    aggregate.dedup_bytes += scan.dedup_bytes;
                    per_path.push((paths[i].clone(), scan));
                }
                return Ok(BatchScanResult { aggregate, per_path });
            }

            // Pre-compute SMB paths so the pipelined stats can borrow strings
            // that outlive the futures' lifetimes. We compute them for the
            // leftover indices only so an oracle-only path costs zero
            // `to_smb_path` calls below.
            let smb_paths: Vec<(usize, String)> = leftover_indices
                .iter()
                .map(|&idx| (idx, self.to_smb_path(&paths[idx])))
                .collect();

            debug!(
                "SmbVolume::scan_for_copy_batch: share={}, {} paths leftover for pipelined stats (oracle handled {})",
                self.share_name,
                smb_paths.len(),
                paths.len() - smb_paths.len()
            );

            // Build N pipelined stats: one cloned `Connection` per path, no
            // lock held across any stat. `Arc<Tree>` is shared cheaply. Empty
            // paths (volume root) skip the stat: the root is always a
            // directory, and they route straight into the recursion list.
            use futures_util::StreamExt;
            use futures_util::stream::FuturesUnordered;

            let tree_arc = self.tree_arc().await?;

            // Index tracks original position so results can be reassembled in input order.
            enum StatOutcome {
                Root,
                // smb2 FileInfo: carries `is_directory` and `size`.
                Entry(smb2::client::tree::FileInfo),
            }

            type StatFuture = Pin<Box<dyn Future<Output = (usize, Result<StatOutcome, smb2::Error>)> + Send>>;
            let mut stat_futs: FuturesUnordered<StatFuture> = FuturesUnordered::new();

            for (idx, smb_path) in &smb_paths {
                let idx = *idx;
                if smb_path.is_empty() {
                    // Root: no stat needed. Inline a ready future so the
                    // ordering logic below still sees a slot for this index.
                    stat_futs.push(Box::pin(std::future::ready((idx, Ok(StatOutcome::Root)))));
                    continue;
                }
                // Briefly lock client to clone a Connection per path, then
                // release. All clones multiplex over the single SMB session.
                let conn = {
                    let mut guard = self.client.lock().await;
                    let client = guard
                        .as_mut()
                        .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?;
                    client.connection_mut().clone()
                };
                let tree = Arc::clone(&tree_arc);
                let path_owned = smb_path.clone();
                stat_futs.push(Box::pin(async move {
                    let mut conn = conn;
                    let r = tree.stat(&mut conn, &path_owned).await;
                    (idx, r.map(StatOutcome::Entry))
                }));
            }

            // `per_path_results` is already shaped to the input length and
            // pre-populated with oracle-resolved entries; the pipelined-stat
            // path below only fills the still-None slots.
            // Indices to recurse into after the stat batch finishes.
            let mut dirs_to_recurse: Vec<usize> = Vec::new();

            while let Some((idx, result)) = stat_futs.next().await {
                match result {
                    Ok(StatOutcome::Root) => {
                        // Root path → always a directory, recurse later.
                        dirs_to_recurse.push(idx);
                    }
                    Ok(StatOutcome::Entry(info)) => {
                        if info.is_directory {
                            dirs_to_recurse.push(idx);
                        } else {
                            per_path_results[idx] = Some(CopyScanResult {
                                file_count: 1,
                                dir_count: 0,
                                total_bytes: info.size,
                                dedup_bytes: info.size,
                                top_level_is_directory: false,
                            });
                        }
                    }
                    Err(e) => {
                        // Mirror handle_smb_result for the state transition on
                        // connection loss, then map and propagate.
                        let kind = e.kind();
                        if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                            warn!(
                                "SmbVolume::scan_for_copy_batch(share={}): connection lost ({}), transitioning to Disconnected",
                                self.share_name, e
                            );
                            self.transition_to_disconnected();
                        } else {
                            warn!("SmbVolume::scan_for_copy_batch(share={}): {}", self.share_name, e);
                        }
                        return Err(map_smb_error(e));
                    }
                }
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
                let scan = self.scan_recursive(smb_path).await?;
                per_path_results[idx] = Some(scan);
            }

            // Fold per-path into aggregate + per_path vec (in input order).
            let mut aggregate = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                dedup_bytes: 0,
                top_level_is_directory: false,
            };
            let mut per_path = Vec::with_capacity(paths.len());
            for (i, slot) in per_path_results.into_iter().enumerate() {
                let scan = slot.expect("every input path must have a result by this point");
                aggregate.file_count += scan.file_count;
                aggregate.dir_count += scan.dir_count;
                aggregate.total_bytes += scan.total_bytes;
                aggregate.dedup_bytes += scan.dedup_bytes;
                per_path.push((paths[i].clone(), scan));
            }

            Ok(BatchScanResult { aggregate, per_path })
        })
    }

    /// Inherent body for the `scan_for_conflicts` trait method (thin delegator in `volume_impl`).
    pub(super) fn scan_for_conflicts_impl<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // List destination directory to check for conflicts
            let entries = self.list_directory_impl(dest_path).await?;
            let mut conflicts = Vec::new();

            for item in source_items {
                if let Some(existing) = entries.iter().find(|e| e.name == item.name) {
                    let dest_modified = existing.modified_at.map(|s| s as i64);
                    conflicts.push(ScanConflict {
                        source_path: item.name.clone(),
                        dest_path: existing.path.clone(),
                        source_size: item.size,
                        dest_size: existing.size.unwrap_or(0),
                        source_modified: item.modified,
                        dest_modified,
                        source_is_directory: item.is_directory,
                        dest_is_directory: existing.is_directory,
                    });
                }
            }

            Ok(conflicts)
        })
    }
}
