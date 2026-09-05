//! Git browser. Ships repo detection, repo info, status, the `.git/*` watcher,
//! friendly-error mapping, and the virtual `.git` portal: `branches/`, `tags/`,
//! `commits/`, `stash/`, `worktrees/`, `submodules/` browsable as virtual
//! trees, with cross-volume copy "for free" because git blobs flow through the
//! existing `VolumeReadStream` abstraction.
//!
//! ## Two seams, no hooks
//!
//! Everything under `.git/<category>/` is a ROUTE: `VolumeManager::resolve`
//! sends it to the read-only `GitPortalVolume` (`volume.rs`), which refuses
//! every mutation by trait default and can't be watched. The `.git/` landing
//! listing is a listing OVERLAY (`overlay.rs`), which reaches a pane and
//! nothing else. `LocalPosixVolume` names git nowhere, so a real file under
//! `.git` is an ordinary local file: editable, renamable, deletable, and
//! walkable when a repo folder is deleted.
//!
//! ## Honest blob streaming
//!
//! gix in 0.81 returns whole-blob `Vec<u8>` for `Object::data`. The
//! `GitBlobReadStream` owns that vec and yields 256 KB chunks for the
//! consumer API shape – memory cost equals blob size. We refuse blobs
//! over `tree::MAX_BLOB_BYTES` (256 MB) up-front rather than OOM. Future
//! work: revisit when gix exposes a chunked loose-object reader.
//!
//! ## Ref-name flat rendering
//!
//! Branches like `feature/foo` show up as a single entry called
//! `feature/foo`, not `feature/` containing `foo`. The classifier
//! greedy-matches ref names against the repo's known refs before
//! treating any remainder as a tree sub-path. See `path.rs`.

use std::path::Path;

pub mod column_meta;
// `FriendlyGitError` moved to `cmdr-fs`: `VolumeError::FriendlyGit` carries it,
// and it maps onto `friendly_error::ErrorCategory`, so the two must live
// together. Aliased so `git::friendly::…` keeps resolving.
pub use cmdr_fs::volume::friendly_error::git as friendly;
pub mod log;
pub mod overlay;
pub mod path;
pub mod portal;
pub mod read_blob;
pub mod repo;
pub mod snapshot_dates;
pub mod stash;
pub mod state_sink;
pub mod status;
pub mod submodules;
pub mod tree;
pub mod virtual_listing;
pub mod volume;
pub mod watcher;
pub mod wiring;
pub mod worktrees;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod category_tests;
#[cfg(test)]
mod column_meta_tests;
#[cfg(test)]
mod overlay_tests;
mod portal_tests;
#[cfg(test)]
mod snapshot_dates_tests;
#[cfg(test)]
pub(crate) mod test_fixtures;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod volume_tests;
#[cfg(test)]
mod walker_exposure_tests;
#[cfg(test)]
mod wiring_tests;

#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use friendly::{FriendlyGitError, FriendlyGitErrorKind};
#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use repo::{RepoInfo, repo_info};
#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use status::{EntryStatus, EntryStatusCode, list_status};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{VolumeError, VolumeReadStream};

/// The answer of a portal lookup that can legitimately find nothing.
///
/// `Ok(None)` means "that path isn't in this snapshot" (a typo in the path bar,
/// a file that only lives on another branch); the caller turns it into
/// `VolumeError::NotFound`. `Err` is reserved for a repo that couldn't answer
/// at all, which reaches the user as the git-specific repair copy.
pub type Lookup<T> = Result<Option<T>, FriendlyGitError>;

/// Opens the blob at `sub` inside `cat`/`name`, or `Ok(None)` when that path
/// holds no blob (missing, or a directory).
fn open_blob(handle: &repo::RepoHandle, cat: path::Cat, name: &str, sub: &str) -> Lookup<Box<dyn VolumeReadStream>> {
    let Some(commit_id) = resolve_commit_for_cat(handle, cat, name)? else {
        return Ok(None);
    };
    let Some(blob_id) = tree::lookup_blob_id(handle, commit_id, sub)? else {
        return Ok(None);
    };
    let bytes = tree::read_blob(handle, blob_id)?;
    Ok(Some(
        Box::new(read_blob::GitBlobReadStream::new(bytes)) as Box<dyn VolumeReadStream>
    ))
}

/// Folds a [`Lookup`] into what a `Volume` method returns: a miss becomes
/// `NotFound` carrying the path the caller asked for, so the transfer layer
/// renders the user's own file name rather than a git diagnostic.
pub(crate) fn found_or_not_found<T>(found: Lookup<T>, path: &Path) -> Result<T, VolumeError> {
    match found {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(VolumeError::NotFound(path.display().to_string())),
        Err(e) => Err(friendly_to_volume_error(e)),
    }
}

fn list_ref_tree(
    handle: &repo::RepoHandle,
    root: &Path,
    cat: path::Cat,
    name: &str,
    sub: &str,
) -> Lookup<Vec<FileEntry>> {
    let Some(commit_id) = resolve_commit_for_cat(handle, cat, name)? else {
        return Ok(None);
    };
    let display_parent = root
        .join(".git")
        .join(cat.as_segment())
        .join(name)
        .join(sub.replace('/', std::path::MAIN_SEPARATOR_STR));
    tree::list_tree(handle, commit_id, sub, &display_parent)
}

/// Resolves a `Cat::* / name` pair to the commit ID whose tree we should
/// browse. Branches/tags peel through refs, commits resolve the SHA prefix,
/// stash resolves through `stash@{n}`.
pub(crate) fn resolve_commit_for_cat(handle: &repo::RepoHandle, cat: path::Cat, name: &str) -> Lookup<gix::ObjectId> {
    match cat {
        path::Cat::Branches | path::Cat::Tags => virtual_listing::resolve_ref_commit(handle, cat, name),
        path::Cat::Commits => log::resolve_commit_id(handle, name).map(Some),
        path::Cat::Stash => {
            // `stash/<n>` is an index, so anything else names no entry.
            let Ok(n) = name.parse::<usize>() else {
                return Ok(None);
            };
            stash::resolve_stash_commit(handle, n).map(Some)
        }
        // Neither category browses a commit tree, so no name under one resolves.
        path::Cat::Worktrees | path::Cat::Submodules => Ok(None),
    }
}

fn friendly_to_volume_error(err: FriendlyGitError) -> VolumeError {
    // Carry the structured payload through the typed variant so the listing
    // pipeline's classifier ships the git kind as the `Git` reason and the FE
    // renders the git-specific copy. Using `VolumeError::PermissionDenied` for
    // the gitdir permission case would lose the git-specific repair copy (the
    // user would land on the generic "No permission" branch).
    VolumeError::FriendlyGit(err)
}
