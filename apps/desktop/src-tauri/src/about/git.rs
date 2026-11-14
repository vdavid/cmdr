//! The git row: the repo around an item, its branch, and what git's status says
//! about the item.
//!
//! The status comes from `cmdr_git::path_status`, which answers the one path: tracked and
//! clean, changed, untracked, or ignored. Only its `Clean` becomes "committed, no changes"
//! (and the removal row's "git can restore it"); a folder git holds only part of (ignored
//! files beside its tracked ones, a submodule) is `Unknown(NotInStatus)`, ❌ never clean.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::payload::{GitContext, GitStatus, TrackedState};
use cmdr_about::text::ItemText;
use cmdr_git::{PathStatus, RepoHandle};

use super::session::SubJobs;
use crate::file_system::framework_pool::note_activity;
use crate::file_system::git::{FriendlyGitError, FriendlyGitErrorKind};

/// How long the row waits for the item's status inside its own budget. It reads the
/// index and walks the one path (a folder's subtree), so a file answers in
/// milliseconds; past this the root and branch stand and the status says it timed out.
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

/// The row for the item at `path` in `repo`. Its status walk goes to `sub_jobs`, so a
/// card gone before a worker picks it up never walks.
pub(crate) fn git_fact(repo: &RepoAround, path: &Path, is_folder: bool, sub_jobs: &SubJobs) -> Fact<GitContext> {
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
        Some(relative) => status_within(handle, &relative, is_folder, STATUS_BUDGET, sub_jobs),
        // The item sits outside the worktree it was found from (a path through a
        // symlink): nothing the repo says is about it.
        None => Fact::unknown(UnknownWhy::NotInStatus),
    };
    Fact::Known(GitContext {
        repo_root: ItemText::new(&root.to_string_lossy()),
        branch: info.branch.as_deref().map(ItemText::new),
        status,
    })
}

/// The item's status, from its own pool job, waited for at most `budget`. A read
/// still going then answers into nothing.
fn status_within(
    handle: &RepoHandle,
    relative: &str,
    is_folder: bool,
    budget: Duration,
    sub_jobs: &SubJobs,
) -> Fact<GitStatus> {
    let (answer, waiting) = mpsc::sync_channel(1);
    let handle = handle.clone();
    let relative = relative.to_string();
    sub_jobs.submit(Box::new(move || {
        note_activity("read the item's git status", "git");
        let _ = answer.send(cmdr_git::path_status(&handle, &relative, is_folder));
    }));
    match waiting.recv_timeout(budget) {
        Ok(Ok(status)) => status_from(status),
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

/// The card's word for what git says about the item. Only `Clean` is "committed, no
/// changes", so only it lets the removal row say git can restore it; a folder git holds
/// only part of is `NotInStatus`, ❌ never clean. Every other change (a staged add or
/// delete, a rename, a conflict, untracked files in a tracked folder) is changes no commit
/// has, worded from the side that never understates a loss.
pub(crate) fn status_from(status: PathStatus) -> Fact<GitStatus> {
    match status {
        PathStatus::Clean => Fact::Known(GitStatus::Tracked(TrackedState::Clean)),
        PathStatus::Changed => Fact::Known(GitStatus::Tracked(TrackedState::Modified)),
        PathStatus::Untracked => Fact::Known(GitStatus::Untracked),
        PathStatus::Ignored => Fact::Known(GitStatus::Ignored),
        PathStatus::TrackedInPart => Fact::unknown(UnknownWhy::NotInStatus),
    }
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
