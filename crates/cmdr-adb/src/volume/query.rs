//! Reading what's on the device without changing it.
use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{ListingProgress, VolumeError};
use tokio_util::sync::CancellationToken;

use super::AdbVolume;
use super::mapping::{stat_to_file_entry, with_link_target};
use super::paths::join_device_path;
use crate::errors::{ENOENT, volume_error_from_errno};
use crate::sync::{SyncDirEntry, SyncEntryKind, SyncSession, SyncStat};

/// How many entries go by between two progress reports.
///
/// ❗ Never quieted: the pane's "Loading N entries…" line and the listing
/// watchdog both read it, and a quarter-million-entry `/sdcard/DCIM` with no
/// report looks exactly like a device that stopped answering.
const PROGRESS_EVERY: usize = 256;

impl AdbVolume {
    /// One `LIST` walk, as the device streams it.
    ///
    /// Cancellation is checked per entry, which is free here: the token is an
    /// atomic and the entries arrive on one socket.
    pub(super) async fn list_directory_impl(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let device = self.to_device_path(path)?;
        let mut session = self.open_sync(&device).await?;

        // ❗ `LIST` answers an unreadable or missing directory with an EMPTY
        // listing rather than an error, so the directory is stat'ed first and
        // the refusal comes from that. One round trip, and the only way to tell
        // "/data on an unrooted phone" from "an empty folder".
        let top = session
            .stat(&device)
            .await
            .map_err(|e| self.inner.map_adb_error(e, &device))?;
        if !top.exists() {
            session.quit().await;
            return Err(volume_error_from_errno(top.errno.unwrap_or(ENOENT), &device));
        }
        if top.kind() != SyncEntryKind::Directory {
            let followed = if top.kind() == SyncEntryKind::Symlink {
                self.follow(&mut session, &device).await
            } else {
                None
            };
            if followed.is_none_or(|t| t.kind() != SyncEntryKind::Directory) {
                session.quit().await;
                return Err(volume_error_from_errno(ENOTDIR, &device));
            }
        }

        let mut entries: Vec<FileEntry> = Vec::new();
        let mut symlinks: Vec<usize> = Vec::new();
        let mut tally = ListingProgress::default();
        let mut cancelled = false;
        let outcome = session
            .list(&device, &mut |entry: SyncDirEntry| {
                if cancel.is_some_and(CancellationToken::is_cancelled) {
                    cancelled = true;
                    return;
                }
                let child = join_device_path(&device, &entry.name);
                let built = stat_to_file_entry(&entry.name, &child, &entry.stat);
                if built.is_symlink {
                    symlinks.push(entries.len());
                }
                if built.is_directory {
                    tally.dirs += 1;
                } else {
                    tally.files += 1;
                    tally.bytes += built.size.unwrap_or(0);
                }
                entries.push(built);
                if let Some(on_progress) = on_progress
                    && entries.len().is_multiple_of(PROGRESS_EVERY)
                {
                    on_progress(tally);
                }
            })
            .await;
        if let Err(e) = outcome {
            session.quit().await;
            return Err(self.inner.map_adb_error(e, &device));
        }
        if cancelled {
            session.quit().await;
            return Err(VolumeError::Cancelled(device));
        }

        // A link to a folder must navigate like one. One `stat` per symlink,
        // on the same socket, ❗ never per entry: a listing is mostly files.
        for index in symlinks {
            if let Some(target) = self.follow(&mut session, &entries[index].path.clone()).await {
                let entry = std::mem::replace(
                    &mut entries[index],
                    FileEntry::new(String::new(), String::new(), false, false),
                );
                let entry = with_link_target(entry, &target);
                if entry.is_directory {
                    tally.dirs += 1;
                    tally.files = tally.files.saturating_sub(1);
                }
                entries[index] = entry;
            }
        }
        session.quit().await;

        if let Some(on_progress) = on_progress {
            on_progress(tally);
        }
        Ok(entries)
    }

    /// One `stat` round trip, as a `FileEntry`.
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let device = self.to_device_path(path)?;
        let mut session = self.open_sync(&device).await?;
        let stat = session
            .stat(&device)
            .await
            .map_err(|e| self.inner.map_adb_error(e, &device))?;
        if !stat.exists() {
            session.quit().await;
            return Err(volume_error_from_errno(stat.errno.unwrap_or(ENOENT), &device));
        }
        let name = Path::new(&device)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone());
        let mut entry = stat_to_file_entry(&name, &device, &stat);
        if entry.is_symlink
            && let Some(target) = self.follow(&mut session, &device).await
        {
            entry = with_link_target(entry, &target);
        }
        session.quit().await;
        Ok(entry)
    }

    /// Whether `path` is there, as a plain yes/no. A path off this volume
    /// doesn't exist ON THIS VOLUME, which is the question asked.
    pub(super) async fn exists_impl(&self, path: &Path) -> bool {
        self.get_metadata_impl(path).await.is_ok()
    }

    /// One `stat` round trip, reduced to the directory bit.
    pub(super) async fn is_directory_impl(&self, path: &Path) -> Result<bool, VolumeError> {
        Ok(self.get_metadata_impl(path).await?.is_directory)
    }

    /// What a symlink at `device` points at, or `None` when the target is
    /// missing or the device declined to say.
    ///
    /// The sync service's `STAT` is an `lstat`, so the target is asked of the
    /// shell (`readlink -f`, in every Android `toybox`) and then stat'ed on the
    /// same sync socket. A `Symlink` answer again means `readlink` couldn't
    /// resolve a chain, which is left as a plain link rather than walked.
    async fn follow(&self, session: &mut SyncSession, device: &str) -> Option<SyncStat> {
        let outcome = crate::shell::run(&self.inner.endpoint, &self.inner.serial, &["readlink", "-f", device])
            .await
            .ok()?;
        if !outcome.succeeded() {
            return None;
        }
        let target = outcome.stdout_text();
        let target = target.trim_end_matches(['\n', '\r']);
        if target.is_empty() {
            return None;
        }
        session.stat(target).await.ok().filter(SyncStat::exists)
    }
}

/// Linux `ENOTDIR`, which `crate::errors` has no name for: the device numbers
/// it, and every Android is Linux.
const ENOTDIR: i32 = 20;
