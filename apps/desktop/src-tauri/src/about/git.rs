//! The git row: the repo around an item, its branch, and what git's status says
//! about the item.
//!
//! The status comes from `cmdr_git::list_status`, which lists CHANGED entries
//! only: a clean tracked file, an ignored one, and one git never saw are all
//! absent from it. So the row answers what the list proves (modified, untracked)
//! and `Unknown(NotInStatus)` otherwise, ❌ never "clean" from absence. A per-path
//! read that tells those apart would replace only [`status_of`]'s source.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::payload::{GitContext, GitStatus, TrackedState};
use cmdr_about::text::ItemText;
use cmdr_git::RepoHandle;

use crate::file_system::framework_pool::note_activity;
use crate::file_system::git::{EntryStatus, EntryStatusCode, FriendlyGitError, FriendlyGitErrorKind};

/// How long the row waits for the status walk inside its own budget. A warm
/// cache answers in microseconds and a cold walk of a 50,000-file repo in about
/// 70 ms (`crates/cmdr-git/DETAILS.md` § Performance); past this the root and
/// branch stand and the status says it timed out. The walk itself runs on and
/// warms the cache, so the next card answers.
pub(crate) const STATUS_BUDGET: Duration = Duration::from_millis(600);

/// The repo around an item, from the look before the plan.
#[derive(Clone)]
pub(crate) enum RepoAround {
    /// No repo holds it (or only a bare one, which has no worktree to hold it).
    None,
    /// The repo's handle and its canonical worktree root.
    Found { handle: RepoHandle, root: PathBuf },
    /// Something is there and git couldn't read it.
    Unreadable(UnknownWhy),
}

impl std::fmt::Debug for RepoAround {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoAround::None => f.write_str("None"),
            RepoAround::Found { .. } => f.write_str("Found"),
            RepoAround::Unreadable(why) => write!(f, "Unreadable({why:?})"),
        }
    }
}

impl RepoAround {
    /// Whether the card has a git row.
    pub(crate) fn is_some(&self) -> bool {
        !matches!(self, RepoAround::None)
    }
}

/// Looks for the repo around `path` (a folder's own, or a file's parent's),
/// through the app's repo cache. Blocking: call it on the about pool.
pub(crate) fn repo_around(path: &Path, is_folder: bool) -> RepoAround {
    let start = if is_folder { Some(path) } else { path.parent() };
    let Some(start) = start else {
        return RepoAround::None;
    };
    match crate::file_system::git::wiring::portal().discover(start) {
        Ok((handle, root)) => RepoAround::Found { handle, root },
        Err(error) => match error.kind {
            FriendlyGitErrorKind::NotARepo | FriendlyGitErrorKind::BareRepo => RepoAround::None,
            _ => RepoAround::Unreadable(why_of(&error)),
        },
    }
}

/// The row for the item at `path` in `repo`.
pub(crate) fn git_fact(repo: &RepoAround, path: &Path, is_folder: bool) -> Fact<GitContext> {
    let (handle, root) = match repo {
        RepoAround::Found { handle, root } => (handle, root),
        RepoAround::Unreadable(why) => return Fact::unknown(*why),
        // The plan has no row then; "no record" if something asked anyway.
        RepoAround::None => return Fact::unknown(UnknownWhy::NoRecord),
    };
    note_activity("read the repo's head", "git");
    let info = match crate::file_system::git::repo_info(handle, root) {
        Ok(info) => info,
        Err(error) => return Fact::unknown(why_of(&error)),
    };
    let status = match relative_path(root, path) {
        Some(relative) => status_within(handle, root, &relative, is_folder, STATUS_BUDGET),
        // The item sits outside the worktree it was found from (a path through a
        // symlink): nothing in the repo's list is about it.
        None => Fact::unknown(UnknownWhy::NotInStatus),
    };
    Fact::Known(GitContext {
        repo_root: ItemText::new(&root.to_string_lossy()),
        branch: info.branch.as_deref().map(ItemText::new),
        status,
    })
}

