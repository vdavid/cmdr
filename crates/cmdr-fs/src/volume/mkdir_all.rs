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
//! ❗ **Leaf first.** The trait's default calls `exists()` once per ancestor,
//! which over a 50 ms link is one round trip per level before a single directory
//! gets made, paid on every copy into a deep destination. The common case is a
//! new folder under a parent that is already there, and that costs exactly one
//! request here.

use std::path::{Path, PathBuf};

use crate::volume::DirectoryCreation;
use crate::volume::VolumeError;
use crate::volume::scan_walk::Walking;

/// What the walk needs from a backend: its own path spelling, and one request
/// that makes exactly one directory.
pub trait MakesDirectories: Sync {
    /// `path` in the backend's own spelling, or `NotFound` when it isn't on this
    /// volume.
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError>;

    /// Makes exactly one directory, no parents.
    ///
    /// ❗ The refusals are load-bearing: `AlreadyExists` when the name is taken,
    /// `NotFound` when an ancestor is missing. Every other error stops the walk,
    /// because a read-only export, a quota, or a refused name fails the same way
    /// at every level and walking would only spend round trips to arrive at the
    /// same answer.
    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()>;
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

    match maker.make_one_directory(&remote).await {
        Ok(()) => {
            return Ok(MadeDirectories {
                leaf: DirectoryCreation::Created,
                shallowest_created: Some(path.to_path_buf()),
            });
        }
        Err(VolumeError::AlreadyExists(_)) => {
            return Ok(MadeDirectories {
                leaf: DirectoryCreation::AlreadyExisted,
                shallowest_created: None,
            });
        }
        // Only a missing ancestor earns the walk.
        Err(VolumeError::NotFound(_)) => {}
        Err(e) => return Err(e),
    }

    // Leaf → root, stopping at the volume root, then created shallowest first so
    // no child is asked for before its parent. Each level keeps both spellings:
    // the remote one to create, the caller's to patch a pane with.
    let mut missing: Vec<(&Path, String)> = Vec::new();
    for ancestor in path.ancestors() {
        let Ok(remote_ancestor) = maker.remote_path_of(ancestor) else {
            break;
        };
        if remote_ancestor == root {
            break;
        }
        missing.push((ancestor, remote_ancestor));
    }

    let mut leaf = DirectoryCreation::AlreadyExisted;
    let mut shallowest_created: Option<PathBuf> = None;
    for (index, (as_addressed, dir)) in missing.iter().enumerate().rev() {
        match maker.make_one_directory(dir).await {
            Ok(()) => {
                shallowest_created.get_or_insert_with(|| as_addressed.to_path_buf());
                if index == 0 {
                    leaf = DirectoryCreation::Created;
                }
            }
            // ❗ A lost race answers `AlreadyExisted` for the leaf, which is the
            // safe direction: the driver keeps its conflict probe.
            Err(VolumeError::AlreadyExists(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(MadeDirectories {
        leaf,
        shallowest_created,
    })
}

#[cfg(test)]
#[path = "mkdir_all_test.rs"]
mod mkdir_all_test;
