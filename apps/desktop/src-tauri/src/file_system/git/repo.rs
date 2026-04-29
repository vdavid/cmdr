//! Repo discovery and info (branch, upstream, ahead/behind, dirty).
//!
//! `discover_repo(path)` walks up from `path` looking for `.git` (dir or
//! gitlink file). It rejects bare repos because the whole UX is anchored on a
//! working tree. `repo_info(repo)` collects the chip-relevant state.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};

/// Snapshot of mutable repo state for the breadcrumb chip.
///
/// Computed at portal entry and re-emitted by the watcher on every relevant
/// `.git/*` change. The frontend never polls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// Canonical repo root (the working tree dir, not `.git`).
    pub repo_root: String,
    /// Short branch name (`main`), or `None` if detached or unborn.
    pub branch: Option<String>,
    /// Short SHA when detached (`a1b2c3d`). `None` otherwise.
    pub detached_sha: Option<String>,
    /// `true` only when HEAD points at a non-existent ref (fresh `git init`).
    pub unborn: bool,
    /// Configured upstream as `remote/branch` (`origin/main`), or `None`.
    pub upstream: Option<String>,
    /// Commits ahead of upstream, `None` if no upstream or count failed.
    pub ahead: Option<u32>,
    /// Commits behind upstream, `None` if no upstream or count failed.
    pub behind: Option<u32>,
    /// Whether the worktree differs from the index, or the index from HEAD.
    /// Untracked files do NOT flip this (matches `git status`'s definition
    /// in `is_dirty`).
    pub is_dirty: bool,
}

/// Thread-safe repo handle suitable for short-lived calls.
///
/// We hand out clones of `Arc<gix::ThreadSafeRepository>`; per call site we
/// turn it into a `gix::Repository` via `.to_thread_local()`. This matches
/// gix's recommended pattern for repos shared across tasks.
pub type RepoHandle = Arc<gix::ThreadSafeRepository>;

/// Discovers a repo from any path inside a worktree.
///
/// Walks up looking for `.git` (dir or gitlink file). Returns the `RepoHandle`
/// and the canonical worktree root. Bare repos are rejected – without a
/// working tree there's nothing for the file manager to anchor on.
///
/// This is the single entry point for repo lookup; `repo_info` and
/// `list_status` both go through the cache to avoid re-opening the same repo.
pub fn discover_repo(path: &Path) -> Result<(RepoHandle, PathBuf), FriendlyGitError> {
    let cache = repo_cache();
    if let Some((handle, root)) = cache.lookup_for_path(path) {
        return Ok((handle, root));
    }

    let repo = gix::ThreadSafeRepository::discover(path).map_err(map_discover_err)?;
    // `ThreadSafeRepository` only exposes `work_dir()` (no `workdir`).
    // Suppress the deprecation warning here – gix kept work_dir on the
    // ThreadSafe wrapper while only the Repository alias got a replacement.
    #[allow(
        deprecated,
        reason = "gix::ThreadSafeRepository only exposes work_dir(); see status.rs for Repository::workdir()"
    )]
    let root = repo
        .work_dir()
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::BareRepo, path.display().to_string()))?
        .to_path_buf();
    let canonical_root = root.canonicalize().unwrap_or(root.clone());
    let handle: RepoHandle = Arc::new(repo);
    cache.insert(canonical_root.clone(), handle.clone());
    Ok((handle, canonical_root))
}

/// Computes branch / detached / dirty / ahead-behind for a discovered repo.
pub fn repo_info(handle: &RepoHandle, repo_root: &Path) -> Result<RepoInfo, FriendlyGitError> {
    let repo = handle.to_thread_local();

    let head = repo.head().map_err(|e| FriendlyGitError::corrupt(repo_root, &e))?;
    let kind = head.kind.clone();

    let mut branch = None;
    let mut detached_sha = None;
    let mut unborn = false;

    match &kind {
        gix::head::Kind::Symbolic(reference) => {
            branch = Some(short_branch_name(reference.name.as_bstr()));
        }
        gix::head::Kind::Unborn(ref_name) => {
            branch = Some(short_branch_name(ref_name.as_bstr()));
            unborn = true;
        }
        gix::head::Kind::Detached { target, .. } => {
            detached_sha = Some(short_sha(target));
        }
    }

    // Upstream + ahead/behind only make sense for a real branch with an upstream.
    let mut upstream = None;
    let mut ahead = None;
    let mut behind = None;
    if !unborn
        && let gix::head::Kind::Symbolic(reference) = &kind
        && let Some(local_id) = repo.head_id().ok().map(|id| id.detach())
    {
        let local_full_name = reference.name.as_ref();
        if let Some(Ok(tracking)) = repo.branch_remote_tracking_ref_name(local_full_name, gix::remote::Direction::Fetch)
        {
            upstream = Some(format_upstream(tracking.as_ref()));
            if let Ok(upstream_ref) = repo.find_reference(tracking.as_ref())
                && let Ok(upstream_id) = upstream_ref.into_fully_peeled_id()
            {
                let upstream_id = upstream_id.detach();
                if let Some((a, b)) = count_ahead_behind(&repo, local_id, upstream_id) {
                    ahead = Some(a);
                    behind = Some(b);
                }
            }
        }
    }

    let is_dirty = if unborn {
        false
    } else {
        repo.is_dirty().unwrap_or(false)
    };

    Ok(RepoInfo {
        repo_root: repo_root.display().to_string(),
        branch,
        detached_sha,
        unborn,
        upstream,
        ahead,
        behind,
        is_dirty,
    })
}

