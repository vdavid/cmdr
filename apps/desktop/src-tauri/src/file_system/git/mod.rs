//! Git browser. Ships repo detection, repo info, status, the `.git/*` watcher,
//! friendly-error mapping, and the virtual `.git` portal: `branches/`, `tags/`,
//! `commits/`, `stash/`, `worktrees/`, `submodules/` browsable as virtual
//! trees, with cross-volume copy "for free" because git blobs flow through the
//! existing `VolumeReadStream` abstraction. The portal root listing also
//! surfaces real `.git/*` entries (HEAD, config, hooks/, objects/, refs/, …)
//! alongside the virtual categories — the user sees everything in one place
//! and navigates real entries through the standard real-FS path.
//!
//! ## Volume hook contract
//!
//! `LocalPosixVolume` calls `git::try_route_*` after `resolve()`. Order is
//! load-bearing: `resolve` normalizes the absolute path, then we classify
//! against any enclosing `.git/`. If a virtual path matches we return its
//! result; otherwise (real `.git/*` entries, or paths outside any `.git/`)
//! the classifier returns `None` and the volume falls through to real-FS
//! code.
//!
//! All mutation methods short-circuit virtual paths via `path::is_virtual`
//! and return `VolumeError::NotSupported`. Git mutations happen out-of-band
//! (the user runs `git` in a terminal) and are surfaced through the
//! `.git`-watcher pipeline (`watch/watcher.rs`).
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
use std::sync::atomic::{AtomicBool, Ordering};

pub mod column_meta;
// `FriendlyGitError` moved to `cmdr-fs`: `VolumeError::FriendlyGit` carries it,
// and it maps onto `friendly_error::ErrorCategory`, so the two must live
// together. Aliased so `git::friendly::…` keeps resolving.
pub use cmdr_fs::volume::friendly_error::git as friendly;
pub mod log;
pub mod path;
pub mod portal;
pub mod read_blob;
pub mod repo;
pub mod snapshot_dates;
pub mod stash;
pub mod status;
pub mod submodules;
pub mod tree;
pub mod virtual_listing;
pub mod volume;
pub mod watcher;
pub mod worktrees;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod category_tests;
#[cfg(test)]
mod column_meta_tests;
#[cfg(test)]
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

#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use friendly::{FriendlyGitError, FriendlyGitErrorKind};
#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use repo::{RepoInfo, discover_repo, repo_info};
#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use status::{EntryStatus, EntryStatusCode, list_status};
#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use watcher::{GitWatcherRegistry, get_watcher_registry};

#[allow(unused_imports, reason = "Used by LocalPosixVolume mutation hooks")]
pub use path::is_virtual;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{VolumeError, VolumeReadStream};

/// The answer of a portal lookup that can legitimately find nothing.
///
/// `Ok(None)` means "that path isn't in this snapshot" (a typo in the path bar,
/// a file that only lives on another branch); the caller turns it into
/// `VolumeError::NotFound`. `Err` is reserved for a repo that couldn't answer
/// at all, which reaches the user as the git-specific repair copy.
pub type Lookup<T> = Result<Option<T>, FriendlyGitError>;

/// Whether the virtual `.git` portal is enabled. Set from the
/// `fileExplorer.git.showVirtualGitPortal` setting at startup and on every
/// toggle. When `false`, the volume hooks short-circuit to real-FS so
/// users see the raw `.git` contents.
static VIRTUAL_PORTAL_ENABLED: AtomicBool = AtomicBool::new(true);

/// Sets the virtual portal preference. Called from app setup after
/// loading settings, and live from the `set_show_virtual_git_portal`
/// command on each toggle.
pub fn set_virtual_portal_enabled(enabled: bool) {
    VIRTUAL_PORTAL_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Returns whether the virtual `.git` portal is enabled.
pub fn is_virtual_portal_enabled() -> bool {
    VIRTUAL_PORTAL_ENABLED.load(Ordering::Relaxed)
}

/// Volume hook for `list_directory`.
///
/// Returns `Some(result)` when the path lives under a virtual `.git/...`
/// portal; `None` when the caller should run real-FS code (real `.git/*`
/// entries like `HEAD`, `config`, `objects/`, etc., and paths outside any
/// `.git/`).
pub fn try_route_listing(path: &Path) -> Option<Result<Vec<FileEntry>, VolumeError>> {
    if !is_virtual_portal_enabled() {
        return None;
    }
    let (virt, handle, root) = path::classify(path)?;
    let result = match &virt {
        // The one listing the ROUTE and this hook answer differently: the portal
        // volume's namespace is the six categories, while `.git/` itself is a
        // mixed listing of real entries plus those six. Everything below it is
        // the volume's own body, so the two can't drift.
        path::VirtualGitPath::Root => Ok(Some(virtual_listing::list_root(&handle, &root))),
        deeper => volume::listing_for(deeper, &handle, &root),
    };
    Some(found_or_not_found(result, path))
}

/// Volume hook for `get_metadata`.
pub fn try_route_metadata(path: &Path) -> Option<Result<FileEntry, VolumeError>> {
    if !is_virtual_portal_enabled() {
        return None;
    }
    let (virt, handle, root) = path::classify(path)?;
    let result = virtual_listing::get_metadata_for(&root, &virt, &handle);
    Some(found_or_not_found(result, path))
}

/// Volume hook for `open_read_stream`. Returns `None` for paths that aren't
/// virtual blobs (real `.git/*` files fall through to the real-FS reader
/// via the volume hook returning `None`).
pub fn try_open_blob_stream(path: &Path) -> Option<Result<Box<dyn VolumeReadStream>, VolumeError>> {
    if !is_virtual_portal_enabled() {
        return None;
    }
    let (virt, handle, _root) = path::classify(path)?;
    use path::VirtualGitPath::*;
    let result = match &virt {
        RefTree(cat, name, sub) if cat.browses_commit_tree() => open_blob(&handle, *cat, name, sub),
        _ => return Some(Err(VolumeError::NotSupported)),
    };
    Some(found_or_not_found(result, path))
}

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
