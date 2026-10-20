//! Reading what's on the share without changing it: listings, metadata,
//! existence, and free space.
//!
//! Each one is a round trip on the browsing session, so they all share the same
//! shape: translate the path, clone the session, classify the result through
//! [`SmbVolume::handle_smb_result`]. The `Volume` methods in `volume_impl.rs`
//! are thin wrappers over these.

use super::links::stat_through_link;
use super::mapping::{directory_entry_to_file_entry, file_info_to_file_entry, fs_info_to_space_info};
use super::session::SessionLease;
use super::{SmbVolume, slow_calls};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{ListingProgress, SpaceInfo, VolumeError};
use log::{debug, trace};
use smb2::client::{Connection, Tree};
use std::path::Path;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

/// The waits between `get_metadata` attempts that answered `STATUS_DELETE_PENDING`:
/// three tries across 0.8 s. Long enough for the common case (another client's
/// handle closing right after the delete), short enough that a move's destination
/// probe never stalls noticeably when the handle stays.
const DELETE_PENDING_BACKOFF: [Duration; 2] = [Duration::from_millis(200), Duration::from_millis(600)];

/// Rate limit for the per-poll `get_space_info` debug line, keyed by share so a
/// busy share can't swallow another's first line. The POLLS are untouched; only
/// the logging is.
static SPACE_INFO_LOG: cmdr_fs::log_rollup::LogRollup = cmdr_fs::log_rollup::LogRollup::new(Duration::from_secs(60));

/// Why [`read_listing`] stopped before the end of the folder.
pub(super) enum ListingStop {
    /// The server refused, or the connection went: classify it like any other
    /// smb2 failure.
    Smb(smb2::Error),
    /// The caller's token fired between two batches.
    Cancelled,
}

/// Lists `smb_path` one server batch at a time (smb2's `DirectoryReader`), so a
/// big folder reports `on_progress` as each batch lands and a `cancel` stops it
/// between batches. Entries are mapped as they arrive, so the raw smb2 entries
/// of a 100,000-entry folder are never held at once.
///
/// A cancelled reader is closed on `runtime`, so the caller gets its answer
/// without waiting out the batch in flight and the CLOSE behind it.
pub(super) async fn read_listing(
    tree: &Tree,
    conn: Connection,
    smb_path: &str,
    display_path: &str,
    on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    cancel: Option<&CancellationToken>,
    runtime: &Handle,
) -> Result<Vec<FileEntry>, ListingStop> {
    let mut reader = tree
        .open_directory_reader(conn, smb_path)
        .await
        .map_err(ListingStop::Smb)?;
    let mut entries = Vec::new();
    let mut tally = ListingProgress::default();
    loop {
        let batch = match cancel {
            Some(token) => tokio::select! {
                biased;
                () = token.cancelled() => {
                    runtime.spawn(async move {
                        if let Err(e) = reader.close().await {
                            debug!("SmbVolume::list_directory: closing a cancelled listing: {e}");
                        }
                    });
                    return Err(ListingStop::Cancelled);
                }
                batch = reader.next_batch() => batch,
            },
            None => reader.next_batch().await,
        };
        let Some(batch) = batch.map_err(ListingStop::Smb)? else {
            return Ok(entries);
        };
        for raw in batch.iter().filter(|e| e.name != "." && e.name != "..") {
            let entry = directory_entry_to_file_entry(raw, display_path);
            if entry.is_directory {
                tally.dirs += 1;
            } else {
                tally.files += 1;
                tally.bytes += entry.size.unwrap_or(0);
            }
            entries.push(entry);
        }
        if let Some(on_progress) = on_progress {
            on_progress(tally);
        }
    }
}

impl SmbVolume {
    /// Shared async implementation of list_directory used by both the trait method
    /// and internal helpers (which need to call it without going through the trait).
    pub(super) async fn list_directory_impl(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        self.list_directory_with_cancel_impl(path, None, None).await
    }

