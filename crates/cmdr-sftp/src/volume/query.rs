//! Reading what's on the server without changing it.
use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{ListingProgress, VolumeError};
use openssh_sftp_client::fs::DirEntry;
use tokio_util::sync::CancellationToken;

use super::SftpVolume;
use super::mapping::metadata_to_file_entry;
use crate::errors::map_sftp_error;

// ⚠️ **A filename that isn't UTF-8 costs the SESSION, not just the listing.**
// SFTP v3 filenames are BYTES with no declared encoding, and a Linux server may
// hand back any sequence. `openssh-sftp-client` deserializes names through a
// strict `ssh_format`, and it does so INSIDE its own read task, which then
// exits — so every later request on that session answers
// `BackgroundTaskFailure`, which `map_sftp_error` reports as a lost connection.
//
// Still the right failure to have: the alternative crate substitutes U+FFFD, and
// a name that addresses nothing gets written at a copy's destination. The fix,
// when it's worth it, is the byte-backed `NameEntry::filename` the vendoring
// escape hatch buys (`openssh-sftp-client` 0.15.7, measured against
// `sftp-fixture-oddnames`, 2026-08-22).

impl SftpVolume {
    /// One `readdir` walk, in the batches the server sends them.
    ///
    /// ❗ Cancellation is checked between BATCHES rather than per entry: a
    /// directory over a 50 ms link is many round trips, and the token is what
    /// lets a pane the user navigated away from stop paying for them.
    pub(super) async fn list_directory_impl(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        use futures_util::StreamExt;

        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        let dir = session
            .sftp()
            .fs()
            .open_dir(&remote)
            .await
            .map_err(|e| map_sftp_error(&e, &remote))?;

        let mut entries = Vec::new();
        let mut tally = ListingProgress::default();
        // `ReadDir` holds a cancellation future, so it isn't `Unpin`.
        let mut stream = std::pin::pin!(dir.read_dir());
        while let Some(next) = stream.next().await {
            if cancel.is_some_and(CancellationToken::is_cancelled) {
                return Err(VolumeError::Cancelled(remote));
            }
            // ⚠️ A non-UTF-8 filename fails the WHOLE readdir here, by design in
            // `openssh-sftp-client`: it deserializes names through `ssh_format`,
            // which is strict. Loud and lossless beats the alternative, where a
            // folder copy writes files under names that address nothing.
            let entry = next.map_err(|e| map_sftp_error(&e, &remote))?;
            let Some(built) = file_entry(&entry, &remote) else {
                continue;
            };
            if built.is_directory {
                tally.dirs += 1;
            } else {
                tally.files += 1;
                tally.bytes += built.size.unwrap_or(0);
            }
            entries.push(built);
        }

        // One report for the whole listing, ❌ never one per entry: the seam is a
        // trait object and a quarter-million-entry directory would pay for every
        // one of them.
        if let Some(on_progress) = on_progress {
            on_progress(tally);
        }
        Ok(entries)
    }

    /// One `stat` round trip, as a `FileEntry`.
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        let meta = session
            .sftp()
            .fs()
            .metadata(&remote)
            .await
            .map_err(|e| map_sftp_error(&e, &remote))?;
        let name = Path::new(&remote)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone());
        Ok(metadata_to_file_entry(&name, &remote, &meta))
    }

    /// Whether `path` is there, as a plain yes/no.
    pub(super) async fn exists_impl(&self, path: &Path) -> bool {
        // A path off this volume doesn't exist ON THIS VOLUME, which is the
        // question asked.
        self.get_metadata_impl(path).await.is_ok()
    }
}

/// One directory entry as a `FileEntry`, or `None` for the two the protocol
/// includes and a pane never shows.
fn file_entry(entry: &DirEntry, parent: &str) -> Option<FileEntry> {
    let name = entry.filename().to_string_lossy().into_owned();
    if name == "." || name == ".." {
        return None;
    }
    let remote_path = if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    };
    Some(metadata_to_file_entry(&name, &remote_path, &entry.metadata()))
}
