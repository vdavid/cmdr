//! Changing what's on the device: create, delete, rename, copy, and the
//! classification a failed shell verb gets.
//!
//! The sync service has no verb for any of these but "write a file", so the
//! rest is the device's shell: `mkdir -p`, `rmdir`, `rm -f`, `mv -f`, `cp -a`.
//! `shell_v2` carries the exit code, and the connect refuses a device without
//! it, so every verb here can tell success from failure.
//!
//! ## Classifying a failed verb, ❌ without reading stderr
//!
//! `toybox`'s wording is not a contract and the device may be localized. So a
//! non-zero exit is read through what the sync service says is at the path and
//! its parent ([`AdbVolume::classify_failed_verb`]): parent missing →
//! `NotFound(parent)`; parent there but not writable (`test -w`) →
//! `PermissionDenied(path)`; anything else → `IoError` carrying stderr for the
//! technical-details panel.
//!
//! ## The two accepted TOCTOU windows
//!
//! `create_file` and a `force = false` rename each `stat` the destination
//! first and refuse on a hit. Neither can be made atomic on this protocol:
//! `SEND` truncates unconditionally and `mv -n` exits 0 whether it moved or
//! not (verified on Android 14 `toybox 0.8.9`, 2026-09-01). The window is the
//! round trip between the stat and the verb, on a device only this host is
//! writing to, and `conformance_test.rs` holds the refusal itself.

use std::path::Path;

use cmdr_fs::volume::{DirectoryCreation, VolumeError};
use log::debug;

use super::AdbVolume;
use crate::errors::{ENOENT, ENOTEMPTY_DEVICE, volume_error_from_errno};
use crate::shell;
use crate::sync::SyncEntryKind;

/// What the sync service says is at a path, for the classifier and the
/// pre-flight probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WhatIsThere {
    /// A directory (or a link to one).
    Directory,
    /// Something that isn't a directory.
    NotADirectory,
    /// Nothing at all.
    Nothing,
}

impl AdbVolume {
    /// Writes `content` to a name nothing else holds, refusing an occupied one
    /// (`writes.rs` § "The two accepted TOCTOU windows").
    pub(super) async fn create_file_impl(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let device = self.to_device_path(path)?;
        debug!("AdbVolume::create_file: {device}, size={}", content.len());
        if self.probe(&device).await != WhatIsThere::Nothing {
            return Err(VolumeError::AlreadyExists(device));
        }
        let stream = Box::new(super::streams::BytesReadStream::new(content.to_vec()));
        self.write_from_stream_impl(path, content.len() as u64, stream, &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await?;
        Ok(())
    }

    /// `mkdir -p`: creates the directory and every missing ancestor, and
    /// succeeds on one that is already there. That is what lets
    /// `create_directory_errors_on_existing_dir` answer `false`.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let device = self.to_device_path(path)?;
        debug!("AdbVolume::create_directory: {device}");
        self.shell_verb(&["mkdir", "-p", &device], &device).await?;
        self.notify_created(path).await;
        Ok(())
    }

