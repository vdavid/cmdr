//! [`OutageVolume`]'s `impl Volume` and `impl ScanSource`: where the knobs
//! `outage_volume_test_support.rs` arms fire. Every call checks the session
//! first ([`OutageVolume::gate`]), and a drop armed at a call goes off as that
//! call starts, or around it for a rename or a delete.

use super::{Fut, OutageVolume, RenameDrop};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataReport};
use cmdr_fs::volume::{
    BackendKind, BatchScanResult, ConnectionState, CopyScanResult, DirectoryCreation, LaneKey, ListingProgress,
    ScanBoundary, ScanConflict, ScanSource, SourceItemInfo, SpaceInfo, StreamLength, StreamWriteProgress,
    StreamWritten, Volume, VolumeError, VolumeReadStream, WriteMode, scan_walk,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts itself into its counter when dropped: held across an await that never
/// resolves, it counts the futures that were abandoned there.
struct CountOnDrop<'a>(&'a AtomicUsize);

impl Drop for CountOnDrop<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl OutageVolume {
    fn gate(&self) -> Result<(), VolumeError> {
        if self.up.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(VolumeError::DeviceDisconnected("the session is down".to_owned().into()))
        }
    }

    /// Runs `call` (a delete or a rename) under whichever path-armed drop it
    /// meets, each firing once: the plain drop on its side, or the late error
    /// that arrives after someone else reconnected.
    async fn under_path_drops<'a>(
        &'a self,
        path: &Path,
        drop: &Mutex<Option<(PathBuf, RenameDrop)>>,
        stale: Option<&Mutex<Option<(PathBuf, RenameDrop)>>>,
        call: impl Fn() -> Fut<'a, Result<(), VolumeError>>,
    ) -> Result<(), VolumeError> {
        match armed_for(drop, path) {
            Some(RenameDrop::BeforeTheServerSeesIt) => self.go_away(),
            Some(RenameDrop::AfterTheServerPerformsIt) => {
                self.gate()?;
                call().await?;
                self.go_away();
            }
            None => {}
        }
        if let Some(side) = stale.and_then(|stale| armed_for(stale, path)) {
            self.gate()?;
            if side == RenameDrop::AfterTheServerPerformsIt {
                call().await?;
            }
            self.go_away();
            self.reconnected_elsewhere();
            return Err(VolumeError::DeviceDisconnected("the session is down".to_owned().into()));
        }
        self.gate()?;
        call().await
    }
}

/// Takes the arm in `slot` when it names `path`.
fn armed_for(slot: &Mutex<Option<(PathBuf, RenameDrop)>>, path: &Path) -> Option<RenameDrop> {
    let mut armed = slot.lock_ignore_poison();
    match &*armed {
        Some((at, _)) if at == path => armed.take().map(|(_, side)| side),
        _ => None,
    }
}

impl ScanSource for OutageVolume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> scan_walk::Walking<'a, FileEntry> {
        self.get_metadata(path)
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> scan_walk::Walking<'a, Vec<FileEntry>> {
        self.list_directory(path, None)
    }
}