/// Drops a cached `RepoHandle`.
///
/// Called by `unsubscribe_git_state` once subscribers drop to zero. We keep
/// it simple: no idle timer, no LRU. If two subscribers race to unsubscribe,
/// the last one out evicts.
pub fn evict_handle(repo_root: &Path) {
    repo_cache().evict(repo_root);
}

fn map_discover_err(err: gix::discover::Error) -> FriendlyGitError {
    let kind = match &err {
        gix::discover::Error::Discover(_) => FriendlyGitErrorKind::NotARepo,
        gix::discover::Error::Open(open_err) => match open_err {
            // gix::open::Error::NotARepository is the "we found .git but it's not a real repo" case.
            // Other open errors fall through as Corrupt.
            gix::open::Error::NotARepository { .. } => FriendlyGitErrorKind::NotARepo,
            _ => FriendlyGitErrorKind::CorruptRepo,
        },
    };
    FriendlyGitError::with_source(kind, err.to_string(), err)
}

fn short_branch_name(name: &gix::bstr::BStr) -> String {
    let s = name.to_string();
    s.strip_prefix("refs/heads/").unwrap_or(&s).to_string()
}

fn short_sha(id: &gix::ObjectId) -> String {
    let hex = id.to_string();
    hex.chars().take(7).collect()
}

fn format_upstream(tracking: &gix::refs::FullNameRef) -> String {
    let s = tracking.as_bstr().to_string();
    // tracking refs look like `refs/remotes/origin/main` – strip the prefix for display.
    s.strip_prefix("refs/remotes/").unwrap_or(&s).to_string()
}

pub(crate) fn count_ahead_behind(
    repo: &gix::Repository,
    local: gix::ObjectId,
    upstream: gix::ObjectId,
) -> Option<(u32, u32)> {
    if local == upstream {
        return Some((0, 0));
    }
    let cache = repo.commit_graph_if_enabled().ok().flatten();
    let mut graph = repo.revision_graph(cache.as_ref());
    let merge_base = repo.merge_base_with_graph(local, upstream, &mut graph).ok()?.detach();
    let ahead = count_commits_between(repo, local, merge_base)?;
    let behind = count_commits_between(repo, upstream, merge_base)?;
    Some((ahead, behind))
}

pub(crate) fn count_commits_between(repo: &gix::Repository, tip: gix::ObjectId, base: gix::ObjectId) -> Option<u32> {
    if tip == base {
        return Some(0);
    }
    let walk = repo
        .rev_walk([tip])
        .with_hidden([base])
        .all()
        .ok()?
        .filter_map(Result::ok);
    let mut count: u32 = 0;
    for _ in walk {
        count = count.saturating_add(1);
        if count > 9_999 {
            // Cap to keep a runaway walk from blocking the chip refresh.
            break;
        }
    }
    Some(count)
}

// ── Handle cache ────────────────────────────────────────────────────────

struct RepoCache {
    inner: RwLock<std::collections::HashMap<PathBuf, RepoHandle>>,
}

impl RepoCache {
    fn new() -> Self {
        Self {
            inner: RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn lookup_for_path(&self, path: &Path) -> Option<(RepoHandle, PathBuf)> {
        let canonical = path.canonicalize().ok()?;
        let inner = self.inner.read().ok()?;
        // Pick the *longest* matching root deterministically. With nested
        // submodules both the parent and the child match `canonical`; the
        // child (deeper path, longer prefix) is the right repo to surface.
        // HashMap iteration is unordered, so without this we'd randomly
        // return parent or child.
        let mut best: Option<(&PathBuf, &RepoHandle)> = None;
        for (root, handle) in inner.iter() {
            if !canonical.starts_with(root) {
                continue;
            }
            match best {
                Some((current_root, _)) if root.as_os_str().len() <= current_root.as_os_str().len() => {}
                _ => best = Some((root, handle)),
            }
        }
        best.map(|(root, handle)| (handle.clone(), root.clone()))
    }

    fn insert(&self, root: PathBuf, handle: RepoHandle) {
        if let Ok(mut inner) = self.inner.write() {
            inner.insert(root, handle);
        }
    }

    fn evict(&self, root: &Path) {
        if let Ok(mut inner) = self.inner.write() {
            inner.remove(root);
        }
    }
}

fn repo_cache() -> &'static RepoCache {
    use std::sync::OnceLock;
    static CACHE: OnceLock<RepoCache> = OnceLock::new();
    CACHE.get_or_init(RepoCache::new)
}
