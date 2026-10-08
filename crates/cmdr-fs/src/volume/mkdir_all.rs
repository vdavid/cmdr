//! `mkdir -p` for a backend whose every level costs a round trip, and the
//! honesty contract the answer carries.
//!
//! ❗ **`Created` is a promise the transfer driver SPENDS.** On it, the driver
//! skips its per-file destination conflict probe for everything it writes
//! inside, because a directory it just made cannot hold anything. So a `Created`
//! for a directory that was merely FOUND turns "would have prompted" into
//! "overwrote", for every file in the copy. Anything short of certainty answers
//! `AlreadyExisted`, including a lost create race.
//!
//! ❗ **Leaf first.** The trait's default probes once per ancestor, which over a
//! 50 ms link is one round trip per level before a single directory gets made,
//! paid on every copy into a deep destination. The common case is a new folder
//! under a parent that is already there, and that costs exactly one request
//! here.
//!
//! ❗ **A taken name isn't a folder until somebody looks.** A server answers a
//! create on an occupied name the same way whatever holds it, so a FILE where a
//! folder should be reads as "already there". Left at that, the path itself
//! answers `AlreadyExisted` and the transfer goes on to write into a folder that
//! isn't one, and a path below it fails one level down as `NotFound`, naming a
//! folder the user asked Cmdr to CREATE as the thing that's missing. Both are
//! [`VolumeError::NotADirectory`], carrying the file that's in the way.
//!
//! The look ([`MakesDirectories::leads_to`]) only ever happens after a refusal,
//! so it classifies and ❌ never guards, and the happy path pays nothing for it:
//!
//! - A leaf that answers `AlreadyExists` costs one look, because nothing later
//!   in the walk would notice a file there.
//! - An ancestor that answers `AlreadyExists` costs none. If it isn't a folder
//!   the level below it fails, and the look happens then.
//! - A create that fails unclassified (or as `NotFound`, mid-walk) looks upward
//!   from the failed level for the nearest thing that exists. Apache answers 400
//!   to everything addressed under a file, the question included, which is why an
//!   unclassified answer to the look means "keep going up".
//!
//! ## A link to a folder is a folder
//!
//! The look FOLLOWS links, so a destination reached through one is created into
//! like any other: that is what `mkdir -p` does, and a path somebody typed or
//! navigated through (`/home` on a NAS, `/sdcard` on a phone) is theirs to write
//! under. ❌ Don't carry the merge engine's "not a directory in its own right"
//! rule over here. That one stops a merge from landing a folder's CONTENTS in a
//! link's target nobody picked; this creates the folder somebody asked for.
//! `conformance::assert_create_directory_all_goes_through_a_link_to_a_folder`
//! holds every backend that has links to it.

use std::path::{Path, PathBuf};

use crate::volume::scan_walk::Walking;
use crate::volume::{DirectoryCreation, Volume, VolumeError};

/// What a name leads to once links are followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeadsTo {
    /// A directory, or a link to one.
    Directory,
    /// A file, or a link to one: anything there that isn't a directory.
    NotADirectory,
    /// Nothing: no such name, or a link to nothing.
    Nothing,
}

/// What the walk needs from a backend: its own path spelling, one request that
/// makes exactly one directory, and one that says what a name leads to.
pub trait MakesDirectories: Sync {
    /// `path` in the backend's own spelling, or `NotFound` when it isn't on this
    /// volume.
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError>;

    /// Makes exactly one directory, no parents.
    ///
    /// ❗ The refusals are load-bearing: `AlreadyExists` when the name is taken,
    /// `NotFound` when an ancestor is missing. A refusal the backend could
    /// classify as anything else stops the walk, because a read-only export, a
    /// quota, or a refused name fails the same way at every level and walking
    /// would only spend round trips to arrive at the same answer.
    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()>;

    /// What the name at `remote` leads to. An `Err` only when the question
    /// itself got no answer.
    ///
    /// ❗ Follow links, one stat that does (module docs § "A link to a folder is a
    /// folder"). An `lstat` here refuses every destination reached through a
    /// link.
    ///
    /// Asked only after [`make_one_directory`](Self::make_one_directory) refused,
    /// so it classifies a refusal that already happened and ❌ never guards a
    /// write.
    fn leads_to<'a>(&'a self, remote: &'a str) -> Walking<'a, LeadsTo>;
}

/// What a `mkdir -p` did.
pub struct MadeDirectories {
    /// Whether the LEAF was created here, or was already there. The promise the
    /// module docs describe.
    pub leaf: DirectoryCreation,
    /// The SHALLOWEST directory this created, if any: its parent is the only
    /// listing a pane could be holding, so it is the one patch worth making.
    /// ❗ One patch, ❌ never one per level.
    pub shallowest_created: Option<PathBuf>,
}

