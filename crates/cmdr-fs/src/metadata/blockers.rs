//! Clearing what makes an item refuse a rename or an unlink: the user flags
//! `uchg`/`uappnd` and the ACL.
//!
//! Two callers want it. A landing whose rename was refused clears the staged
//! item, lands it, and puts back what it cleared ([`Unblock::Cleared`] hands the
//! set over). A reversal or a sweep about to remove an item Cmdr made clears it
//! so the removal can happen at all. Both ask only after a first attempt was
//! refused, so an ordinary item never reaches here.
//!
//! A removal can also be refused over the FOLDER holding the item: Locked, a
//! `deny delete_child` ACL, or a mode without owner write, all of which a copied
//! folder carries from its source. [`open_parent_folder`] opens that folder for
//! the one removal and puts it back after.

use std::io;
use std::path::Path;

use super::LandedMetadata;

/// What [`clear_blockers`] did to one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unblock {
    /// It cleared these; put them back on the item where it lands to keep them.
    Cleared(LandedMetadata),
    /// The item wears nothing to clear (or is gone), so a retry would be refused
    /// the same way.
    NothingToClear,
    /// It wears a blocker this process can't clear (`schg`/`sappnd`, which only
    /// root can), or the clear itself was refused.
    Refused,
}

/// Clears `path`'s own `uchg`/`uappnd` and strips its ACL, flags first because
/// `uchg` refuses an ACL write. Never follows a symlink. On Linux there's never
/// anything to clear (module docs).
pub fn clear_blockers(path: &Path) -> Unblock {
    super::platform::clear_blockers(path)
}

/// What stands between an item and a rename or an unlink of it, as a move that
/// needs one sees it. Read by [`removal_lock`], before anything is cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalLock {
    /// Nothing does.
    Free,
    /// The user's own protection: `uchg`/`uappnd` (Finder's Locked), or an ACL
    /// entry denying `delete` (or `delete_child`, which refuses taking anything
    /// out of a folder). [`clear_blockers`] lifts it.
    Locked,
    /// `schg`/`sappnd`: only root can clear them, so no consent can move it.
    SystemLocked,
}

/// Whether `path` itself (never a symlink's target) wears a [`RemovalLock`].
/// System flags win over the user's lock: an item wearing both can't move
/// either way. A deny entry that only reaches the item's future children
/// (`only_inherit`) doesn't count, and neither does an ACL that can't be read:
/// the move then meets the refusal where it would have anyway. Always
/// [`RemovalLock::Free`] off macOS (module docs).
pub fn removal_lock(path: &Path) -> RemovalLock {
    super::platform::removal_lock(path)
}

/// What [`open_parent_folder`] did to the folder holding an item.
#[derive(Debug)]
pub enum ParentOpening {
    /// It opened the folder; [`OpenedFolder::close`] puts back what it changed.
    Opened(OpenedFolder),
    /// The folder refuses nothing it could change, so a retry would be refused
    /// the same way.
    NothingToOpen,
    /// It wears a blocker this process can't clear, or the change was refused.
    Refused,
}

/// A folder [`open_parent_folder`] opened for one removal. Closing it, or
/// dropping it, puts back its mode, ACL, and flags.
#[derive(Debug)]
pub struct OpenedFolder {
    path: std::path::PathBuf,
    restore: Option<LandedMetadata>,
}

impl OpenedFolder {
    /// An opened `path`, which `restore` puts back on close.
    #[cfg_attr(
        target_family = "wasm",
        expect(dead_code, reason = "the browser build has no folder to open")
    )]
    pub(super) fn new(path: std::path::PathBuf, restore: LandedMetadata) -> Self {
        Self {
            path,
            restore: Some(restore),
        }
    }

    /// Puts the folder's own mode, ACL, and flags back, in the order
    /// [`super::apply_landed`] keeps.
    pub fn close(mut self) -> super::MetadataReport {
        self.restore_now()
    }

    fn restore_now(&mut self) -> super::MetadataReport {
        match self.restore.take() {
            Some(set) => super::apply_landed(&self.path, &set),
            None => super::MetadataReport::ABSENT,
        }
    }
}

