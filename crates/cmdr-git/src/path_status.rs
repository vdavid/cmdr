//! What git says about ONE path: tracked and clean, changed, untracked, or ignored.
//!
//! [`list_status`](crate::list_status) lists CHANGED entries only, so absence there can mean
//! clean, ignored, or outside the repo. This answers the single path instead: an index lookup
//! says whether git tracks it, the ignore stack (`.gitignore`, `info/exclude`, the global
//! file) says whether a rule covers an untracked one, and a status walk limited to the path
//! by a literal pathspec says whether a tracked one changed. No full-repo walk, no cache.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gix::bstr::{BStr, BString, ByteSlice as _};

use crate::fault_guard::GuardedRead;
use crate::repo::RepoHandle;
use crate::status::{EntryStatusCode, index_worktree_to_entry, tree_index_to_entry};
use cmdr_fs::volume::git_error::{FriendlyGitError, FriendlyGitErrorKind};

/// What git says about one path in a worktree ([`path_status`]).
///
/// ❗ Only `Clean` says git holds every byte of it: a host may tell a person git can bring a
/// `Clean` item back, and ❌ nothing else. A folder is `Clean` only when every tracked file under
/// it matches the last commit AND nothing untracked or ignored sits beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStatus {
    /// Tracked, and identical to the last commit (for a folder: all of it, with nothing else
    /// inside).
    Clean,
    /// Tracked, with changes no commit holds, staged or not; or a tracked folder holding
    /// untracked files.
    Changed,
    /// Git doesn't track it, and no ignore rule covers it or a folder above it.
    Untracked,
    /// Git doesn't track it, and an ignore rule covers it or a folder above it.
    Ignored,
    /// Tracked, but git doesn't hold all of it: a folder whose tracked files match the last
    /// commit and that also holds ignored files, or a submodule, whose own worktree is another
    /// repo's.
    TrackedInPart,
}

/// What git says about `relative` (`/`-separated, relative to the worktree root; empty for
/// the root itself) in `repo`. `is_folder` says whether the item is a folder, which decides
/// how the index and the ignore rules read it.
///
/// Blocking: an index read, the ignore files, and for a tracked item a status walk limited
/// to that path. Run it on a blocking thread.
pub fn path_status(repo: &RepoHandle, relative: &str, is_folder: bool) -> Result<PathStatus, FriendlyGitError> {
    let local = repo.to_thread_local();
    let work_dir = local
        .workdir()
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::BareRepo, ""))?
        .to_path_buf();
    let canonical_root = work_dir.canonicalize().unwrap_or(work_dir);
    let read = GuardedRead::begin(&canonical_root);
    let relative = relative.trim_matches('/');
    read.vouch(classify(&local, relative, is_folder, &read.stop_flag()))
}

fn classify(
    repo: &gix::Repository,
    relative: &str,
    is_folder: bool,
    stop: &Arc<AtomicBool>,
) -> Result<PathStatus, FriendlyGitError> {
    let index = repo.index_or_empty().map_err(corrupt)?;
    let path: &BStr = relative.as_bytes().as_bstr();
    let own_entry = index.entry_by_path(path);
    if own_entry.is_some_and(|entry| entry.mode == gix::index::entry::Mode::COMMIT) {
        // A submodule: its commit is the outer repo's, its files another repo's.
        let dirty = crate::repo::dirty_submodule_paths(repo, stop)
            .iter()
            .any(|p| p == relative);
        return Ok(if dirty || walk(repo, relative, stop)?.changed {
            PathStatus::Changed
        } else {
            PathStatus::TrackedInPart
        });
    }
    let tracked = if is_folder {
        relative.is_empty() && !index.entries().is_empty()
            || index
                .prefixed_entries(format!("{relative}/").as_bytes().as_bstr())
                .is_some_and(|e| !e.is_empty())
    } else {
        own_entry.is_some()
    };
    if !tracked {
        return Ok(if is_ignored(repo, &index, relative, is_folder)? {
            PathStatus::Ignored
        } else {
            PathStatus::Untracked
        });
    }
    let seen = walk(repo, relative, stop)?;
    Ok(if seen.changed || (is_folder && seen.untracked) {
        PathStatus::Changed
    } else if is_folder && seen.ignored {
        PathStatus::TrackedInPart
    } else {
        PathStatus::Clean
    })
}

