//! Git browser foundation (M1) + virtual `.git` portal (M2).
//!
//! M1 ships repo detection, repo info, status, the `.git/*` watcher, and
//! friendly-error mapping. M2 adds the virtual portal: `branches/`,
//! `tags/`, `raw/` browsable as virtual trees, with cross-volume copy
//! "for free" because git blobs flow through the existing `VolumeReadStream`
//! abstraction.
//!
//! ## Volume hook contract (M2)
//!
//! `LocalPosixVolume` calls `git::try_route_*` after `resolve()`. Order is
//! load-bearing: `resolve` normalizes the absolute path, then we classify
//! against any enclosing `.git/`. If a virtual path matches we return its
//! result; otherwise the volume falls through to real-FS code.
//!
//! All mutation methods short-circuit virtual paths via `path::is_virtual`
//! and return `VolumeError::NotSupported`. Git mutations happen out-of-band
//! (the user runs `git` in a terminal) and are surfaced through the
//! `.git`-watcher pipeline (`watcher.rs`).
//!
//! ## Honest blob streaming
//!
//! gix in 0.81 returns whole-blob `Vec<u8>` for `Object::data`. The
//! `GitBlobReadStream` owns that vec and yields 256 KB chunks for the
//! consumer API shape — memory cost equals blob size. We refuse blobs
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

pub mod friendly;
pub mod log;
pub mod path;
pub mod read_blob;
pub mod repo;
pub mod stash;
pub mod status;
pub mod submodules;
pub mod tree;
pub mod virtual_listing;
pub mod watcher;
pub mod worktrees;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod m2_tests;
#[cfg(test)]
mod m3_tests;
#[cfg(test)]
mod tests;

#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M3 modules"
)]
pub use friendly::{FriendlyGitError, FriendlyGitErrorKind};
#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M3 modules"
)]
pub use repo::{RepoInfo, discover_repo, repo_info};
#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M3 modules"
)]
pub use status::{EntryStatus, EntryStatusCode, list_status};
#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M3 modules"
)]
pub use watcher::{GitWatcherRegistry, get_watcher_registry};

#[allow(unused_imports, reason = "Used by LocalPosixVolume mutation hooks")]
pub use path::is_virtual;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{VolumeError, VolumeReadStream};

/// Volume hook for `list_directory`.
///
/// Returns `Some(result)` when the path lives under a virtual `.git/...`
/// portal; `None` when the caller should run real-FS code.
pub fn try_route_listing(path: &Path) -> Option<Result<Vec<FileEntry>, VolumeError>> {
    let (virt, handle, root) = path::classify(path)?;
    use path::VirtualGitPath::*;
    let result = match &virt {
        Root => Ok(virtual_listing::list_root(&root)),
        Category(path::Cat::Branches) => virtual_listing::list_branches(&handle, &root),
        Category(path::Cat::Tags) => virtual_listing::list_tags(&handle, &root),
        Category(path::Cat::Commits) => log::list_commits(&handle, &root),
        Category(path::Cat::Stash) => stash::list_stashes(&handle, &root),
        Category(path::Cat::Worktrees) => worktrees::list_worktrees(&handle, &root),
        Category(path::Cat::Submodules) => submodules::list_submodules(&handle, &root),
        Category(path::Cat::Raw) => virtual_listing::list_raw(&root, ""),
        Ref(cat, name) if cat.browses_commit_tree() => list_ref_tree(&handle, &root, *cat, name, ""),
        RefTree(cat, name, sub) if cat.browses_commit_tree() => list_ref_tree(&handle, &root, *cat, name, sub),
        // Worktrees and submodules are leaf entries with `redirectToPath`;
        // listing them as if they were directories returns empty (the
        // frontend redirects on Enter so this rarely fires in practice).
        Ref(_, _) | RefTree(_, _, _) => Ok(Vec::new()),
        Raw(sub) => virtual_listing::list_raw(&root, sub),
    };
    Some(result.map_err(friendly_to_volume_error))
}