    /// The listing, one server batch at a time ([`read_listing`]): `on_progress`
    /// ticks per batch, and `cancel` stops it between batches with
    /// `VolumeError::Cancelled`.
    pub(super) async fn list_directory_with_cancel_impl(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        let display_path = self.to_display_path(&smb_path);

        // TRACE, not DEBUG: this fires per listing for both the live pane and the index
        // scan, and was ~9% of normal file-log volume. The scan's own progress signal is
        // the throttled `network_scanner: scanning…` DEBUG heartbeat. Bump back with
        // `RUST_LOG=cmdr_smb::volume=trace` when chasing a listing bug.
        trace!(
            "SmbVolume::list_directory: share={:?}, input={:?}, smb_path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::dir_path(path),
            LogPrivate::dir_path(&smb_path)
        );

        let start = std::time::Instant::now();

        let SessionLease { tree, conn, generation } = self.clone_session().await?;
        let runtime = self.inner.host().runtime();
        let listed = read_listing(&tree, conn, &smb_path, &display_path, on_progress, cancel, &runtime).await;
        slow_calls::note(&self.inner.share_name, "list_directory", start.elapsed());
        let entries = match listed {
            Ok(entries) => entries,
            Err(ListingStop::Cancelled) => {
                return Err(VolumeError::Cancelled("Listing cancelled".into()));
            }
            Err(ListingStop::Smb(e)) => return self.handle_smb_result("list_directory", &smb_path, generation, Err(e)),
        };

        trace!(
            "SmbVolume::list_directory: completed in {:?}, {} entries",
            start.elapsed(),
            entries.len()
        );

        Ok(entries)
    }

    /// One `stat` round trip, mapped to a `FileEntry`. The share root is
    /// synthesized rather than asked for: it has no parent directory to list it
    /// out of.
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        debug!(
            "SmbVolume::get_metadata: share={:?}, input={:?}, smb_path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::path(path),
            LogPrivate::path(&smb_path)
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

        // A `STATUS_DELETE_PENDING` is waited out briefly before it's reported: a
        // delete succeeds while another open keeps the file alive, and the name
        // answers that status until the last handle closes, usually within a
        // moment. As an error it ended a move at its destination probe for a
        // state that clears on its own (cmdr-reports#17). A handle that outlives
        // the retries still surfaces, and the dialog's Retry covers the rest.
        let mut backoff = DELETE_PENDING_BACKOFF.iter();
        let info = loop {
            let start = std::time::Instant::now();
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            let r = stat_through_link(&tree, &mut conn, &smb_path).await;
            slow_calls::note(&self.inner.share_name, "get_metadata", start.elapsed());
            if let Err(e) = &r
                && e.status() == Some(smb2::types::status::NtStatus::DELETE_PENDING)
                && let Some(delay) = backoff.next()
            {
                debug!(
                    "SmbVolume::get_metadata(share={:?}): {e}; asking again in {delay:?}",
                    LogPrivate::volume(&self.inner.share_name)
                );
                tokio::time::sleep(*delay).await;
                continue;
            }
            break self.handle_smb_result("get_metadata", &smb_path, generation, r)?;
        };

        let name = Path::new(&smb_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| smb_path.clone());
        let display_path = self.to_display_path(&smb_path);

        Ok(file_info_to_file_entry(&info, name, display_path))
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

        let start = std::time::Instant::now();
        match self.clone_session().await {
            Ok(SessionLease {
                tree,
                mut conn,
                generation: _,
            }) => {
                let found = tree.stat(&mut conn, &smb_path).await.is_ok();
                slow_calls::note(&self.inner.share_name, "exists", start.elapsed());
                found
            }
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
            let start = std::time::Instant::now();
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            let r = stat_through_link(&tree, &mut conn, &smb_path).await;
            slow_calls::note(&self.inner.share_name, "is_directory", start.elapsed());
            self.handle_smb_result("is_directory", &smb_path, generation, r)?
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
                    "SmbVolume::get_space_info: share={:?} ×{} in {}s",
                    LogPrivate::volume(&self.inner.share_name),
                    batch.count,
                    batch.elapsed.as_secs()
                );
            } else {
                debug!(
                    "SmbVolume::get_space_info: share={:?}",
                    LogPrivate::volume(&self.inner.share_name)
                );
            }
        }

        let info = {
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            let r = tree.fs_info(&mut conn).await;
            // The share root: the question is about the share, not any one file.
            self.handle_smb_result("get_space_info", "", generation, r)?
        };

        Ok(fs_info_to_space_info(&info))
    }
}
