//! What an in-flight operation has put at the destination, and how a reversal
//! recognizes it again.
//!
//! Cancelling a transfer reverses it: the copy's Rollback deletes what it wrote,
//! the move's renames items back. Both acts are destructive, and both happen at a
//! path the operation may have written hours earlier, so a bare path isn't enough
//! to act on. What these types carry is the answer to "is the thing sitting here
//! still the thing I put here?".
//!
//! The identity is deliberately NOT symmetric across backends, and the two cases
//! are separate variants rather than one struct of optional fields, so no call
//! site can quietly record half of one. `transfer/DETAILS.md` § "What the
//! in-flight ledgers record".

use std::fs::Metadata;
use std::path::{Path, PathBuf};

/// One destination path this operation put on disk, with the identity a reversal
/// rechecks before removing it or renaming it back.
///
/// Build one through [`WrittenFile::local`], [`WrittenFile::volume`], or
/// [`WrittenFile::own_partial`]: each names the kind of write it describes, and
/// none of them can produce a local entry missing its node id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WrittenFile {
    /// Where it landed. For a move ledger this is the moved-TO path, the one a
    /// reversal has to recognize before renaming it back.
    pub(crate) path: PathBuf,
    pub(crate) identity: WrittenIdentity,
}

impl WrittenFile {
    /// A local entry, snapshotted from the path itself with `symlink_metadata`
    /// (so a symlink describes the LINK, never its target). A stat that fails
    /// lands [`WrittenIdentity::Unverifiable`].
    pub(crate) fn local(path: PathBuf) -> Self {
        let identity = WrittenIdentity::at_local_path(&path);
        Self { path, identity }
    }

    /// A local entry whose metadata the caller already holds — the same snapshot
    /// [`WrittenFile::local`] takes, without the second stat. A same-FS move
    /// passes the SOURCE's metadata: the rename carries the node id across, so it
    /// describes the landed entry exactly.
    pub(crate) fn local_stat(path: PathBuf, meta: Option<&Metadata>) -> Self {
        Self {
            path,
            identity: WrittenIdentity::of_stat(meta),
        }
    }

    /// A complete file written through a `Volume` backend, with the byte count
    /// the leaf copier piped into it.
    pub(crate) fn volume(path: PathBuf, size: u64) -> Self {
        Self {
            path,
            identity: WrittenIdentity::VolumeFile { size },
        }
    }

    /// A write THIS operation was still making when it stopped. See
    /// [`WrittenIdentity::OwnPartial`].
    pub(crate) fn own_partial(path: PathBuf) -> Self {
        Self {
            path,
            identity: WrittenIdentity::OwnPartial,
        }
    }
}

/// How a reversal can tell that what now sits at a destination path is still what
/// this operation put there.
///
/// ❌ Don't add an mtime to any variant. Snapshots are whole seconds, and FAT32
/// and network mounts store mtime more coarsely than that, so every preserved
/// mtime reads back truncated and the whole copy looks changed. Symlinks are
/// worse: the snapshot would come from the source link while `copy_symlink`
/// creates a fresh one, so every copied link drifts. Both leave files behind at
/// the destination, which is the failure a reversal exists to avoid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WrittenIdentity {
    /// A local file (or symlink): the node id it landed on, plus its size.
    ///
    /// The node id is what makes this exact rather than plausible. Size alone
    /// passes a file that was replaced by a DIFFERENT file of the same size,
    /// which is what an editor's write-temp-then-rename produces; a
    /// rename-into-place preserves nothing, so the node changes and the reversal
    /// correctly leaves it alone. Nothing changes a node id without someone
    /// touching the file, so it adds no false alarms of its own.
    LocalFile { size: u64, node: NodeId },
    /// A local directory: the node id alone. A directory's own reported size
    /// changes as children come and go, so it proves nothing about identity and
    /// would fire on a folder someone merely dropped a file into.
    LocalDir { node: NodeId },
    /// A complete file on a `Volume` backend (SMB, MTP, an archive): the size the
    /// write reported, and nothing else. No backend but the local filesystem
    /// offers a stable node id, so these carry the same-size exposure the
    /// operation log's own reversal has always carried.
    VolumeFile { size: u64 },
    /// A write this operation was still making when it stopped: a truncated
    /// destination, or a staged temp that never landed.
    ///
    /// There is no complete file to recognize here, and by construction no size
    /// either — but nothing except this operation can plausibly own a destination
    /// path that never held a complete file, so a reversal removes one of these
    /// on sight. ❌ It is NOT an entry whose identity we happen not to know
    /// ([`WrittenIdentity::Unverifiable`] is that): folding the two together
    /// strands a truncated file at the destination, the exact outcome the
    /// mid-file-cancel work exists to prevent.
    OwnPartial,
    /// Nothing could be established about the entry — the stat that would have
    /// snapshotted it failed. Unprovable, so a reversal fails safe and leaves it.
    Unverifiable,
}