/// The item's status, from a walk on its own pool job, waited for at most
/// `budget`. A walk still going then answers into nothing.
fn status_within(
    handle: &RepoHandle,
    root: &Path,
    relative: &str,
    is_folder: bool,
    budget: Duration,
) -> Fact<GitStatus> {
    let (answer, waiting) = mpsc::sync_channel(1);
    let handle = handle.clone();
    let root = root.to_path_buf();
    super::pool::submit(Box::new(move || {
        note_activity("walk the repo's status", "git");
        let _ = answer.send(crate::file_system::git::list_status(&handle, &root));
    }));
    match waiting.recv_timeout(budget) {
        Ok(Ok(entries)) => status_of(&entries, relative, is_folder),
        Ok(Err(error)) => Fact::unknown(why_of(&error)),
        Err(mpsc::RecvTimeoutError::Timeout) => Fact::unknown(UnknownWhy::TimedOut),
        // The walk panicked: no answer is coming.
        Err(mpsc::RecvTimeoutError::Disconnected) => Fact::unknown(UnknownWhy::Unparseable),
    }
}

/// `path` relative to the canonical `root`, `/`-separated, empty for the root
/// itself. The parent is canonicalized and the name kept, so a symlink is asked
/// about as itself, never as what it points at.
fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let canonical = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent.canonicalize().ok()?.join(name),
        // `/` itself.
        _ => path.canonicalize().ok()?,
    };
    let relative = canonical.strip_prefix(root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

/// What git's changed-entries list proves about the item at `relative`.
///
/// - A file: its own entry. Untracked is `Untracked`; every other change (a
///   staged add or delete, a rename, a conflict) is changes no commit has, worded
///   from the more painful side so the removal row never understates a loss.
/// - A folder: the entries under it. Any tracked change makes it hold uncommitted
///   work. Untracked entries alone don't say whether git tracks the rest of it, so
///   that's `NotInStatus`, as is a folder with no entry at all.
/// - Absent from the list: `NotInStatus`, ❌ never `Clean`.
pub(crate) fn status_of(entries: &[EntryStatus], relative: &str, is_folder: bool) -> Fact<GitStatus> {
    let prefix = format!("{relative}/");
    let about_it = |entry: &&EntryStatus| {
        if is_folder {
            relative.is_empty() || entry.relative_path.starts_with(&prefix)
        } else {
            entry.relative_path == relative
        }
    };
    let codes: Vec<EntryStatusCode> = entries.iter().filter(about_it).map(|entry| entry.code).collect();
    let tracked_change = codes
        .iter()
        .any(|code| !matches!(code, EntryStatusCode::Untracked | EntryStatusCode::Ignored));
    if tracked_change {
        return Fact::Known(GitStatus::Tracked(TrackedState::Modified));
    }
    if !is_folder && codes.contains(&EntryStatusCode::Untracked) {
        return Fact::Known(GitStatus::Untracked);
    }
    if !is_folder && codes.contains(&EntryStatusCode::Ignored) {
        return Fact::Known(GitStatus::Ignored);
    }
    Fact::unknown(UnknownWhy::NotInStatus)
}

/// Why git couldn't answer, by its typed kind, ❌ never its message.
fn why_of(error: &FriendlyGitError) -> UnknownWhy {
    match error.kind {
        FriendlyGitErrorKind::VolumeGone => UnknownWhy::Unreachable,
        FriendlyGitErrorKind::PermissionDenied | FriendlyGitErrorKind::GitDirPermissionDenied => {
            UnknownWhy::PermissionDenied
        }
        FriendlyGitErrorKind::NotARepo
        | FriendlyGitErrorKind::OrphanedWorktree
        | FriendlyGitErrorKind::CorruptRepo
        | FriendlyGitErrorKind::IndexLocked
        | FriendlyGitErrorKind::BareRepo
        | FriendlyGitErrorKind::BlobTooLarge
        | FriendlyGitErrorKind::ShallowBoundary
        | FriendlyGitErrorKind::MissingObject => UnknownWhy::Unparseable,
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