impl Drop for OpenedFolder {
    fn drop(&mut self) {
        self.restore_now();
    }
}

/// Opens the folder holding `child` so `child` can leave it: clears its own
/// `uchg`/`uappnd`, strips its ACL (a `deny delete_child` refuses the unlink),
/// and adds owner write and search when it lacks them (a `0500` folder refuses
/// it too). For a folder a copy made and carried a source's metadata onto, which
/// a reversal now empties. ❗ The folder isn't removed here: what's opened is put
/// back by [`OpenedFolder::close`].
pub fn open_parent_folder(child: &Path) -> ParentOpening {
    super::platform::open_parent_folder(child)
}

/// Why a `remove_*_clearing_blockers` call left the item in place.
#[derive(Debug)]
pub enum RemovalRefusal {
    /// The item wears a blocker this process can't clear ([`Unblock::Refused`]).
    Locked,
    /// The removal itself failed: the first refusal when there was nothing to
    /// clear, else the retry's.
    Io(io::Error),
}

/// `remove_file`, and when that's refused, [`clear_blockers`] and one retry,
/// then [`open_parent_folder`] and one more.
pub fn remove_file_clearing_blockers(path: &Path) -> Result<(), RemovalRefusal> {
    remove_clearing(path, |p| std::fs::remove_file(p))
}

/// `remove_dir` (empty only), and when that's refused, [`clear_blockers`] and
/// one retry, then [`open_parent_folder`] and one more.
pub fn remove_dir_clearing_blockers(path: &Path) -> Result<(), RemovalRefusal> {
    remove_clearing(path, |p| std::fs::remove_dir(p))
}

/// The removal, then on a refusal the item's own blockers, then its folder's
/// ([`open_parent_folder`], put back right after), each followed by one retry.
fn remove_clearing(path: &Path, remove: impl Fn(&Path) -> io::Result<()>) -> Result<(), RemovalRefusal> {
    let refused = |result: io::Result<()>| match result {
        Ok(()) => Ok(None),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(Some(e)),
        Err(e) => Err(RemovalRefusal::Io(e)),
    };
    let Some(first) = refused(remove(path))? else {
        return Ok(());
    };
    let refusal = match clear_blockers(path) {
        Unblock::Cleared(_) => match refused(remove(path))? {
            None => return Ok(()),
            Some(again) => again,
        },
        Unblock::NothingToClear => first,
        Unblock::Refused => return Err(RemovalRefusal::Locked),
    };
    match open_parent_folder(path) {
        ParentOpening::Opened(folder) => {
            let removed = remove(path);
            folder.close();
            removed.map_err(RemovalRefusal::Io)
        }
        ParentOpening::NothingToOpen => Err(RemovalRefusal::Io(refusal)),
        ParentOpening::Refused => Err(RemovalRefusal::Locked),
    }
}

/// `remove_dir_all`, and when that fails, clearing every entry left in the tree
/// (and the root) before one retry. Recursing only after a refusal keeps an
/// ordinary tree at exactly the cost of `remove_dir_all`.
///
/// ❗ Only for a tree Cmdr made and owns whole (a cross-filesystem move's
/// staging folder): it clears and removes everything in it.
pub fn remove_tree_clearing_blockers(root: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(root) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(e),
        Err(first) => {
            if !clear_tree(root) {
                return Err(first);
            }
            std::fs::remove_dir_all(root)
        }
    }
}

/// Clears every entry under `dir` and `dir` itself, never following a link.
/// Whether it cleared anything.
fn clear_tree(dir: &Path) -> bool {
    // The folder first, so a Locked folder's children can then be read past it
    // and removed out of it.
    let mut cleared = matches!(clear_blockers(dir), Unblock::Cleared(_));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                cleared |= clear_tree(&path);
            } else {
                cleared |= matches!(clear_blockers(&path), Unblock::Cleared(_));
            }
        }
    }
    cleared
}
