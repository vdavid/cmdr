//! `Volume::open_random_reader` for SMB: one smb2 `FileReader` held open for many
//! positioned reads, so each read is one READ round trip instead of the
//! CREATE + READ + CLOSE that `read_range` pays per call.
//!
//! The handle outlives the session it was opened on (the viewer holds it for a
//! whole session, restic for a whole browse), so a read that meets the session's
//! end reopens the file on the next one (`resume.rs`) and reads on, once smb2
//! has proved it's still the version the reader was reading. A file that changed
//! meanwhile is `SourceChanged`, from then on.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{VolumeError, VolumeRandomReader};
use log::{debug, info};
use smb2::client::stream::FileReader;

use super::dfs_links::link_open_reader;
use super::mapping::map_smb_error;
use super::resume::Reattach;
use super::session::SessionLease;
use super::state::{SessionGeneration, ends_the_session};
use super::{SmbVolume, SmbVolumeInner};

/// A held-open SMB file. Carries the share and the session its handle belongs
/// to, so an error reaches the share's error funnel like the streaming-download
/// producer's does: it outlives the call that opened it and holds no
/// `&SmbVolume`.
pub(super) struct SmbRandomReader {
    /// The handle reads go through, swapped for a reopened one after a
    /// reconnect. Behind an `Arc` so a read holds it without the lock, and a
    /// read still running on a replaced handle finishes on it.
    held: std::sync::Mutex<Held>,
    /// One reopen at a time: reads that meet the same dead handle wait here and
    /// then read through the one the first of them opened.
    reopening: tokio::sync::Mutex<()>,
    /// Set once a reopen found the file changed: every read after that is the
    /// same refusal, with nothing sent.
    changed: AtomicBool,
    share: Arc<SmbVolumeInner>,
    /// The share path, for routing a reopen through a DFS link.
    smb_path: String,
    /// The app-addressable path, the payload of a path-carrying `VolumeError`.
    display_path: String,
}

/// A handle and the main session it belongs to.
#[derive(Clone)]
struct Held {
    reader: Arc<FileReader>,
    generation: SessionGeneration,
}

impl SmbRandomReader {
    fn current(&self) -> Held {
        self.held.lock_ignore_poison().clone()
    }

    /// Reports `e` from session `generation` through the share's funnel and maps it.
    fn report(&self, generation: SessionGeneration, op: &str, e: smb2::Error) -> VolumeError {
        map_reader_error(&self.share, generation, op, &self.display_path, e)
    }

    fn refusal(&self) -> VolumeError {
        VolumeError::SourceChanged(self.display_path.clone())
    }

    /// A read on `held`, and once more on a reopened handle when `held`'s
    /// session has ended.
    async fn read_at_impl(&self, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        if self.changed.load(Ordering::Acquire) {
            return Err(self.refusal());
        }
        let held = self.current();
        let e = match held.reader.read_at(offset, len as u64).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => e,
        };
        if !ends_the_session(&e) {
            return Err(self.report(held.generation, "read_at", e));
        }
        // Whose connection went: a reader behind a DFS link owns the TARGET's,
        // and with the share's own session still up there's nothing to report;
        // the reopen dials the target again. Otherwise the loss reaches the
        // funnel first, as any call's does: it marks the session lost, which is
        // what the reopen waits out.
        if !self.share.installed_session_is_up(held.generation).await {
            self.share.note_smb_error(held.generation, &e);
        }
        let fresh = self.reopened_after(&held, e).await?;
        fresh
            .reader
            .read_at(offset, len as u64)
            .await
            .map_err(|e| self.report(fresh.generation, "read_at", e))
    }

    /// The handle to read through after `stale` lost its session: the one a
    /// concurrent read already reopened, or a new one, opened once a session
    /// newer than `stale`'s is up and proved to read the same version of the
    /// file. `lost` is what `stale` failed with, reported if no session comes
    /// back in time.
    async fn reopened_after(&self, stale: &Held, lost: smb2::Error) -> Result<Held, VolumeError> {
        let _one_at_a_time = self.reopening.lock().await;
        if self.changed.load(Ordering::Acquire) {
            return Err(self.refusal());
        }
        let now = self.current();
        if !Arc::ptr_eq(&now.reader, &stale.reader) {
            return Ok(now);
        }
        let mut tries = Reattach::new(&self.share);
        while tries.next().await.is_ok() {
            let Ok(SessionLease { tree, conn, generation }) = self.share.clone_session().await else {
                continue;
            };
            let previous = Arc::clone(&stale.reader);
            let reopened = self
                .share
                .or_through_link(
                    generation,
                    &self.smb_path,
                    tree.reopen_file_reader(conn, &stale.reader),
                    move |client, tree, _path| {
                        Box::pin(async move { client.reopen_file_reader(tree, &previous).await })
                    },
                )
                .await;
            match reopened {
                Ok(reader) => {
                    info!(
                        "SmbRandomReader: reopened after a reconnect (volume={:?}, path={:?})",
                        self.share.volume_id,
                        LogPrivate::file_path(&self.display_path)
                    );
                    let fresh = Held {
                        reader: Arc::new(reader),
                        generation,
                    };
                    *self.held.lock_ignore_poison() = fresh.clone();
                    return Ok(fresh);
                }
                // The new session went too: funnel it and wait for the next.
                Err(e) if ends_the_session(&e) => {
                    self.share.note_smb_error(generation, &e);
                }
                Err(e) => {
                    let refused = self.report(generation, "reopen", e);
                    if matches!(refused, VolumeError::SourceChanged(_)) {
                        self.changed.store(true, Ordering::Release);
                    }
                    return Err(refused);
                }
            }
        }
        // Out of tries: the loss stands as it came (the funnel already has it).
        Err(map_smb_error(lost, &self.display_path))
    }
}