/// Volume hook for `get_metadata`.
pub fn try_route_metadata(path: &Path) -> Option<Result<FileEntry, VolumeError>> {
    let (virt, handle, root) = path::classify(path)?;
    let result = virtual_listing::get_metadata_for(&root, &virt, &handle);
    Some(result.map_err(friendly_to_volume_error))
}

/// Volume hook for `open_read_stream`. Returns `None` for paths that aren't
/// virtual blobs.
pub fn try_open_blob_stream(path: &Path) -> Option<Result<Box<dyn VolumeReadStream>, VolumeError>> {
    let (virt, handle, root) = path::classify(path)?;
    use path::VirtualGitPath::*;
    let result = match &virt {
        RefTree(cat, name, sub) if cat.browses_commit_tree() => {
            let commit_id = match resolve_commit_for_cat(&handle, *cat, name) {
                Ok(id) => id,
                Err(e) => return Some(Err(friendly_to_volume_error(e))),
            };
            let blob_id = match tree::lookup_blob_id(&handle, commit_id, sub) {
                Ok(id) => id,
                Err(e) => return Some(Err(friendly_to_volume_error(e))),
            };
            tree::read_blob(&handle, blob_id)
                .map(|bytes| Box::new(read_blob::GitBlobReadStream::new(bytes)) as Box<dyn VolumeReadStream>)
        }
        Raw(sub) if !sub.is_empty() => {
            // Real-FS file under .git/raw/...
            let real = virtual_listing::real_gitdir_path(&root, sub);
            return Some(open_real_file_stream(&real));
        }
        _ => return Some(Err(VolumeError::NotSupported)),
    };
    Some(result.map_err(friendly_to_volume_error))
}

fn list_ref_tree(
    handle: &repo::RepoHandle,
    root: &Path,
    cat: path::Cat,
    name: &str,
    sub: &str,
) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let commit_id = resolve_commit_for_cat(handle, cat, name)?;
    let display_parent = root
        .join(".git")
        .join(cat.as_segment())
        .join(name)
        .join(sub.replace('/', std::path::MAIN_SEPARATOR_STR));
    tree::list_tree(handle, commit_id, sub, &display_parent)
}

/// Resolves a `Cat::* / name` pair to the commit ID whose tree we should
/// browse. Branches/tags peel through refs (M2), commits resolve the SHA
/// prefix, stash resolves through `stash@{n}`.
pub(crate) fn resolve_commit_for_cat(
    handle: &repo::RepoHandle,
    cat: path::Cat,
    name: &str,
) -> Result<gix::ObjectId, FriendlyGitError> {
    match cat {
        path::Cat::Branches | path::Cat::Tags => virtual_listing::resolve_ref_commit(handle, cat, name),
        path::Cat::Commits => log::resolve_commit_id(handle, name),
        path::Cat::Stash => {
            let n: usize = name
                .parse()
                .map_err(|_| FriendlyGitError::new(FriendlyGitErrorKind::CorruptRepo, name.to_string()))?;
            stash::resolve_stash_commit(handle, n)
        }
        _ => Err(FriendlyGitError::new(
            FriendlyGitErrorKind::CorruptRepo,
            name.to_string(),
        )),
    }
}

fn open_real_file_stream(real: &Path) -> Result<Box<dyn VolumeReadStream>, VolumeError> {
    let bytes = std::fs::read(real).map_err(VolumeError::from)?;
    Ok(Box::new(read_blob::GitBlobReadStream::new(bytes)) as Box<dyn VolumeReadStream>)
}

fn friendly_to_volume_error(err: FriendlyGitError) -> VolumeError {
    match err.kind {
        FriendlyGitErrorKind::NotARepo
        | FriendlyGitErrorKind::CorruptRepo
        | FriendlyGitErrorKind::OrphanedWorktree
        | FriendlyGitErrorKind::IndexLocked
        | FriendlyGitErrorKind::BareRepo
        | FriendlyGitErrorKind::BlobTooLarge => VolumeError::IoError {
            message: err.to_string(),
            raw_os_error: None,
        },
        FriendlyGitErrorKind::PermissionDenied => VolumeError::PermissionDenied(err.path.clone()),
    }
}
