// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These two can't go with them: `unused_crate_dependencies` is judged per
// compilation unit (as a package-wide flag every test target would report unused
// externs for deps only the lib uses), and `missing_docs` is this crate's own
// contract — its API is a deliverable rather than a side effect.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr knows about a git repository, with no app around it.
//!
//! Two jobs, one repository handle between them. The **chip** wants a
//! repository's mutable state: which branch, how far from upstream, dirty or
//! not, and per-entry status for the column. The **portal** turns the six things
//! a `.git` holds into browsable trees, so `branches/main/src/lib.rs` opens like
//! a file even though it's an object in a pack.
//!
//! It's a crate because `cargo check -p cmdr-git --all-targets` verifies both
//! with no app in the graph, so a sideways reach into the app is a compile error
//! rather than a convention. Nothing here names `tauri`, and nothing here holds
//! a user-facing word — the host renders every sentence from the typed values
//! this crate returns, `GitEntryMeta` included.
//!
//! ## The shape of the thing
//!
//! - [`GitPortal`] is the value that owns everything mutable: the open-repository
//!   cache, the per-repo `.git/*` watcher, and the [`GitStateSink`] that watcher
//!   reports through. The app parks one; a test builds its own.
//! - [`GitPortalVolume`] is a read-only `Volume` over one repository's virtual
//!   trees, built from an `Arc<GitPortal>` by its own constructor. A host routes
//!   to it and then knows nothing about git: listing, reading, and copying out
//!   all go through the trait.
//! - [`repo_info`] and [`list_status`] are the chip's two answers, callable with
//!   a [`RepoHandle`] alone.
//!
//! ## Two things a caller has to know
//!
//! **Routing is lexical, and it is the host's to do.** [`portal_route`] is pure
//! string work over path segments: no `stat`, no repository open, because it runs
//! on every path-bearing call. Whether that `.git` is a directory, a linked
//! worktree's gitlink file, or not a repository at all is the volume's question,
//! answered once on first use. A path that isn't in a snapshot comes back as
//! `VolumeError::NotFound`, ❌ never as a damaged repository.
//!
//! **The `.git/` landing listing is NOT this crate's.** The six category rows
//! reach a PANE and nothing else, through the host's own listing overlay, which
//! calls [`GitPortal::category_rows`]. The moment a copy scan or a delete walker
//! can see a row with no inode behind it, a repo delete stops half-way with
//! `.git/` still on disk. That was a real bug.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the boundary's
//! rationale and the capped surface.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_git` an extern crate of
// its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_git as _;

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::friendly_error::git::FriendlyGitError;
use cmdr_fs::volume::{VolumeError, VolumeReadStream};

mod column_meta;
mod log;
mod path;
mod portal;
mod read_blob;
mod repo;
mod snapshot_dates;
mod stash;
mod state_sink;
mod status;
mod submodules;
mod tree;
mod virtual_listing;
mod volume;
mod watcher;
mod worktrees;

// ❌ No outer `///` here: it would concatenate with the file's own `//!` header
// and resolve that header's intra-doc links in THIS scope, where none of them
// exist. The module documents itself.
#[cfg(any(test, feature = "testing"))]
pub mod test_fixtures;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod category_tests;
#[cfg(test)]
mod column_meta_tests;
#[cfg(test)]
mod repo_tests;
#[cfg(test)]
mod snapshot_dates_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod tree_tests;
#[cfg(test)]
mod volume_tests;

pub use path::{portal_route, virtual_category_prefixes};
pub use portal::GitPortal;
pub use repo::{RepoHandle, RepoInfo, repo_info};
pub use state_sink::{GitStateSink, no_git_state_sink};
pub use status::{EntryStatus, EntryStatusCode, list_status};
pub use volume::GitPortalVolume;

/// A [`GitStateSink`] that remembers what it was told, so a test can assert on
/// what a subscriber would have seen.
///
/// ❌ Not `cfg(test)` alone: that's set only while a crate compiles its OWN test
/// target, so a consumer's test build would see the recorder vanish.
#[cfg(any(test, feature = "testing"))]
pub use state_sink::RecordingGitStateSink;

/// The answer of a portal lookup that can legitimately find nothing.
///
/// `Ok(None)` means "that path isn't in this snapshot" (a typo in the path bar,
/// a file that only lives on another branch); the caller turns it into
/// `VolumeError::NotFound`. `Err` is reserved for a repo that couldn't answer
/// at all, which reaches the user as the git-specific repair copy.
pub(crate) type Lookup<T> = Result<Option<T>, FriendlyGitError>;

/// Opens the blob at `sub` inside `cat`/`name`, or `Ok(None)` when that path
/// holds no blob (missing, or a directory).
fn open_blob(handle: &RepoHandle, cat: path::Cat, name: &str, sub: &str) -> Lookup<Box<dyn VolumeReadStream>> {
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

fn list_ref_tree(handle: &RepoHandle, root: &Path, cat: path::Cat, name: &str, sub: &str) -> Lookup<Vec<FileEntry>> {
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
pub(crate) fn resolve_commit_for_cat(handle: &RepoHandle, cat: path::Cat, name: &str) -> Lookup<gix::ObjectId> {
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