    /// The same verb, answering honestly whether the leaf was there before.
    ///
    /// ❗ `Created` is a promise the transfer driver SPENDS by skipping its
    /// per-file destination conflict probe, so the leaf is stat'ed before the
    /// `mkdir` and a hit answers `AlreadyExisted`.
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        let device = self.to_device_path(path)?;
        if device == "/" || self.probe(&device).await == WhatIsThere::Directory {
            return Ok(DirectoryCreation::AlreadyExisted);
        }
        self.shell_verb(&["mkdir", "-p", &device], &device).await?;
        self.notify_created(path).await;
        Ok(DirectoryCreation::Created)
    }

    /// Deletes one file or one EMPTY directory.
    ///
    /// ❗ Strictly one node: `rmdir` for a directory, `rm -f` for anything else,
    /// ❌ never `rm -r`. Real data-safety logic leans on the refusal: a
    /// same-volume move keeps a skipped child's only copy purely by letting its
    /// parent's delete fail, and that refusal carries `ENOTEMPTY`.
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let device = self.to_device_path(path)?;
        debug!("AdbVolume::delete: {device}");
        match self.probe(&device).await {
            WhatIsThere::Nothing => return Err(volume_error_from_errno(ENOENT, &device)),
            WhatIsThere::Directory => {
                if let Err(e) = self.shell_verb(&["rmdir", &device], &device).await {
                    // The one refusal the contract names: something is still
                    // inside. Asked of the sync service, not of stderr.
                    let held = self.list_directory_impl(path, None, None).await.map(|e| !e.is_empty());
                    if matches!(held, Ok(true)) {
                        return Err(volume_error_from_errno(ENOTEMPTY_DEVICE, &device));
                    }
                    return Err(e);
                }
            }
            WhatIsThere::NotADirectory => self.shell_verb(&["rm", "-f", &device], &device).await?,
        }
        self.notify_deleted(path).await;
        Ok(())
    }

    /// Moves an entry, clearing the destination only when the caller said it
    /// may (`writes.rs` § "The two accepted TOCTOU windows" for `force = false`).
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let device_from = self.to_device_path(from)?;
        let device_to = self.to_device_path(to)?;
        debug!("AdbVolume::rename: {device_from} → {device_to}, force={force}");
        if !force && self.probe(&device_to).await != WhatIsThere::Nothing {
            return Err(VolumeError::AlreadyExists(device_to));
        }
        if self.probe(&device_from).await == WhatIsThere::Nothing {
            return Err(volume_error_from_errno(ENOENT, &device_from));
        }
        self.shell_verb(&["mv", "-f", &device_from, &device_to], &device_to)
            .await?;
        self.notify_renamed(from, to).await;
        Ok(())
    }

    /// Copies one file on the device, without the bytes crossing USB.
    ///
    /// `cp` gives no progress, so the callback hears once, at the end, with the
    /// size the source reported. The destination is truncated if it exists,
    /// which is `write_from_stream`'s contract too.
    pub(super) async fn copy_within_impl(
        &self,
        from: &Path,
        to: &Path,
        on_progress: &(dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let device_from = self.to_device_path(from)?;
        let device_to = self.to_device_path(to)?;
        let source = self.get_metadata_impl(from).await?;
        if source.is_directory {
            return Err(VolumeError::IsADirectory(device_from));
        }
        let size = source.size.unwrap_or(0);
        self.shell_verb(&["cp", "-f", &device_from, &device_to], &device_to)
            .await?;
        if on_progress(size, size).is_break() {
            let _ = shell::run(&self.inner.endpoint, &self.inner.serial, &["rm", "-f", &device_to]).await;
            return Err(VolumeError::Cancelled(device_to));
        }
        self.notify_created(to).await;
        Ok(size)
    }

    // ── The shared pieces ────────────────────────────────────────────

    /// Runs one shell verb and classifies a non-zero exit against `path`.
    pub(super) async fn shell_verb(&self, argv: &[&str], path: &str) -> Result<(), VolumeError> {
        let outcome = shell::run(&self.inner.endpoint, &self.inner.serial, argv)
            .await
            .map_err(|e| self.inner.map_adb_error(e, path))?;
        if outcome.succeeded() {
            return Ok(());
        }
        Err(self.classify_failed_verb(path, &outcome.stderr).await)
    }

    /// The failure of a verb aimed at `path`, read through what is at the path
    /// and its parent now (`writes.rs` § "Classifying a failed verb").
    async fn classify_failed_verb(&self, path: &str, stderr: &[u8]) -> VolumeError {
        let parent = match path.rfind('/') {
            Some(0) | None => "/".to_string(),
            Some(at) => path[..at].to_string(),
        };
        if self.probe(&parent).await == WhatIsThere::Nothing {
            return VolumeError::NotFound(parent);
        }
        let writable = shell::run(&self.inner.endpoint, &self.inner.serial, &["test", "-w", &parent])
            .await
            .map(|o| o.succeeded());
        if matches!(writable, Ok(false)) {
            return VolumeError::PermissionDenied(path.to_string());
        }
        let message = String::from_utf8_lossy(stderr).trim().to_string();
        debug!("AdbVolume: a shell verb on {path} failed: {message}");
        VolumeError::IoError {
            message,
            raw_os_error: None,
        }
    }

    /// What the sync service says is at `device`. A link is followed, so a
    /// link to a folder reads as a folder.
    pub(super) async fn probe(&self, device: &str) -> WhatIsThere {
        let Ok(mut session) = self.open_sync(device).await else {
            return WhatIsThere::Nothing;
        };
        let stat = session.stat(device).await;
        session.quit().await;
        match stat {
            Ok(stat) if stat.exists() => match stat.kind() {
                SyncEntryKind::Directory => WhatIsThere::Directory,
                SyncEntryKind::Symlink => match self.get_metadata_impl(Path::new(device)).await {
                    Ok(entry) if entry.is_directory => WhatIsThere::Directory,
                    _ => WhatIsThere::NotADirectory,
                },
                _ => WhatIsThere::NotADirectory,
            },
            _ => WhatIsThere::Nothing,
        }
    }
}
