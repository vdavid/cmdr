//! Path translation helpers and the `impl Volume for SmbVolume` block
//! (identity, capabilities, query, mutation, scan, streaming, space, and
//! reconnect trait methods). A trait impl can't be split across files, so all
//! `Volume` methods live here; they lean on inherent helpers in the sibling
//! concern modules (`session::clone_session`, `streams::open_smb_download_stream`,
//! `scan::scan_recursive`, `reconnect::do_attempt_reconnect`, etc.).

use super::*;

impl SmbVolume {
    /// Converts a volume-relative path to the SMB relative path string.
    ///
    /// The frontend sends paths relative to the volume root (which is the mount path).
    /// smb2 expects paths relative to the share root with `/` separators.
    /// NFC-normalizes the result because macOS sends NFD (decomposed) paths
    /// but SMB servers expect NFC (composed). Without this, paths with accented
    /// characters (like "ä") fail with STATUS_OBJECT_PATH_NOT_FOUND.
    pub(super) fn to_smb_path(&self, path: &Path) -> String {
        use unicode_normalization::UnicodeNormalization;

        let path_str = path.to_string_lossy();

        // Handle paths that start with the mount path (absolute paths from frontend)
        if let Some(relative) = path_str.strip_prefix(self.mount_path.to_string_lossy().as_ref()) {
            let trimmed = relative.trim_start_matches('/');
            return trimmed.nfc().collect();
        }

        // Handle empty or root paths
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return String::new();
        }

        // Strip leading slash for absolute paths
        let raw = path_str.strip_prefix('/').unwrap_or(&path_str);
        raw.nfc().collect()
    }

    /// Returns the full absolute path for a relative SMB path (under mount point).
    pub(super) fn to_display_path(&self, smb_path: &str) -> String {
        if smb_path.is_empty() {
            self.mount_path.to_string_lossy().to_string()
        } else {
            format!("{}/{}", self.mount_path.display(), smb_path)
        }
    }

    /// Shared async implementation of list_directory used by both the trait method
    /// and internal helpers (which need to call it without going through the trait).
    pub(super) async fn list_directory_impl(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let smb_path = self.to_smb_path(path);
        let display_path = self.to_display_path(&smb_path);

        // TRACE, not DEBUG: this fires per listing for both the live pane and the index
        // scan, and was ~9% of normal file-log volume. The scan's own progress signal is
        // the throttled `network_scanner: scanning…` DEBUG heartbeat. Bump back with
        // `RUST_LOG=cmdr_lib::file_system::volume::backends::smb=trace` when chasing a listing bug.
        trace!(
            "SmbVolume::list_directory: share={}, input={:?}, smb_path={:?}",
            self.share_name, path, smb_path
        );

        let start = std::time::Instant::now();

        let result = {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.list_directory(&mut conn, &smb_path).await;
            self.handle_smb_result("list_directory", r)?
        };

        let entries: Vec<FileEntry> = result
            .iter()
            .filter(|e| e.name != "." && e.name != "..")
            .map(|e| directory_entry_to_file_entry(e, &display_path))
            .collect();

        trace!(
            "SmbVolume::list_directory: completed in {:?}, {} entries",
            start.elapsed(),
            entries.len()
        );

        Ok(entries)
    }
}

