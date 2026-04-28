//! Tauri commands for the git browser (M1).
//!
//! Thin pass-throughs over `file_system::git`. Every command is async and
//! wrapped with `blocking_with_timeout` so a hung NFS / SMB / FUSE mount can
//! never freeze the IPC thread.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;

use crate::commands::util::{TimedOut, blocking_with_timeout_flag};
use crate::file_system::git::{EntryStatus, RepoInfo, discover_repo, get_watcher_registry, list_status, repo_info};

/// Budget per the M1 plan: discover + repo info ≤ 50 ms p95 on a 50k-file
/// repo. We give the IPC layer 2 s to also cover slow NFS / SMB filesystems
/// where even a `stat` can stall.
const GIT_REPO_INFO_TIMEOUT: Duration = Duration::from_secs(2);
/// Status walks can take longer on huge worktrees. 5 s lets the chip stay
/// responsive without giving up before gix returns.
const GIT_STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// Returns the repo info for any path inside a worktree, or `None` if there's
/// no repo above it.
///
/// The frontend uses this on every navigation to populate the breadcrumb chip
/// (`subscribe_git_state` is the live channel; this is the one-shot variant).
#[tauri::command]
pub async fn get_git_repo_info(path: String) -> TimedOut<Option<RepoInfo>> {
    blocking_with_timeout_flag(GIT_REPO_INFO_TIMEOUT, None, move || {
        let path_buf = PathBuf::from(&path);
        let (handle, root) = discover_repo(&path_buf).ok()?;
        repo_info(&handle, &root).ok()
    })
    .await
}

/// Subscribes a frontend pane to live `git-state-changed` events for the repo
/// at `repo_root`. Returns the current `RepoInfo` synchronously so the chip
/// never sees an empty interim state.
#[tauri::command]
pub async fn subscribe_git_state(app: AppHandle, repo_root: String) -> Result<RepoInfo, GitCommandError> {
    let result = tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&repo_root);
        get_watcher_registry()
            .subscribe(app, &path)
            .map_err(GitCommandError::from_friendly)
    })
    .await;
    match result {
        Ok(Ok(info)) => Ok(info),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(GitCommandError {
            title: "We couldn't subscribe to repo updates".to_string(),
            message: "The git watcher task didn't finish in time. Try again in a moment.".to_string(),
        }),
    }
}

/// Drops one subscriber for the repo. The watcher itself stays alive until the
/// last subscriber unsubscribes.
#[tauri::command]
pub async fn unsubscribe_git_state(repo_root: String) {
    let _ = tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&repo_root);
        get_watcher_registry().unsubscribe(&path);
    })
    .await;
}

/// Returns the per-entry status for a worktree. The `dir` argument scopes the
/// caller's interest; today gix returns the whole worktree and the frontend
/// filters, but the parameter is here so M2 can scope properly without an IPC
/// shape change.
#[tauri::command]
pub async fn get_git_status_for_paths(repo_root: String, dir: String) -> TimedOut<Vec<EntryStatus>> {
    blocking_with_timeout_flag(GIT_STATUS_TIMEOUT, Vec::new(), move || {
        let root = PathBuf::from(&repo_root);
        let scope = PathBuf::from(&dir);
        let (handle, _root) = match discover_repo(&root) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        list_status(&handle, &scope).unwrap_or_default()
    })
    .await
}

/// Friendly-shaped error for git IPC commands. Mirrors the conventions of
/// `commands::util::IpcError` but carries the title separately so the
/// frontend can render category-styled messages.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommandError {
    pub title: String,
    pub message: String,
}

impl GitCommandError {
    fn from_friendly(err: crate::file_system::git::FriendlyGitError) -> Self {
        Self {
            title: err.title().to_string(),
            message: err.explanation().to_string(),
        }
    }
}

impl std::fmt::Display for GitCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.title, self.message)
    }
}
