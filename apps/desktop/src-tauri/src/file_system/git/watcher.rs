//! Per-repo watcher: subscribes to `.git/*` mutable-state paths and re-emits
//! `RepoInfo` whenever they change.
//!
//! Frontend never polls. The chip subscribes once via `subscribe_git_state`
//! and updates reactively from `git-state-changed` events. Debounce is 200 ms
//! per repo, matching the existing listing watcher in `file_system/listing/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::repo::{RepoInfo, discover_repo, repo_info};

/// Tauri event payload for `git-state-changed`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStateChangedPayload {
    pub repo_root: String,
    pub info: RepoInfo,
}

/// One per repo. Owns the notify-rs debouncer and the subscriber count.
struct Subscription {
    refcount: u32,
    /// Keep the debouncer alive so the watcher thread doesn't stop.
    /// Stored as `dyn Drop` because `notify_debouncer_full::Debouncer` is
    /// generic over the watcher impl and we don't want to leak that here.
    _debouncer: Box<dyn DropAny + Send>,
}

/// Type-erased drop helper.
trait DropAny {}
impl<T> DropAny for T {}

/// App-wide registry of per-repo subscriptions.
pub struct GitWatcherRegistry {
    inner: Mutex<HashMap<PathBuf, Subscription>>,
}

impl GitWatcherRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Adds a subscriber for `repo_root`. Spawns the watcher on first call,
    /// bumps the refcount on subsequent ones. Returns the current `RepoInfo`
    /// snapshot synchronously so the chip never sees an empty interim state.
    pub fn subscribe(&self, app: AppHandle, repo_root: &Path) -> Result<RepoInfo, super::FriendlyGitError> {
        let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());

        let (handle, root) = discover_repo(&canonical)?;
        let info = repo_info(&handle, &root)?;

        let mut inner = self.inner.lock().expect("git watcher mutex poisoned");
        if let Some(sub) = inner.get_mut(&root) {
            sub.refcount = sub.refcount.saturating_add(1);
            return Ok(info);
        }

        // First subscriber: start the debouncer.
        let watcher_root = root.clone();
        let app_for_cb = app.clone();
        let mut debouncer = new_debouncer(Duration::from_millis(200), None, move |result: DebounceEventResult| {
            if result.is_err() {
                return;
            }
            recompute_and_emit(&app_for_cb, &watcher_root);
        })
        .map_err(|e| {
            super::FriendlyGitError::with_source(super::FriendlyGitErrorKind::CorruptRepo, e.to_string(), e)
        })?;

        for path in watch_paths(&root) {
            // Some paths (`refs/`) are dirs, others (`HEAD`, `index`) are files.
            // notify happily handles both. Missing paths are common (no MERGE_HEAD
            // until a merge starts) — we register watches lazily by watching the
            // `.git` dir non-recursively as a fallback so create-then-modify still fires.
            if path.exists() {
                let mode = if path.is_dir() {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                let _ = debouncer.watch(&path, mode);
            }
        }
        // Always watch `.git` itself for create events on optional files.
        let dot_git = git_dir_path(&root);
        if dot_git.exists() {
            let _ = debouncer.watch(&dot_git, RecursiveMode::NonRecursive);
        }

        inner.insert(
            root.clone(),
            Subscription {
                refcount: 1,
                _debouncer: Box::new(debouncer) as Box<dyn DropAny + Send>,
            },
        );
        Ok(info)
    }

    /// Drops a subscriber. Tears the watcher down on the last unsubscribe.
    pub fn unsubscribe(&self, repo_root: &Path) {
        let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        let mut inner = self.inner.lock().expect("git watcher mutex poisoned");
        if let Some(sub) = inner.get_mut(&canonical) {
            sub.refcount = sub.refcount.saturating_sub(1);
            if sub.refcount == 0 {
                inner.remove(&canonical);
                super::repo::evict_handle(&canonical);
            }
        }
    }

    /// For tests: count active repos.
    #[cfg(test)]
    #[allow(dead_code, reason = "Used by integration tests")]
    pub fn active_repo_count(&self) -> usize {
        self.inner.lock().map(|i| i.len()).unwrap_or(0)
    }
}

impl Default for GitWatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-global watcher registry. Mirrors the global volume / listing
/// patterns elsewhere in the codebase.
pub fn get_watcher_registry() -> &'static GitWatcherRegistry {
    static REG: OnceLock<GitWatcherRegistry> = OnceLock::new();
    REG.get_or_init(GitWatcherRegistry::new)
}

fn recompute_and_emit(app: &AppHandle, repo_root: &Path) {
    let Ok((handle, root)) = discover_repo(repo_root) else {
        return;
    };
    let Ok(info) = repo_info(&handle, &root) else {
        return;
    };
    let payload = GitStateChangedPayload {
        repo_root: root.display().to_string(),
        info,
    };
    let _ = app.emit("git-state-changed", payload);
}

/// Returns the gitdir for a worktree (handles gitlink files).
fn git_dir_path(repo_root: &Path) -> PathBuf {
    let dot_git = repo_root.join(".git");
    if dot_git.is_file() {
        // gitlink: contents look like `gitdir: <path>` (relative or absolute).
        if let Ok(content) = std::fs::read_to_string(&dot_git)
            && let Some(stripped) = content.trim().strip_prefix("gitdir:")
        {
            let p = stripped.trim();
            let path = if Path::new(p).is_absolute() {
                PathBuf::from(p)
            } else {
                repo_root.join(p)
            };
            return path;
        }
    }
    dot_git
}

/// The set of paths inside `.git` whose changes should trigger a re-emit.
/// See plan § Architecture > Watcher.
fn watch_paths(repo_root: &Path) -> Vec<PathBuf> {
    let git_dir = git_dir_path(repo_root);
    [
        "HEAD",
        "ORIG_HEAD",
        "MERGE_HEAD",
        "FETCH_HEAD",
        "packed-refs",
        "index",
        "refs",
        "logs/HEAD",
    ]
    .iter()
    .map(|sub| git_dir.join(sub))
    .collect()
}
