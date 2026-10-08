//! The vocabulary for work a server does on its own: whether a rename is one
//! call there, how big a subtree is, and the progress a server-side copy
//! reports.
//!
//! An object store (S3) has no rename: a "rename" copies every object and
//! deletes the source. Every caller of [`Volume::rename`]
//! assumes one cheap call, so a volume says per entry which kind of work a
//! rename is ([`RenameWork`]), and a caller that gets
//! [`RenameWork::CopyThenDelete`] sends the entry through the transfer engine as
//! a move, with progress, pause, cancel, and journaling.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use super::{FileEntry, ScannedFile, Volume, VolumeError};

/// How renaming one entry runs on its volume
/// ([`Volume::rename_work`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameWork {
    /// One cheap server-side call renames the entry, a folder's whole subtree
    /// included: POSIX `rename`, SMB's and SFTP's rename, MTP's `MoveObject`.
    OneCall,
    /// The server has no rename for this entry: its bytes are copied (inside
    /// the server where it can) and the source deleted after. An object
    /// store's folder (one copy and one delete per object), or a file big
    /// enough that its copy needs progress and cancel.
    CopyThenDelete,
}

/// What [`Volume::tally_subtree`] counted under
/// one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtreeTally {
    /// Files counted, a folder's at any depth. A file is one.
    pub files: u64,
    /// Their bytes.
    pub bytes: u64,
    /// Folders seen, the tallied folder itself included; `0` for a file.
    pub folders: u64,
    /// Each counted file's size and date (on an object store, its upload
    /// time), so the rename's cost can be estimated without asking again.
    pub per_file: Vec<ScannedFile>,
    /// `false` when the count stopped at its cap, so there are more files
    /// than `files` (and more bytes than `bytes`).
    pub complete: bool,
}

/// The progress and pause hook a server-side copy reports through
/// ([`Volume::copy_on_server`]).
///
/// A server-side copy moves no bytes through Cmdr, so there's no stream to
/// park between chunks: a backend that copies in pieces (S3's parts) asks
/// [`checkpoint`](Self::checkpoint) before starting each one, which is where a
/// pause lands and a cancel arrives.
pub trait ServerCopyProgress: Sync {
    /// `done` of `total` bytes are in place. `Break` asks the copy to stop.
    fn advanced(&self, done: u64, total: u64) -> ControlFlow<()>;

    /// Waits out a pause, then answers whether to go on. Call it before
    /// starting each piece, ❌ never while holding anything the server bills
    /// by the minute.
    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>>;
}

/// Counts the files under `path` by listing each folder in turn, stopping once
/// more than `cap` are found: the body of the trait's default
/// [`tally_subtree`](super::Volume::tally_subtree).
pub async fn tally_by_listing<V: Volume + ?Sized>(
    volume: &V,
    path: &Path,
    cap: u64,
) -> Result<SubtreeTally, VolumeError> {
    let entry = volume.get_metadata(path).await?;
    if !entry.is_directory {
        let file = scanned(&entry);
        return Ok(SubtreeTally {
            files: 1,
            bytes: file.size,
            folders: 0,
            per_file: vec![file],
            complete: true,
        });
    }
    let mut tally = SubtreeTally {
        files: 0,
        bytes: 0,
        folders: 0,
        per_file: Vec::new(),
        complete: true,
    };
    let mut pending: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        tally.folders += 1;
        for child in volume.list_directory(&dir, None).await? {
            // A link is counted as the one entry it is, ❌ never walked: a
            // rename moves it as itself.
            if child.is_directory && !child.is_symlink {
                pending.push(PathBuf::from(&child.path));
                continue;
            }
            if tally.files >= cap {
                tally.complete = false;
                return Ok(tally);
            }
            let file = scanned(&child);
            tally.files += 1;
            tally.bytes += file.size;
            tally.per_file.push(file);
        }
    }
    Ok(tally)
}

fn scanned(entry: &FileEntry) -> ScannedFile {
    ScannedFile {
        size: entry.size.unwrap_or(0),
        modified_at: entry.modified_at,
    }
}

#[cfg(test)]
#[path = "server_side_test.rs"]
mod server_side_test;
