//! What an operation's landed items say about themselves, read for their journal
//! rows: the node each one is ([`NodeIdentity`]), and for a volume transfer its
//! modified time too.
//!
//! The node is what lets an undo, and "Tell me about this", tell the item an
//! operation landed from anything that took its name since (`cmdr-operation-log`
//! DETAILS § "A landed item must still be the node that landed"). It's recorded
//! only on a local disk whose filesystem keeps one for good, decided once per
//! operation ([`OpScope::records_landed_nodes`]). ❗ Reading it never fails or
//! redirects the operation: a stat that fails records no identity, and the row
//! keeps the checks a row from before identities gets.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::fs::Metadata;
use std::path::{Path, PathBuf};

use cmdr_fs::volume::{LandedModified, MountClass, NodeIdentity, Volume};

use super::ledger::{WrittenFile, WrittenIdentity};
use super::scope::OpScope;

/// The node a copy or a move landed as on the local disk, for its row.
///
/// `metadata` is an `lstat` the caller already holds of the landed entry, or of the
/// source of a same-filesystem rename, which carries the node across: ❌ never a
/// stat made for this. `None` (the stat failed) records no identity, as does a
/// destination whose filesystem keeps no lasting one
/// (`local_disk::keeps_node_identity`, asked once per operation with `landed_at`,
/// a path on the destination that exists). Either way the row keeps the checks a
/// row from before identities gets, and the operation is untouched.
pub(crate) fn landed_node(scope: &OpScope, landed_at: &Path, metadata: Option<&Metadata>) -> Option<NodeIdentity> {
    let metadata = metadata?;
    scope
        .records_landed_nodes(|| crate::local_disk::keeps_node_identity(landed_at))
        .then(|| NodeIdentity::of(metadata))
        .flatten()
}

/// What a volume transfer's landed files report about themselves, read back from
/// the destination right after one top-level source lands: each file's modified
/// time, and on a local disk the node it landed as.
///
/// **The time** makes each row's snapshot size AND mtime. Size alone passed a file
/// edited in place at the same length (a VM disk image, a SQLite database, a
/// re-saved document), and undoing the copy deleted it with the user's edits (#519).
///
/// The landed file's OWN time, never the source's: a destination that doesn't keep
/// the source's date (WebDAV, an Android phone over MTP) would drift on every file.
/// ❗ It must be the time the undo's recheck reads, `get_metadata`: a FILE source
/// asks exactly that, and a DIRECTORY source takes what each write reported
/// (`WrittenFile::landed_modified`) and lists only the folders holding a file whose
/// write reported nothing, which the `Volume` contract makes agree with
/// `get_metadata` (`conformance::assert_landed_time_agrees_with_get_metadata`). On
/// S3 a listing is the upload time and a HEAD the source's date, so every write
/// there reports, and an undo of a folder copy reverses it (#651).
///
/// **The node** ([`NodeIdentity`]) is read only where the destination is a local
/// disk whose filesystem keeps one ([`volume_keeps_landed_nodes`]): one `lstat` per
/// landed file there, and ❌ no call at all on a share, a server, or a phone, whose
/// rows record none.
///
/// The cost on a remote destination is bounded per SOURCE, never per file: a FILE
/// source asks one `get_metadata`, a DIRECTORY source at most one `list_directory`
/// per destination folder its unreported files landed in. Only a transfer that
/// journals reads anything back, and a stopped or failed source doesn't (`Default`,
/// so a stop never waits on the share): its rows keep what the writes reported, or
/// the size-only check, and no node. A file the read-back can't place, or a backend
/// that reports no mtime, records `None`, which `verify_snapshot` skips over.
// DEFAULT-OK: empty means "nothing read back", and every lookup then answers `None`,
// the size-only check the row had before; it claims no time and no node for any file.
#[derive(Debug, Default)]
pub(crate) struct LandedReadBack {
    modified: HashMap<PathBuf, i64>,
    nodes: HashMap<PathBuf, NodeIdentity>,
}