impl Volume for SmbVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.mount_path
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn lane_key(&self) -> LaneKey {
        // Serialize transfers that hit the same share over one session.
        // `volume_id` already encodes `server+port+share` (via
        // `smb_volume_id`), exactly the server+share granularity we want.
        LaneKey::new(self.volume_id.clone())
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let entries = self.list_directory_impl(path).await?;
            // smb2's list_directory returns all entries at once, so report
            // progress as a single batch after the call completes. Tally files
            // / dirs / bytes from the returned entries so the FE scan dialog
            // doesn't see "0 bytes, 0 dirs" climbing on Direct SMB scans.
            if let Some(on_progress) = on_progress {
                let mut tally = crate::file_system::volume::ListingProgress::default();
                for e in &entries {
                    if e.is_directory {
                        tally.dirs += 1;
                    } else {
                        tally.files += 1;
                        tally.bytes += e.size.unwrap_or(0);
                    }
                }
                on_progress(tally);
            }
            Ok(entries)
        })
    }

    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        _cancel: Option<&'a Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        // SMB ignores the cancel flag (there's no mid-listing SMB cancel today; the
        // scanner's `LIST_TIMEOUT` on a detached task handles a wedged listing) —
        // same as the default `list_directory_with_cancel` this used to fall through
        // to. The override exists to draw from the per-scan connection pool when one
        // is active; see `scan_pool.rs`.
        Box::pin(async move { self.list_directory_for_scan_impl(path).await })
    }

    fn begin_scan_session<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Refcounted: concurrent background users (an index rescan overlapping a
            // media enrichment pass) share ONE pool; `open_scan_pool` is idempotent.
            self.scan_session_refs.fetch_add(1, Ordering::AcqRel);
            self.open_scan_pool().await
        })
    }

    fn end_scan_session<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Saturating decrement: an unmatched end (a pass racing unmount
            // teardown) must not underflow into a never-closing pool.
            let prev = self
                .scan_session_refs
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| Some(n.saturating_sub(1)))
                .unwrap_or(0);
            // Close only when the LAST session ends; an earlier end while a sibling
            // still scans would tear the pool out from under it mid-flight.
            if prev <= 1 {
                self.close_scan_pool().await;
            }
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::get_metadata: share={}, input={:?}, smb_path={:?}",
                self.share_name, path, smb_path
            );

            // For root, synthesize a directory entry
            if smb_path.is_empty() {
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.mount_path.to_string_lossy().to_string(),
                    true,
                    false,
                ));
            }

            let info = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.stat(&mut conn, &smb_path).await;
                self.handle_smb_result("get_metadata", r)?
            };

            let name = Path::new(&smb_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| smb_path.clone());
            let display_path = self.to_display_path(&smb_path);

            let mut fe = FileEntry::new(name, display_path, info.is_directory, false);
            fe.size = if info.is_directory { None } else { Some(info.size) };
            fe.modified_at = filetime_to_unix_secs(info.modified);
            fe.created_at = filetime_to_unix_secs(info.created);
            Ok(fe)
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);
            if smb_path.is_empty() {
                return true; // Root always exists if we're connected
            }

            {
                match self.clone_session().await {
                    Ok((tree, mut conn)) => tree.stat(&mut conn, &smb_path).await.is_ok(),
                    Err(_) => false,
                }
            }
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);
            if smb_path.is_empty() {
                return Ok(true); // Root is always a directory
            }

            let info = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.stat(&mut conn, &smb_path).await;
                self.handle_smb_result("is_directory", r)?
            };

            Ok(info.is_directory)
        })
    }

    fn supports_watching(&self) -> bool {
        // Starts as false: the existing FSEvents watcher on the OS mount
        // point already provides change notifications. smb2-native watching
        // can be added later as an optimization.
        false
    }

    fn supports_local_fs_access(&self) -> bool {
        // SmbVolume handles listing notifications via notify_mutation,
        // so the old std::fs-based synthetic diff path is not needed.
        false
    }

    fn supports_foreground_yield(&self) -> bool {
        // A running copy and the pane's listings share ONE SMB session, so a
        // transfer off this share competes with every navigation on it. Opting in
        // tells `CheckpointStream` not to start the next chunk while the user is
        // browsing this share. The read holds nothing between chunks, so this is a
        // park in place, not a session release. See `foreground_yield.rs`.
        true
    }

    fn supports_foreground_yield_as_destination(&self) -> bool {
        // An UPLOAD to this share (local → SMB) writes in discrete SMB2 WRITE
        // chunks and holds only a file handle between them, with NO oplock or
        // lease requested (`create_file_writer` → `OplockLevel::None`, no durable
        // context; see `smb/streams.rs`). So a running upload can stand aside for
        // the user browsing the same share between chunks. `CheckpointStream` caps
        // each such park so the open write handle never sits idle long enough for
        // the server to reap it. Contrast the read side (`supports_foreground_yield`):
        // both share this volume's per-share `foreground_pending` probe.
        true
    }

    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { foreground_yield::foreground_pending(&self.volume_id) })
    }

    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move { foreground_yield::wait_until_foreground_idle(&self.volume_id).await })
    }

    fn listing_is_watched(&self, _path: &Path) -> bool {
        // SMB watching is volume-level: the smb_watcher monitors the whole share
        // via CHANGE_NOTIFY. So once the watcher is alive and the session is
        // Direct, every cached listing on this volume is oracle-eligible.
        // `watcher_cancel` is a std `Mutex` (not async): use `try_lock` and treat
        // contention as "not watched" to keep the oracle out of the lock-wait path.
        // The oracle will simply fall through to a real read; that's the safe
        // direction. Don't hold the lock across awaits (we never `.await` here
        // anyway: this is a sync method).
        let has_watcher = match self.watcher_cancel.try_lock() {
            Ok(guard) => guard.is_some(),
            Err(_) => return false,
        };
        has_watcher && self.connection_state() == ConnectionState::Direct
    }

    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

            match mutation {
                MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                    let entry_path = parent_path.join(name);
                    match self.get_metadata(&entry_path).await {
                        Ok(entry) => {
                            let change = if matches!(mutation, MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(&self.volume_id, parent_path, change);
                        }
                        Err(e) => {
                            warn!(
                                "SmbVolume::notify_mutation: couldn't stat {}: {}",
                                entry_path.display(),
                                e
                            );
                        }
                    }
                }
                MutationEvent::Deleted(name) => {
                    notify_directory_changed(&self.volume_id, parent_path, DirectoryChange::Removed(name));
                }
                MutationEvent::Renamed { from, to } => {
                    let new_path = parent_path.join(&to);
                    match self.get_metadata(&new_path).await {
                        Ok(entry) => {
                            notify_directory_changed(
                                &self.volume_id,
                                parent_path,
                                DirectoryChange::Renamed {
                                    old_name: from,
                                    new_entry: entry,
                                },
                            );
                        }
                        Err(e) => {
                            warn!(
                                "SmbVolume::notify_mutation: couldn't stat renamed entry {}: {}",
                                new_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        })
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            debug!("SmbVolume::get_space_info: share={}", self.share_name);

            let info = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.fs_info(&mut conn).await;
                self.handle_smb_result("get_space_info", r)?
            };

            Ok(fs_info_to_space_info(&info))
        })
    }

    fn space_poll_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(5))
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);
            let data = content.to_vec();

            debug!("SmbVolume::create_file: share={}, path={:?}", self.share_name, smb_path);

            {
                let (tree, conn) = self.clone_session().await?;
                // No-clobber contract via the exclusive-create writer
                // (`FileCreate` disposition): if the file already exists the
                // server returns `STATUS_OBJECT_NAME_COLLISION`, which the
                // smb2 crate maps to `ErrorKind::AlreadyExists`. The earlier
                // stat-then-write workaround left a microsecond TOCTOU
                // window; this closes it atomically at the protocol layer.
                let writer_result = tree.create_file_writer_exclusive(conn, &smb_path).await;
                let mut writer = self.handle_smb_result("create_file(open)", writer_result)?;
                if !data.is_empty() {
                    let write_result = writer.write_chunk(&data).await;
                    self.handle_smb_result("create_file(write_chunk)", write_result)?;
                }
                let finish_result = writer.finish().await;
                self.handle_smb_result("create_file(finish)", finish_result)?;
            }

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    MutationEvent::Created(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::create_directory: share={}, path={:?}",
                self.share_name, smb_path
            );

            {
                let (tree, mut conn) = self.clone_session().await?;
                let result = tree.create_directory(&mut conn, &smb_path).await;
                self.handle_smb_result("create_directory", result)?;
            }

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    MutationEvent::Created(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!("SmbVolume::delete: share={}, path={:?}", self.share_name, smb_path);

            // Try delete_file first (one round-trip). If the path is a directory,
            // the server returns STATUS_FILE_IS_A_DIRECTORY; then try delete_directory.
            // This avoids a stat round-trip for every file in bulk deletes.
            let file_result = {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.delete_file(&mut conn, &smb_path).await;
                self.handle_smb_result("delete_file", r)
            };

            match file_result {
                Ok(()) => {} // File deleted successfully
                Err(VolumeError::IsADirectory(_)) => {
                    // Expected fall-through: path is a directory, retry with delete_directory.
                    let (tree, mut conn) = self.clone_session().await?;
                    let r = tree.delete_directory(&mut conn, &smb_path).await;
                    self.handle_smb_result("delete_directory", r)?;
                }
                Err(e) => return Err(e),
            }

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    MutationEvent::Deleted(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_from = self.to_smb_path(from);
            let smb_to = self.to_smb_path(to);

            debug!(
                "SmbVolume::rename: share={}, from={:?}, to={:?}, force={}",
                self.share_name, smb_from, smb_to, force
            );

            if force {
                // Check if dest exists and delete it first
                let dest_exists = {
                    let (tree, mut conn) = self.clone_session().await?;
                    tree.stat(&mut conn, &smb_to).await.is_ok()
                };

                if dest_exists {
                    // Try file delete first; if it fails specifically because the path is a
                    // directory, try directory delete. Any other error (PermissionDenied,
                    // SharingViolation, …) propagates immediately instead of being masked
                    // by a second futile delete.
                    let file_result = {
                        let (tree, mut conn) = self.clone_session().await?;
                        let r = tree.delete_file(&mut conn, &smb_to).await;
                        self.handle_smb_result("rename(delete_dest_file)", r)
                    };
                    match file_result {
                        Ok(()) => {}
                        Err(VolumeError::IsADirectory(_)) => {
                            // Expected fall-through: dest is a directory, retry with delete_directory.
                            // Any other error (PermissionDenied, SharingViolation, …) propagates immediately
                            // instead of being masked by a second futile delete.
                            let (tree, mut conn) = self.clone_session().await?;
                            let r = tree.delete_directory(&mut conn, &smb_to).await;
                            self.handle_smb_result("rename(delete_dest_dir)", r)?;
                        }
                        Err(e) => return Err(e),
                    }
                }
            } else {
                // Check if dest exists and return AlreadyExists if so
                let dest_exists = {
                    let (tree, mut conn) = self.clone_session().await?;
                    tree.stat(&mut conn, &smb_to).await.is_ok()
                };
                if dest_exists {
                    return Err(VolumeError::AlreadyExists(to.display().to_string()));
                }
            }

            {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.rename(&mut conn, &smb_from, &smb_to).await;
                self.handle_smb_result("rename", r)?;
            }

            // Notify listing cache about the rename
            if let (Some(from_parent), Some(from_name)) = (from.parent(), from.file_name()) {
                let to_name = to
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let from_parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(from_parent)));

                if from.parent() == to.parent() {
                    // Same-directory rename
                    self.notify_mutation(
                        &self.volume_id,
                        &from_parent_display,
                        MutationEvent::Renamed {
                            from: from_name.to_string_lossy().to_string(),
                            to: to_name,
                        },
                    )
                    .await;
                } else {
                    // Cross-directory move: remove from source, add in dest
                    self.notify_mutation(
                        &self.volume_id,
                        &from_parent_display,
                        MutationEvent::Deleted(from_name.to_string_lossy().to_string()),
                    )
                    .await;
                    if let Some(to_parent) = to.parent() {
                        let to_parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(to_parent)));
                        self.notify_mutation(&self.volume_id, &to_parent_display, MutationEvent::Created(to_name))
                            .await;
                    }
                }
            }
            Ok(())
        })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_impl(path)
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_impl(paths)
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.scan_for_conflicts_impl(source_items, dest_path)
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // Reads the `network.smbConcurrency` setting (default 10, clamped 1..=32).
        // Updated at app startup from `settings.json` via
        // `file_system::set_smb_concurrency`. Lock-free atomic load on every
        // call, so a settings change in the current session applies on the next
        // batch-copy dispatch (no reconnect required; Connection::clone is
        // cheap).
        crate::file_system::smb_concurrency()
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            debug!(
                "SmbVolume::open_read_stream: share={}, path={:?}",
                self.share_name, smb_path
            );

            let stream = self.open_smb_download_stream(&smb_path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path);

            // Compound fast-path: if the caller-provided hint fits in one READ,
            // send CREATE+READ+CLOSE as a single compound frame (1 RTT) instead
            // of the 3-RTT streaming open. Drives the compound on a cloned
            // `Connection` with no lock held, so N concurrent small reads
            // pipeline over one SMB session. Falls through to the streaming
            // path when the hint is missing or too large, or when the file
            // changed size since the scan: shrunk files come back short
            // (`data.len() != size`), grown-past-`max_read` files come back as
            // a typed `ErrorKind::TooLarge` (smb2 refuses to truncate a file
            // that no longer fits in one READ).
            if let Some(size) = size_hint {
                let (tree, mut conn) = self.clone_session().await?;
                let max_read = conn.params().map(|p| p.max_read_size).unwrap_or(65536) as u64;
                if size > 0 && size <= max_read {
                    debug!(
                        "SmbVolume::open_read_stream_with_hint: share={}, path={:?}, size={}; using compound fast-path",
                        self.share_name, smb_path, size
                    );
                    match tree.read_file_compound(&mut conn, &smb_path).await {
                        Err(e) if matches!(e.kind(), smb2::ErrorKind::TooLarge) => {
                            debug!(
                                "SmbVolume::open_read_stream_with_hint: file grew past max_read since the scan ({}); falling back to streaming",
                                e
                            );
                        }
                        read_result => {
                            let data = self.handle_smb_result("open_read_stream_with_hint(compound)", read_result)?;
                            if data.len() as u64 == size {
                                return Ok(Box::new(InlineReadStream::new(data)) as Box<dyn VolumeReadStream>);
                            }
                            debug!(
                                "SmbVolume::open_read_stream_with_hint: compound read returned {} bytes, expected {}; falling back to streaming",
                                data.len(),
                                size
                            );
                        }
                    }
                }
            }

            debug!(
                "SmbVolume::open_read_stream_with_hint: share={}, path={:?}; using streaming path",
                self.share_name, smb_path
            );
            let stream = self.open_smb_download_stream(&smb_path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    fn open_read_stream_for_scan<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        // Background bulk reads (media enrichment prefetch) draw small hinted files
        // from the scan-connection pool when one is active; see `scan_pool.rs`.
        Box::pin(async move { self.open_read_stream_for_scan_impl(path, size_hint).await })
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if len == 0 {
                return Ok(Vec::new());
            }
            let smb_path = self.to_smb_path(path);
            debug!(
                "SmbVolume::read_range: share={}, path={:?}, offset={}, len={}",
                self.share_name, smb_path, offset, len
            );

            // One open -> one positioned read -> close per call. `smb2::FileReader`
            // itself serves many `read_at`s per open, but `Volume::read_range` is
            // stateless (no handle persists across calls), so opening per call is
            // the simple, correct shape for now: a remote-zip browse issues only a
            // handful of ranged reads (the `TailCachedSource` collapses the
            // central-directory parse to ~1). Caching an open `FileReader` per path
            // is the future optimization; see the archive backend DETAILS.
            let (tree, conn) = self.clone_session().await?;
            let reader = self.handle_smb_result("read_range(open)", tree.open_file_reader(conn, &smb_path).await)?;

            let read_result = reader.read_at(offset, len as u64).await;
            // Close the handle regardless of the read outcome. Relying on `Drop`
            // would only log and leak the handle until session teardown, so we
            // close explicitly on both the success and error paths.
            let close_result = reader.close().await;

            let data = self.handle_smb_result("read_range", read_result)?;
            self.handle_smb_result("read_range(close)", close_result)?;
            Ok(data)
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.write_from_stream_impl(dest, size, stream, on_progress)
    }

    fn smb_connection_state(&self) -> Option<SmbConnectionState> {
        // SmbVolume always returns `Some` so the frontend can distinguish
        // "not an SMB volume" (None) from "SMB volume in trouble"
        // (Some(Disconnected)). The reconnect manager keys off the latter.
        // The internal state machine is binary; the outer `OsMount` variant
        // is only attached by `enrich_smb_connection_state` for SMB shares
        // that have an OS mount but no Cmdr smb2 session at all.
        Some(match self.connection_state() {
            ConnectionState::Direct => SmbConnectionState::Direct,
            ConnectionState::Disconnected => SmbConnectionState::Disconnected,
        })
    }

    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.do_attempt_reconnect())
    }

    fn reconnect_with_credentials<'a>(
        &'a self,
        username: String,
        password: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.do_reconnect_with_credentials(username, password))
    }

    fn on_unmount(&self) {
        // Mark the volume permanently dead so any in-flight reconnect bails
        // out before installing a session into an orphaned volume.
        self.unmounted.store(true, Ordering::Relaxed);

        // Transition to Disconnected. We deliberately set the atomic directly
        // instead of going through `transition_to_disconnected()`, because the
        // volume is being unregistered: the FE will learn via `volumes-changed`
        // and an extra `smb-connection-changed` event would race with that.
        self.state.store(ConnectionState::Disconnected as u8, Ordering::Relaxed);

        // Cancel the background watcher task. The task will call watcher.close()
        // to release the SMB directory handle before exiting.
        if let Ok(mut guard) = self.watcher_cancel.lock()
            && let Some(cancel_tx) = guard.take()
        {
            let _ = cancel_tx.send(());
            debug!("SmbVolume cleanup for {}: watcher cancel sent", self.share_name);
        }

        // Tear down any live scan pool: a member session must not keep walking an
        // unmounted volume. Sync (no runtime here): flip its `closed` flag so
        // reconnect loops bail and drop this reference; the member sessions close
        // when the last `Arc` drops (within one backoff step). See `scan_pool.rs`.
        self.close_scan_pool_sync();

        // Drop the smb2 session. Uses blocking_lock() / blocking_write() since
        // on_unmount is sync (called from FSEvents thread, no Tokio runtime).
        // Safe because we just set state to Disconnected, so no async task
        // will acquire either lock. Drop Tree first, then SmbClient: Tree
        // holds a tree_id referenced by session-scoped server state, and we
        // want it to go first so any lingering `FileDownload` clones finish
        // before the client (which owns the Connection) vanishes. In
        // practice all three just drop their Arc refcounts; the order is
        // defensive.
        {
            let mut tree_guard = self.tree.blocking_write();
            *tree_guard = None;
        }
        {
            let mut client_guard = self.client.blocking_lock();
            *client_guard = None;
        }

        debug!("SmbVolume cleanup for {}: smb2 session dropped", self.share_name);
    }
}