/// Reports `e` from session `generation` through the share's funnel and maps it.
fn map_reader_error(
    share: &SmbVolumeInner,
    generation: SessionGeneration,
    op: &str,
    display_path: &str,
    e: smb2::Error,
) -> VolumeError {
    share.note_smb_error(generation, &e);
    debug!(
        "SmbRandomReader::{op}(volume={:?}, path={:?}): {e}",
        share.volume_id,
        LogPrivate::file_path(display_path)
    );
    map_smb_error(e, display_path)
}

impl VolumeRandomReader for SmbRandomReader {
    fn size(&self) -> u64 {
        self.current().reader.size()
    }

    fn read_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + '_>> {
        Box::pin(self.read_at_impl(offset, len))
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send>> {
        Box::pin(async move {
            let Held { reader, generation } = self.current();
            let share = Arc::clone(&self.share);
            let display_path = self.display_path.clone();
            drop(self);
            // Every read borrowed `self`, so this is the last holder.
            let Ok(reader) = Arc::try_unwrap(reader) else {
                // Unreachable as above; dropping closes it all the same.
                return Ok(());
            };
            match reader.close().await {
                Ok(()) => Ok(()),
                // A handle whose session ended (a refused reopen leaves the dead
                // one in place) has nothing left to close: the server lets go of
                // a dead session's opens itself.
                Err(e) if ends_the_session(&e) => {
                    share.note_smb_error(generation, &e);
                    Ok(())
                }
                Err(e) => Err(map_reader_error(&share, generation, "close", &display_path, e)),
            }
        })
    }
}

impl SmbVolume {
    /// The body of `Volume::open_random_reader`: one CREATE, then the handle is the
    /// caller's until it closes it.
    pub(super) async fn open_random_reader_impl(
        &self,
        path: &Path,
    ) -> Result<Box<dyn VolumeRandomReader>, VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        debug!(
            "SmbVolume::open_random_reader: share={:?}, path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::file_path(&smb_path)
        );
        let SessionLease { tree, conn, generation } = self.clone_session().await?;
        let opened = self
            .or_through_link(
                generation,
                &smb_path,
                tree.open_file_reader(conn, &smb_path),
                link_open_reader,
            )
            .await;
        let reader = self.handle_smb_result("open_random_reader", &smb_path, generation, opened)?;
        if reader.info().is_directory {
            // Close it rather than leak the handle; the caller only needs the refusal.
            let _ = reader.close().await;
            return Err(VolumeError::IsADirectory(self.to_display_path(&smb_path)));
        }
        Ok(Box::new(SmbRandomReader {
            held: std::sync::Mutex::new(Held {
                reader: Arc::new(reader),
                generation,
            }),
            reopening: tokio::sync::Mutex::new(()),
            changed: AtomicBool::new(false),
            share: Arc::clone(&self.inner),
            display_path: self.to_display_path(&smb_path),
            smb_path,
        }))
    }
}
