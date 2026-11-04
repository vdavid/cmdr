//! The AppleDouble `._<name>` file a share keeps beside `<name>`, and taking it
//! along when Cmdr deletes `<name>`. `DETAILS.md` § "AppleDouble sidecars".
//!
//! A QNAP keeps a file's resource fork in a visible `._<name>` (`vfs_fruit` with
//! `fruit:resource = file`), and its recycle bin turns a delete into a move that
//! leaves that file behind, through macOS's own client too. So after a delete
//! lands, the volume looks for the sidecar and deletes it, but only when its
//! first bytes are AppleDouble's magic: a person's own `._notes` stays.
//!
//! Cheap by construction: only a share with named streams, and only once this
//! share has shown a `._` name or been handed a resource fork to write
//! ([`Sidecars::seen`]). Until then a delete sends what it always did.

use super::SmbVolume;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{MutationEvent, Volume};
use log::{debug, warn};
use smb2::client::{Connection, Tree};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The first four bytes of every AppleDouble file (RFC 1740's `0x00051607`).
pub(super) const APPLE_DOUBLE_MAGIC: [u8; 4] = [0x00, 0x05, 0x16, 0x07];

/// The xattr a resource fork travels as. A share with `fruit:resource = file`
/// keeps one in a `._` file, so writing it means the share has sidecars.
const RESOURCE_FORK_XATTR: &str = "com.apple.ResourceFork";

/// What the probe reads in its one frame: a whole small sidecar (FinderInfo
/// alone is 82 bytes), one credit's worth.
const PROBE_READ_BYTES: u64 = 4096;

/// How many taken sidecars a share remembers for the delete that may still come
/// for each ([`Sidecars::take_back`]). Sidecars are rare (a file with a resource
/// fork), so this holds every one a walk takes in all but a folder of thousands
/// of forked files; past it, the oldest is forgotten and its later delete
/// answers `NotFound` as it did before sidecars went.
const TAKEN_CAP: usize = 4096;

/// Whether `name` is a sidecar's own name, which has no sidecar of its own.
pub(super) fn is_sidecar_name(name: &str) -> bool {
    name.starts_with("._")
}

/// Whether `head`, a file's first bytes, is AppleDouble.
pub(super) fn is_apple_double(head: &[u8]) -> bool {
    head.starts_with(&APPLE_DOUBLE_MAGIC)
}

/// The sidecar beside `smb_path` (`._<name>`, same folder), or `None` for a
/// path that is a sidecar itself or names nothing (the share's top).
pub(super) fn sidecar_path(smb_path: &str) -> Option<String> {
    let (folder, name) = match smb_path.rsplit_once('/') {
        Some((folder, name)) => (Some(folder), name),
        None => (None, smb_path),
    };
    if name.is_empty() || is_sidecar_name(name) {
        return None;
    }
    Some(match folder {
        Some(folder) => format!("{folder}/._{name}"),
        None => format!("._{name}"),
    })
}

/// One share's sidecar bookkeeping.
#[derive(Default)]
pub(super) struct Sidecars {
    /// Set once a listing or stat on this share named a `._` entry, or a copy
    /// wrote a resource fork here. Until then no delete probes.
    seen: AtomicBool,
    /// The sidecars a delete took, oldest first, so the delete a walk still
    /// sends for one (it listed `._a` beside `a`) answers `Ok`.
    taken: Mutex<VecDeque<String>>,
}

impl Sidecars {
    /// Notes an entry this share listed or stat'd.
    pub(super) fn note_name(&self, name: &str) {
        if is_sidecar_name(name) && !self.seen.load(Ordering::Relaxed) {
            self.seen.store(true, Ordering::Relaxed);
        }
    }

    /// Notes the xattrs a copy writes here: a resource fork may become a sidecar.
    pub(super) fn note_written_xattr(&self, name: &str) {
        if name == RESOURCE_FORK_XATTR {
            self.seen.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn seen(&self) -> bool {
        self.seen.load(Ordering::Relaxed)
    }

    fn remember_taken(&self, smb_path: &str) {
        let mut taken = self.taken.lock_ignore_poison();
        if taken.len() >= TAKEN_CAP {
            taken.pop_front();
        }
        taken.push_back(smb_path.to_string());
    }

    /// Whether a delete took the sidecar at `smb_path`, forgetting it: the
    /// caller's own `NotFound` there is then that delete's doing.
    pub(super) fn take_back(&self, smb_path: &str) -> bool {
        let mut taken = self.taken.lock_ignore_poison();
        match taken.iter().position(|path| path == smb_path) {
            Some(at) => {
                taken.remove(at);
                true
            }
            None => false,
        }
    }
}

impl SmbVolume {
    /// Deletes the AppleDouble sidecar of `smb_path`, which a delete just took,
    /// and tells the panes. Best-effort: ❌ never fails the delete it follows.
    /// Skipped, with nothing sent, on a share without named streams (there
    /// `._` files are Finder's own AppleDouble, which a copy can't carry, so a
    /// move would lose them), on one that hasn't shown a sidecar yet, and
    /// behind a DFS link. Otherwise one probe (CREATE+READ+CLOSE, `NotFound`
    /// the usual answer), then the delete when its bytes are AppleDouble.
    pub(super) async fn delete_apple_double_sidecar(&self, tree: &Arc<Tree>, conn: &Connection, smb_path: &str) {
        if !self.stores_xattrs() || !self.inner.sidecars.seen() || self.inner.link_paths.covers(smb_path) {
            return;
        }
        let Some(sidecar) = sidecar_path(smb_path) else {
            return;
        };
        match read_head(tree, conn, &sidecar).await {
            Ok(Some(head)) if is_apple_double(&head) => {}
            Ok(Some(_)) => {
                debug!(
                    "smb: path={:?} isn't AppleDouble, so it stays",
                    LogPrivate::file_path(&sidecar)
                );
                return;
            }
            Ok(None) => return,
            Err(e) => {
                debug!(
                    "smb: couldn't look at the `._` file beside path={:?}, so it stays: {e}",
                    LogPrivate::file_path(smb_path)
                );
                return;
            }
        }
        // Remembered BEFORE the delete goes out, so a walk's own delete of it,
        // which may be next, reads its `NotFound` as done.
        self.inner.sidecars.remember_taken(&sidecar);
        match tree.delete_file(&mut conn.clone(), &sidecar).await {
            Ok(()) => {
                debug!(
                    "smb: deleted the AppleDouble file at path={:?}",
                    LogPrivate::file_path(&sidecar)
                );
                self.notify_sidecar_deleted(&sidecar).await;
            }
            Err(e) if e.kind() == smb2::ErrorKind::NotFound => {
                // Someone else took it first; their delete owns the answer.
                self.inner.sidecars.take_back(&sidecar);
            }
            Err(e) => {
                self.inner.sidecars.take_back(&sidecar);
                warn!(
                    "smb: the AppleDouble file at path={:?} stays, it didn't delete: {e}",
                    LogPrivate::file_path(&sidecar)
                );
            }
        }
    }

    /// The pane patch for a deleted sidecar: one `Deleted` in its folder.
    async fn notify_sidecar_deleted(&self, sidecar: &str) {
        let display = self.to_display_path(sidecar);
        let path = Path::new(&display);
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
            && let Some(parent_display) = self.display_path_for(parent)
        {
            self.notify_mutation(
                &self.inner.volume_id,
                &parent_display,
                MutationEvent::Deleted(name.to_string_lossy().to_string()),
            )
            .await;
        }
    }
}

/// The first bytes of the file at `smb_path`, or `None` when nothing is there.
/// One frame for a missing or small file; a bigger one (a large resource fork)
/// opens and reads its head.
async fn read_head(tree: &Arc<Tree>, conn: &Connection, smb_path: &str) -> Result<Option<Vec<u8>>, smb2::Error> {
    match tree
        .read_file_compound_sized(&mut conn.clone(), smb_path, PROBE_READ_BYTES)
        .await
    {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == smb2::ErrorKind::NotFound => Ok(None),
        Err(smb2::Error::FileTooLargeForSingleRead { .. }) => {
            let reader = match tree.open_file_reader(conn.clone(), smb_path).await {
                Ok(reader) => reader,
                Err(e) if e.kind() == smb2::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            let head = reader.read_at(0, APPLE_DOUBLE_MAGIC.len() as u64).await;
            if let Err(e) = reader.close().await {
                debug!("smb: closing a `._` file's probe: {e}");
            }
            head.map(Some)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[path = "apple_double_test.rs"]
mod apple_double_test;