impl WrittenIdentity {
    /// The identity of a local entry the caller already stat'd with
    /// `symlink_metadata`. `None` means the stat failed, which is the only
    /// honest route to [`WrittenIdentity::Unverifiable`].
    pub(crate) fn of_stat(meta: Option<&Metadata>) -> Self {
        let Some(meta) = meta else {
            return Self::Unverifiable;
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let node = NodeId {
                dev: meta.dev(),
                ino: meta.ino(),
            };
            if meta.is_dir() {
                Self::LocalDir { node }
            } else {
                Self::LocalFile { size: meta.len(), node }
            }
        }
        // No node id off Unix, and size alone isn't an identity worth deleting on.
        #[cfg(not(unix))]
        {
            let _ = meta;
            Self::Unverifiable
        }
    }

    /// The identity of the entry at `path`, read with `symlink_metadata` so a
    /// symlink describes itself rather than whatever it points at. The recheck
    /// must stat the same way, or a copied link that dangles reads as absent.
    pub(crate) fn at_local_path(path: &Path) -> Self {
        Self::of_stat(std::fs::symlink_metadata(path).ok().as_ref())
    }

    /// The size this entry was recorded with, for the journal row that mirrors
    /// it. `None` wherever a size was never part of the identity.
    pub(crate) fn recorded_size(&self) -> Option<u64> {
        match self {
            Self::LocalFile { size, .. } | Self::VolumeFile { size } => Some(*size),
            Self::LocalDir { .. } | Self::OwnPartial | Self::Unverifiable => None,
        }
    }
}

/// A local filesystem node, as `(dev, ino)`. Equal pairs are the same file: the
/// device disambiguates inode numbers, which are only unique per filesystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NodeId {
    dev: u64,
    ino: u64,
}

// ============================================================================
// The local copy's ledger
// ============================================================================

/// Tracks created files/directories for rollback on failure.
///
/// If dropped without calling `commit()`, automatically rolls back
/// (deletes) all recorded files and directories. This ensures cleanup
/// even if a thread panics during the copy loop.
#[cfg_attr(test, derive(Debug))]
pub(crate) struct CopyTransaction {
    /// The destination files this operation wrote, in creation order, each with
    /// the identity it landed with. A **stack**: [`CopyTransaction::pop_file`]
    /// takes the newest one off as it's reversed, so at every instant the ledger
    /// claims exactly what this operation still has on disk. A reversal that
    /// stops halfway can then report honestly on what it left.
    created_files: Vec<WrittenFile>,
    /// In creation order. Not a stack: these rows are also what
    /// `commit_journaling_created_dirs` journals after a reversal, so the list
    /// has to survive one. Rollback prunes them empty-only, deepest first.
    pub created_dirs: Vec<PathBuf>,
    /// Set to `true` by `commit()` to prevent rollback on drop.
    committed: bool,
}

impl CopyTransaction {
    pub fn new() -> Self {
        Self {
            created_files: Vec::new(),
            created_dirs: Vec::new(),
            committed: false,
        }
    }

    pub fn record_file(&mut self, file: WrittenFile) {
        self.created_files.push(file);
    }

    pub fn record_dir(&mut self, path: PathBuf) {
        self.created_dirs.push(path);
    }

    /// The files this operation still claims to have on disk, newest last.
    pub fn created_files(&self) -> &[WrittenFile] {
        &self.created_files
    }

    /// The destination paths, for the passes that only need to name them (the
    /// closing `fdatasync`, the cross-FS move's staging→final rebase).
    pub fn created_file_paths(&self) -> Vec<PathBuf> {
        self.created_files.iter().map(|f| f.path.clone()).collect()
    }

    /// Take the newest file off the ledger, to reverse it. Popping BEFORE the
    /// delete is deliberate: an entry this operation has stopped claiming is one
    /// nothing will try to remove twice, whether the delete succeeded, was
    /// refused, or found the file already gone.
    pub fn pop_file(&mut self) -> Option<WrittenFile> {
        self.created_files.pop()
    }

    /// Rolls back all created files and directories.
    ///
    /// Intentional: rollback removes the files THIS operation created; it does
    /// NOT restore an original that an Overwrite replaced (we keep no per-file
    /// backup — see `overwrite::safe_overwrite_file` step 4). Keeping backups for
    /// the whole operation risks unexpectedly filling the user's drive on a
    /// large Overwrite. Revisit if users complain. See transfer/volume/DETAILS.md
    /// § "Overwrite isn't reversible".
    pub fn rollback(&mut self) {
        // Delete files first (newest first), draining the ledger as they go.
        while let Some(file) = self.pop_file() {
            let _ = std::fs::remove_file(&file.path);
        }
        // Then directories (deepest first, already in reverse due to creation order)
        for dir in self.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(dir);
        }
    }

    /// Marks the transaction as committed, preventing rollback on drop.
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for CopyTransaction {
    fn drop(&mut self) {
        if !self.committed {
            log::warn!(
                "CopyTransaction dropped without commit, rolling back {} files and {} dirs",
                self.created_files.len(),
                self.created_dirs.len()
            );
            self.rollback();
        }
    }
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
