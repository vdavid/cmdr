//! Per-repo watcher: subscribes to `.git/*` mutable-state paths and recomputes
//! `RepoInfo` whenever they change.
//!
//! Frontend never polls. The chip subscribes once and updates reactively from
//! whatever the host does with a [`GitStateSink`] report. Debounce is 200 ms per
//! repo, matching the existing listing watcher in `file_system/listing/`.
//!
//! ❌ Nothing here names a window. The watcher recomputes a typed snapshot and
//! reports it; wording it and refreshing panes is the host's, through the sink.

#[cfg(test)]
use crate::ignore_poison::IgnorePoison;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};

use super::repo::{RepoCache, RepoInfo, repo_info};
use super::state_sink::GitStateSink;

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
    ///
    /// `repos` is shared rather than borrowed because the debounce callback
    /// outlives this call: every recompute opens the repository through the same
    /// cache the caller's own lookups use.
    pub fn subscribe(
        &self,
        repos: Arc<RepoCache>,
        sink: Arc<dyn GitStateSink>,
        repo_root: &Path,
    ) -> Result<RepoInfo, super::FriendlyGitError> {
        let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());

        let (handle, root) = repos.discover(&canonical)?;
        let info = repo_info(&handle, &root)?;

        let mut inner = self.inner.lock().expect("git watcher mutex poisoned");
        if let Some(sub) = inner.get_mut(&root) {
            sub.refcount = sub.refcount.saturating_add(1);
            return Ok(info);
        }

        // First subscriber: start the debouncer.
        let watcher_root = root.clone();
        let mut debouncer = new_debouncer(Duration::from_millis(200), None, move |result: DebounceEventResult| {
            if result.is_err() {
                return;
            }
            recompute_and_report(&repos, sink.as_ref(), &watcher_root);
        })
        .map_err(|e| {
            super::FriendlyGitError::with_source(super::FriendlyGitErrorKind::CorruptRepo, e.to_string(), e)
        })?;

        for path in watch_paths(&root) {
            // Some paths (`refs/`) are dirs, others (`HEAD`, `index`) are files.
            // notify happily handles both. Missing paths are common (no MERGE_HEAD
            // until a merge starts) – we register watches lazily by watching the
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

    /// Drops a subscriber. Tears the watcher down on the last unsubscribe, and
    /// releases what that repo was holding open: nobody is looking at it, so a
    /// full-repo-sized status snapshot and an open `gix` handle would be pure
    /// leak.
    pub fn unsubscribe(&self, repos: &RepoCache, repo_root: &Path) {
        let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        let mut inner = self.inner.lock().expect("git watcher mutex poisoned");
        if let Some(sub) = inner.get_mut(&canonical) {
            sub.refcount = sub.refcount.saturating_sub(1);
            if sub.refcount == 0 {
                inner.remove(&canonical);
                repos.evict(&canonical);
                super::status::invalidate_status_cache(&canonical);
            }
        }
    }

    /// For tests: count active repos.
    #[cfg(test)]
    pub fn active_repo_count(&self) -> usize {
        self.inner.lock_ignore_poison().len()
    }
}

impl Default for GitWatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Recomputes the repo's snapshot and hands it to the sink.
///
/// The status cache is dropped BEFORE the report goes out: a subscriber reacts
/// to a report by asking for status, and a cache still holding the pre-change
/// walk would answer that with what the user just stopped seeing.
///
/// Any `.git/*` mutation we watch for is a superset of "the index might have
/// moved", so the drop is unconditional. Cheap (a `HashMap` remove), so it isn't
/// worth filtering by event type.
fn recompute_and_report(repos: &RepoCache, sink: &dyn GitStateSink, repo_root: &Path) {
    let Ok((handle, root)) = repos.discover(repo_root) else {
        return;
    };
    let Ok(info) = repo_info(&handle, &root) else {
        return;
    };
    super::status::invalidate_status_cache(&root);
    sink.repo_changed(&root, info);
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
    let mut paths: Vec<PathBuf> = [
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
    .collect();

    // Linked worktrees: each has its own HEAD under
    // `<common-dir>/worktrees/<name>/HEAD`. We register one watch per
    // worktree at subscribe time. New worktrees added later are picked
    // up via the non-recursive `.git` watch (the `worktrees/` parent
    // directory's create event triggers a re-subscribe path on the
    // refresh – and even without that, a `git worktree add` always
    // touches `HEAD` in the main repo too, which fires a re-emit).
    //
    // Decision: per-worktree registration on enumeration rather than glob
    // support. notify-debouncer-full doesn't natively glob. Registering
    // each `worktrees/<name>/HEAD` keeps the notify config flat and
    // self-documenting; the cost is a few extra watcher entries per
    // worktree, which is negligible at typical worktree counts (1-5).
    let worktrees_dir = git_dir.join("worktrees");
    if let Ok(read) = std::fs::read_dir(&worktrees_dir) {
        for entry in read.flatten() {
            let head = entry.path().join("HEAD");
            if head.exists() {
                paths.push(head);
            }
        }
    }
    paths
}
