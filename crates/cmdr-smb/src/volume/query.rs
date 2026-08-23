//! Reading what's on the share without changing it: listings, metadata,
//! existence, and free space.
//!
//! Each one is a round trip on the browsing session, so they all share the same
//! shape: translate the path, clone the session, classify the result through
//! [`SmbVolume::handle_smb_result`]. The `Volume` methods in `volume_impl.rs`
//! are thin wrappers over these.

use super::SmbVolume;
use super::mapping::{directory_entry_to_file_entry, filetime_to_unix_secs, fs_info_to_space_info};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{ListingProgress, SpaceInfo, VolumeError};
use log::{debug, trace};
use std::path::Path;
use std::time::Duration;

/// Rate limit for the per-poll `get_space_info` debug line, keyed by share so a
/// busy share can't swallow another's first line. The POLLS are untouched; only
/// the logging is.
static SPACE_INFO_LOG: cmdr_fs::log_rollup::LogRollup = cmdr_fs::log_rollup::LogRollup::new(Duration::from_secs(60));

impl SmbVolume {
    /// Shared async implementation of list_directory used by both the trait method
    /// and internal helpers (which need to call it without going through the trait).
    pub(super) async fn list_directory_impl(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        let display_path = self.to_display_path(&smb_path);

        // TRACE, not DEBUG: this fires per listing for both the live pane and the index
        // scan, and was ~9% of normal file-log volume. The scan's own progress signal is
        // the throttled `network_scanner: scanning…` DEBUG heartbeat. Bump back with
        // `RUST_LOG=cmdr_lib::file_system::volume::backends::smb=trace` when chasing a listing bug.
        trace!(
            "SmbVolume::list_directory: share={}, input={:?}, smb_path={:?}",
            self.inner.share_name, path, smb_path
        );

        let start = std::time::Instant::now();

        let result = {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.list_directory(&mut conn, &smb_path).await;
            self.handle_smb_result("list_directory", &smb_path, r)?
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

    /// `list_directory_impl` plus the one-shot progress report the scan dialog needs.
    pub(super) async fn list_directory_with_progress_impl(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let entries = self.list_directory_impl(path).await?;
        // smb2's list_directory returns all entries at once, so report
        // progress as a single batch after the call completes. Tally files
        // / dirs / bytes from the returned entries so the FE scan dialog
        // doesn't see "0 bytes, 0 dirs" climbing on Direct SMB scans.
        if let Some(on_progress) = on_progress {
            let mut tally = ListingProgress::default();
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
    }

    /// One `stat` round trip, mapped to a `FileEntry`. The share root is
    /// synthesized rather than asked for: it has no parent directory to list it
    /// out of.
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        debug!(
            "SmbVolume::get_metadata: share={}, input={:?}, smb_path={:?}",
            self.inner.share_name, path, smb_path
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
            self.handle_smb_result("get_metadata", &smb_path, r)?
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
    }

    /// Whether `path` is there, as a plain yes/no: anything that can't be asked
    /// (a path off this share, a session that won't clone) is a no.
    pub(super) async fn exists_impl(&self, path: &Path) -> bool {
        // A path outside this share doesn't exist ON THIS VOLUME, which is
        // exactly the question asked.
        let Ok(smb_path) = self.to_smb_path(path) else {
            return false;
        };
        if smb_path.is_empty() {
            return true; // Root always exists if we're connected
        }

        match self.clone_session().await {
            Ok((tree, mut conn)) => tree.stat(&mut conn, &smb_path).await.is_ok(),
            Err(_) => false,
        }
    }

    /// One `stat` round trip, reduced to the directory bit.
    pub(super) async fn is_directory_impl(&self, path: &Path) -> Result<bool, VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        if smb_path.is_empty() {
            return Ok(true); // Root is always a directory
        }

        let info = {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.stat(&mut conn, &smb_path).await;
            self.handle_smb_result("is_directory", &smb_path, r)?
        };

        Ok(info.is_directory)
    }

    /// The share's own free/total, polled by whichever pane is showing it.
    pub(super) async fn get_space_info_impl(&self) -> Result<SpaceInfo, VolumeError> {
        // Polled every 5 s per share for as long as a pane shows it (~480 lines
        // an hour), and each one says only "we asked again". Rolled up per share
        // so the bundle still shows the polls flowing, and at what rate, without
        // one line per round trip. A failure is never rolled up: it goes through
        // `handle_smb_result` below.
        if let Some(batch) = SPACE_INFO_LOG.record(&self.inner.share_name) {
            if batch.is_rolled_up() {
                debug!(
                    "SmbVolume::get_space_info: share={} ×{} in {}s",
                    self.inner.share_name,
                    batch.count,
                    batch.elapsed.as_secs()
                );
            } else {
                debug!("SmbVolume::get_space_info: share={}", self.inner.share_name);
            }
        }

        let info = {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.fs_info(&mut conn).await;
            // The share root: the question is about the share, not any one file.
            self.handle_smb_result("get_space_info", "", r)?
        };

        Ok(fs_info_to_space_info(&info))
    }
}