impl Volume for OutageVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(self.lane.clone())
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_concurrent_ops(&self) -> usize {
        self.inner.max_concurrent_ops()
    }
    fn connection_generation(&self) -> Option<u64> {
        Some(self.generation())
    }
    fn backend_kind(&self) -> BackendKind {
        *self.backend.lock_ignore_poison()
    }
    fn connection_state(&self) -> Option<ConnectionState> {
        Some(
            if self.needs_sign_in.load(Ordering::SeqCst) && !self.up.load(Ordering::SeqCst) {
                ConnectionState::NeedsSignIn
            } else if self.up.load(Ordering::SeqCst) {
                ConnectionState::Direct
            } else {
                ConnectionState::Disconnected
            },
        )
    }
    fn attempt_reconnect<'a>(&'a self) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.reconnect_attempts.fetch_add(1, Ordering::SeqCst);
            if self.hang_reconnects.load(Ordering::SeqCst) {
                let _abandoned = CountOnDrop(&self.abandoned_reconnects);
                std::future::pending::<()>().await;
            }
            if self.needs_sign_in.load(Ordering::SeqCst) {
                return Err(VolumeError::PermissionDenied {
                    path: "/".to_owned(),
                    raw_os_error: None,
                });
            }
            if self.reachable.load(Ordering::SeqCst) {
                self.bring_the_session_back();
                Ok(())
            } else {
                Err(VolumeError::ConnectionTimeout(
                    "the server didn't answer".to_owned().into(),
                ))
            }
        })
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        Box::pin(async move {
            self.listings.lock_ignore_poison().push(path.to_path_buf());
            let armed_here = {
                let mut armed = self.drop_at_listing_of.lock_ignore_poison();
                armed.as_deref() == Some(path) && armed.take().is_some()
            };
            if armed_here {
                self.go_away();
            }
            self.gate()?;
            if self.refuse_listing_of.lock_ignore_poison().as_deref() == Some(path) {
                let path = path.display().to_string();
                return Err(VolumeError::PermissionDenied {
                    path,
                    raw_os_error: None,
                });
            }
            self.inner.list_directory(path, on_progress).await
        })
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        Box::pin(async move {
            let armed_here = {
                let mut armed = self.drop_at_stat_of.lock_ignore_poison();
                armed.as_deref() == Some(path) && armed.take().is_some()
            };
            if armed_here {
                self.go_away();
            }
            self.gate()?;
            self.inner.get_metadata(path).await
        })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Fut<'a, bool> {
        Box::pin(async move { self.gate().is_ok() && self.inner.exists(path).await })
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<bool, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.is_directory(path).await
        })
    }
    fn create_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_directory(path).await
        })
    }
    fn create_directory_all<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<DirectoryCreation, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_directory_all(path).await
        })
    }
    fn create_file<'a>(&'a self, path: &'a Path, content: &'a [u8]) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_file(path, content).await
        })
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            let refused_here = self.up.load(Ordering::SeqCst) && {
                let mut armed = self.refuse_deletes_of.lock_ignore_poison();
                match armed.as_mut() {
                    Some((held, left)) if held == path && *left > 0 => {
                        *left -= 1;
                        true
                    }
                    _ => false,
                }
            };
            if refused_here {
                return Err(VolumeError::IoError {
                    message: "sharing violation".into(),
                    raw_os_error: None,
                });
            }
            self.under_path_drops(
                path,
                &self.drop_at_delete_of,
                Some(&self.stale_error_at_delete_of),
                || self.inner.delete(path),
            )
            .await
        })
    }
    fn rename<'a>(&'a self, from: &'a Path, to: &'a Path, force: bool) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            let n = self.renames.fetch_add(1, Ordering::SeqCst);
            let armed = *self.drop_at_rename.lock_ignore_poison();
            match armed {
                Some((at, RenameDrop::BeforeTheServerSeesIt)) if at == n => self.go_away(),
                Some((at, RenameDrop::AfterTheServerPerformsIt)) if at == n => {
                    self.gate()?;
                    self.inner.rename(from, to, force).await?;
                    self.go_away();
                }
                _ => {}
            }
            let stale = *self.stale_error_at_rename.lock_ignore_poison();
            if let Some((at, side)) = stale
                && at == n
            {
                self.gate()?;
                if side == RenameDrop::AfterTheServerPerformsIt {
                    self.inner.rename(from, to, force).await?;
                }
                self.go_away();
                self.reconnected_elsewhere();
                return Err(VolumeError::DeviceDisconnected("the session is down".to_owned().into()));
            }
            if let Some(side) = armed_for(&self.drop_at_rename_of, from) {
                if side == RenameDrop::AfterTheServerPerformsIt {
                    self.gate()?;
                    self.inner.rename(from, to, force).await?;
                }
                self.go_away();
                self.gate()?;
            }
            if let Some(side) = armed_for(&self.stale_error_at_rename_of, from) {
                self.gate()?;
                if side == RenameDrop::AfterTheServerPerformsIt {
                    self.inner.rename(from, to, force).await?;
                }
                self.go_away();
                self.reconnected_elsewhere();
                return Err(VolumeError::DeviceDisconnected("the session is down".to_owned().into()));
            }
            self.under_path_drops(to, &self.drop_at_rename_onto, None, || {
                self.inner.rename(from, to, force)
            })
            .await
        })
    }
    fn supports_atomic_replace_rename(&self) -> bool {
        self.replaces_atomically.load(Ordering::SeqCst)
    }
    fn supports_copy_within(&self) -> bool {
        self.copies_on_server.load(Ordering::SeqCst)
    }
    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Fut<'a, Result<u64, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.server_copies.fetch_add(1, Ordering::SeqCst);
            let stream = self.inner.open_read_stream(from).await?;
            let size = stream.total_size().known().expect("stored files have a known length");
            self.inner
                .write_from_stream(
                    to,
                    WriteMode::CreateOrReplace,
                    StreamLength::Known(size),
                    stream,
                    &|progress| on_progress(progress.bytes_written, size),
                )
                .await
                .map(|written| written.bytes)
        })
    }
    fn get_space_info<'a>(&'a self) -> Fut<'a, Result<SpaceInfo, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.get_space_info().await
        })
    }
    fn open_read_stream<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<Box<dyn VolumeReadStream>, VolumeError>> {
        Box::pin(async move {
            let n = self.opens.fetch_add(1, Ordering::SeqCst);
            if *self.drop_at_open.lock_ignore_poison() == Some(n) {
                self.go_away();
            }
            self.gate()?;
            self.inner.open_read_stream(path).await
        })
    }
    fn scan_for_copy<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<CopyScanResult, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.scan_for_copy(path).await
        })
    }
    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Fut<'a, Result<BatchScanResult, VolumeError>> {
        // The shared walk over this volume's own stat and listing, so a drop
        // armed at a listing lands inside the walk, as on a real share.
        scan_walk::scan_trees(self, paths, boundary)
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Fut<'a, Result<Vec<ScanConflict>, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.scan_for_conflicts(source_items, dest_path).await
        })
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Fut<'a, Result<StreamWritten, VolumeError>> {
        Box::pin(async move {
            let n = self.writes.fetch_add(1, Ordering::SeqCst);
            self.written.lock_ignore_poison().push(dest.to_path_buf());
            if *self.drop_at_write.lock_ignore_poison() == Some(n) {
                self.go_away();
            }
            self.gate()?;
            let written = self
                .inner
                .write_from_stream(dest, mode, length, stream, on_progress)
                .await?;
            if *self.drop_after_write.lock_ignore_poison() == Some(n) {
                if self.drop_cuts_the_stamp.load(Ordering::SeqCst) {
                    let servers_own = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_secs());
                    self.inner.set_modified_at(dest, Some(servers_own));
                }
                self.go_away();
                return Err(VolumeError::DeviceDisconnected("the session is down".to_owned().into()));
            }
            Ok(written)
        })
    }
    fn write_is_single_shot<'a>(&'a self, _length: StreamLength) -> Fut<'a, bool> {
        Box::pin(async move { self.single_shot.load(Ordering::SeqCst) })
    }
    fn apply_metadata<'a>(
        &'a self,
        path: &'a Path,
        carried: &'a CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Fut<'a, MetadataReport> {
        Box::pin(async move {
            let armed_here = {
                let mut armed = self.drop_at_apply_metadata_of.lock_ignore_poison();
                armed.as_deref() == Some(path) && armed.take().is_some()
            };
            if armed_here {
                self.go_away();
            }
            let refused_here = self.refuse_metadata_of.lock_ignore_poison().as_deref() == Some(path);
            if self.gate().is_err() || refused_here {
                return MetadataReport::uniform(carried, modified, ClassOutcome::Refused);
            }
            self.inner.apply_metadata(path, carried, modified).await
        })
    }
}