impl LandedReadBack {
    /// Read back what one top-level source landed: `landed` for a FILE source,
    /// every `files` entry for a DIRECTORY source (its time only when its write
    /// reported none).
    pub(crate) async fn read_back(
        scope: &OpScope,
        dest: &dyn Volume,
        landed: &Path,
        source_is_dir: bool,
        files: &[WrittenFile],
    ) -> Self {
        Self {
            modified: Self::modified_times(dest, landed, source_is_dir, files).await,
            nodes: Self::nodes(scope, dest, landed, source_is_dir, files).await,
        }
    }

    /// The node each landed file is, where the destination keeps one. A stat that
    /// fails records none for that file.
    async fn nodes(
        scope: &OpScope,
        dest: &dyn Volume,
        landed: &Path,
        source_is_dir: bool,
        files: &[WrittenFile],
    ) -> HashMap<PathBuf, NodeIdentity> {
        let mut nodes = HashMap::new();
        if !scope.records_landed_nodes(|| volume_keeps_landed_nodes(dest, landed)) {
            return nodes;
        }
        let paths: Vec<&Path> = if source_is_dir {
            files.iter().map(|file| file.path.as_path()).collect()
        } else {
            vec![landed]
        };
        for path in paths {
            if let Ok(Some(node)) = dest.node_identity(path).await {
                nodes.insert(path.to_path_buf(), node);
            }
        }
        nodes
    }

    async fn modified_times(
        dest: &dyn Volume,
        landed: &Path,
        source_is_dir: bool,
        files: &[WrittenFile],
    ) -> HashMap<PathBuf, i64> {
        let mut times = HashMap::new();
        if !source_is_dir {
            if let Some(secs) = dest
                .get_metadata(landed)
                .await
                .ok()
                .and_then(|entry| entry.modified_at)
                .and_then(|secs| i64::try_from(secs).ok())
            {
                times.insert(landed.to_path_buf(), secs);
            }
            return times;
        }
        // Each folder's landed files by name. A listed name is only ever matched
        // against these, never joined onto a path.
        let mut by_folder: BTreeMap<&Path, HashMap<&OsStr, &WrittenFile>> = BTreeMap::new();
        for file in files
            .iter()
            .filter(|file| file.landed_modified == LandedModified::Unknown)
        {
            if let (Some(folder), Some(name)) = (file.path.parent(), file.path.file_name()) {
                by_folder.entry(folder).or_default().insert(name, file);
            }
        }
        for (folder, wanted) in by_folder {
            let Ok(entries) = dest.list_directory(folder, None).await else {
                continue;
            };
            for entry in &entries {
                let Some(file) = wanted.get(OsStr::new(&entry.name)) else {
                    continue;
                };
                // What landed has to be what's listed: a link for a link this
                // transfer carried, a plain file for anything else.
                let is_what_landed = if matches!(file.identity, WrittenIdentity::VolumeLink { .. }) {
                    entry.is_symlink
                } else {
                    !entry.is_directory && !entry.is_symlink
                };
                if is_what_landed && let Some(secs) = entry.modified_at.and_then(|secs| i64::try_from(secs).ok()) {
                    times.insert(file.path.clone(), secs);
                }
            }
        }
        times
    }

    /// The landed mtime read back for `path`, if one was.
    pub(crate) fn modified(&self, path: &Path) -> Option<i64> {
        self.modified.get(path).copied()
    }

    /// The node `path` landed as, if one was read back.
    pub(crate) fn node(&self, path: &Path) -> Option<NodeIdentity> {
        self.nodes.get(path).copied()
    }
}

/// Whether `dest` is a local disk whose filesystem keeps a lasting node identity
/// at `landed`, a path that exists on it. `mount_class` answers from what the
/// volume's registration found, so a share, a server, or a phone is ruled out
/// with no call to it; only a local disk pays the one `statfs`.
fn volume_keeps_landed_nodes(dest: &dyn Volume, landed: &Path) -> bool {
    dest.mount_class() == Some(MountClass::LocalDisk)
        && crate::local_disk::keeps_node_identity(&cmdr_fs::volume::root_anchored(dest.root(), landed))
}