/// Creates `path` and any missing ancestors under the volume root.
pub async fn create_directory_all(maker: &dyn MakesDirectories, path: &Path) -> Result<MadeDirectories, VolumeError> {
    let remote = maker.remote_path_of(path)?;
    // The volume root always exists, and so does every spelling of it.
    let root = maker.remote_path_of(Path::new("/"))?;
    if remote == root {
        return Ok(MadeDirectories {
            leaf: DirectoryCreation::AlreadyExisted,
            shallowest_created: None,
        });
    }

    // Leaf → root, stopping at the volume root. Each level keeps both spellings:
    // the remote one to create, the caller's to patch a pane with.
    let mut levels: Vec<(&Path, String)> = Vec::new();
    for ancestor in path.ancestors() {
        let Ok(remote_ancestor) = maker.remote_path_of(ancestor) else {
            break;
        };
        if remote_ancestor == root {
            break;
        }
        levels.push((ancestor, remote_ancestor));
    }

    match maker.make_one_directory(&remote).await {
        Ok(()) => {
            return Ok(MadeDirectories {
                leaf: DirectoryCreation::Created,
                shallowest_created: Some(path.to_path_buf()),
            });
        }
        Err(VolumeError::AlreadyExists(_)) => {
            refuse_unless_a_directory(maker, &remote).await?;
            return Ok(MadeDirectories {
                leaf: DirectoryCreation::AlreadyExisted,
                shallowest_created: None,
            });
        }
        // Only a missing ancestor earns the walk.
        Err(VolumeError::NotFound(_)) => {}
        Err(e) => return Err(blamed_on_what_is_in_the_way(maker, &levels, e).await),
    }

    // Created shallowest first, so no child is asked for before its parent.
    let mut leaf = DirectoryCreation::AlreadyExisted;
    let mut shallowest_created: Option<PathBuf> = None;
    for (index, (as_addressed, dir)) in levels.iter().enumerate().rev() {
        match maker.make_one_directory(dir).await {
            Ok(()) => {
                shallowest_created.get_or_insert_with(|| as_addressed.to_path_buf());
                if index == 0 {
                    leaf = DirectoryCreation::Created;
                }
            }
            // ❗ A lost race answers `AlreadyExisted` for the leaf, which is the
            // safe direction: the driver keeps its conflict probe.
            Err(VolumeError::AlreadyExists(_)) => {
                if index == 0 {
                    refuse_unless_a_directory(maker, dir).await?;
                }
            }
            Err(e) => return Err(blamed_on_what_is_in_the_way(maker, &levels[index..], e).await),
        }
    }
    Ok(MadeDirectories {
        leaf,
        shallowest_created,
    })
}

/// The leaf answered `AlreadyExists`: fine when the name leads to a folder,
/// [`VolumeError::NotADirectory`] when it leads anywhere else. A link to nothing
/// counts as in the way: the name is taken and no folder is behind it.
async fn refuse_unless_a_directory(maker: &dyn MakesDirectories, remote: &str) -> Result<(), VolumeError> {
    match maker.leads_to(remote).await? {
        LeadsTo::Directory => Ok(()),
        LeadsTo::NotADirectory | LeadsTo::Nothing => Err(VolumeError::NotADirectory(remote.to_string())),
    }
}

/// The refusal to report for a create that failed with `e` at `levels[0]`:
/// [`VolumeError::NotADirectory`] naming the nearest thing at or above it that
/// exists and isn't a folder, or `e` itself.
///
/// ❗ The look may only make a report MORE accurate. A folder at the nearest
/// level that exists, or a question nobody could answer, leaves `e` standing.
async fn blamed_on_what_is_in_the_way(
    maker: &dyn MakesDirectories,
    levels: &[(&Path, String)],
    e: VolumeError,
) -> VolumeError {
    // A refusal the backend classified (no permission, read-only, full, gone)
    // already says what it is.
    if !matches!(e, VolumeError::NotFound(_) | VolumeError::IoError { .. }) {
        return e;
    }
    // `NotFound` already says nothing is at the failed level itself.
    let first = usize::from(matches!(e, VolumeError::NotFound(_)));
    for (_, level) in levels.iter().skip(first) {
        match maker.leads_to(level).await {
            Ok(LeadsTo::NotADirectory) => return VolumeError::NotADirectory(level.clone()),
            // Nothing here, or a server that can't even address the name, which
            // is what one does under a file: the answer is further up.
            Ok(LeadsTo::Nothing) | Err(VolumeError::IoError { .. }) => {}
            Ok(LeadsTo::Directory) | Err(_) => break,
        }
    }
    e
}

/// [`LeadsTo`] for a path on any [`Volume`], from the two questions every
/// backend already answers. The trait's default `create_directory_all` asks it
/// in place of a bare `exists()`, at the same cost.
///
/// ❗ The second look is what follows a link: `is_directory` is an `lstat` on a
/// local disk, where metadata reports a link to a folder as a directory. It
/// only happens for a name that is there and isn't a directory in its own right.
pub(super) async fn volume_path_leads_to<V: Volume + ?Sized>(volume: &V, path: &Path) -> LeadsTo {
    match volume.is_directory(path).await {
        Ok(true) => LeadsTo::Directory,
        Ok(false) => match volume.get_metadata(path).await {
            Ok(entry) if entry.is_directory => LeadsTo::Directory,
            _ => LeadsTo::NotADirectory,
        },
        // Unreadable reads as absent, as it does to `exists()`: the create that
        // follows reports what is actually wrong.
        Err(_) => LeadsTo::Nothing,
    }
}

#[cfg(test)]
#[path = "mkdir_all_test.rs"]
mod mkdir_all_test;