/// Whether an ignore rule covers `relative` or any folder above it. Once git ignores a
/// folder, nothing below it can be un-ignored, so the first covered folder answers.
fn is_ignored(
    repo: &gix::Repository,
    index: &gix::index::State,
    relative: &str,
    is_folder: bool,
) -> Result<bool, FriendlyGitError> {
    if relative.is_empty() {
        return Ok(false);
    }
    let mut stack = repo
        .excludes(
            index,
            None,
            gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
        )
        .map_err(corrupt)?;
    let parts: Vec<&str> = relative.split('/').collect();
    for end in 1..=parts.len() {
        let prefix = parts[..end].join("/");
        let folder = end < parts.len() || is_folder;
        let mode = folder.then_some(gix::index::entry::Mode::DIR);
        let platform = stack
            .at_entry(prefix.as_bytes().as_bstr(), mode)
            .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
        if platform.is_excluded() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What a status walk limited to `relative` saw.
#[derive(Default)]
struct Seen {
    /// A tracked change: staged or not, a conflict, a deletion.
    changed: bool,
    untracked: bool,
    ignored: bool,
}

/// One status walk over `relative` alone (everything under it, for a folder), by a LITERAL
/// pathspec, so a `*` or `?` in a name is a character, ❌ never a glob.
fn walk(repo: &gix::Repository, relative: &str, stop: &Arc<AtomicBool>) -> Result<Seen, FriendlyGitError> {
    let work_dir = repo
        .workdir()
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::BareRepo, ""))?;
    let platform = repo
        .status(gix::progress::Discard)
        .map_err(corrupt)?
        .should_interrupt_owned(Arc::clone(stop))
        .untracked_files(gix::status::UntrackedFiles::Collapsed)
        .index_worktree_submodules(crate::repo::submodule_status(false))
        .dirwalk_options(|options| options.emit_ignored(Some(gix::dir::walk::EmissionMode::CollapseDirectory)));
    let spec: Vec<BString> = if relative.is_empty() {
        Vec::new()
    } else {
        vec![BString::from(format!(":(literal){relative}"))]
    };
    let mut seen = Seen::default();
    for item in platform.into_iter(spec).map_err(corrupt)? {
        let item = item.map_err(corrupt)?;
        let entry = match &item {
            gix::status::Item::IndexWorktree(change) => index_worktree_to_entry(change, work_dir),
            gix::status::Item::TreeIndex(change) => tree_index_to_entry(change),
        };
        let Some((path, code)) = entry else { continue };
        if !concerns(relative, &path) {
            continue;
        }
        match code {
            EntryStatusCode::Untracked => seen.untracked = true,
            EntryStatusCode::Ignored => seen.ignored = true,
            _ => seen.changed = true,
        }
    }
    Ok(seen)
}

/// Whether a walk's `path` is about `relative`: the item itself, something under it, or a
/// collapsed folder above it. A sibling sharing its name's start (`sub-other` beside `sub`)
/// isn't.
fn concerns(relative: &str, path: &str) -> bool {
    let path = path.trim_end_matches('/');
    relative.is_empty()
        || path == relative
        || path.strip_prefix(relative).is_some_and(|rest| rest.starts_with('/'))
        || relative.strip_prefix(path).is_some_and(|rest| rest.starts_with('/'))
}

fn corrupt<E: std::error::Error + Send + Sync + 'static>(e: E) -> FriendlyGitError {
    FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e)
}

#[cfg(test)]
#[path = "path_status_tests.rs"]
mod tests;
