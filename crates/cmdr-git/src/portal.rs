//! The git portal: the value that owns everything the virtual `.git` trees are
//! served from, and that every [`GitPortalVolume`](crate::GitPortalVolume) is
//! built over.
//!
//! One portal per process, parked by the app (`wiring.rs`) the way `volume_host`
//! parks the host. It holds the [`RepoCache`] every open repo handle lives in,
//! the per-repo watcher registry, the [`GitStateSink`] that registry reports
//! through, and the [`VolumeHost`] its volumes spawn blocking `gix` work onto.
//!
//! ❌ Nothing here reaches a global. A volume always holds its own
//! `Arc<GitPortal>`, and a test builds its own portal with a detached sink.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::friendly_error::git::FriendlyGitError;
use cmdr_fs::volume::host::VolumeHost;

use crate::repo::{RepoCache, RepoHandle, RepoInfo};
use crate::state_sink::GitStateSink;
use crate::virtual_listing;
use crate::watcher::GitWatcherRegistry;
use cmdr_fs::entry::FileEntry;

/// Everything the virtual `.git` portal needs to answer, in one value.
pub struct GitPortal {
    /// Shared with the watcher's debounce callbacks, which outlive any one call,
    /// hence the `Arc` around a cache the portal otherwise owns outright.
    repos: Arc<RepoCache>,
    watchers: GitWatcherRegistry,
    sink: Arc<dyn GitStateSink>,
    host: VolumeHost,
}

impl GitPortal {
    /// Builds a portal that asks `host` for what it can't answer itself (today:
    /// the runtime its volumes run blocking `gix` work on) and reports every
    /// repo change to `sink`.
    pub fn new(host: VolumeHost, sink: Arc<dyn GitStateSink>) -> Self {
        Self::over(host, sink, GitWatcherRegistry::new())
    }

    /// A portal whose watcher arms nothing with the operating system:
    /// [`fire_watcher`](Self::fire_watcher) stands in for it.
    ///
    /// The instrument for every cell that asserts the registry's BOOKKEEPING —
    /// which repositories are watched, what a change reports, what a listing
    /// arms — none of which needs a real FSEvents stream. Arming one over a
    /// repository's ~10 `.git/*` paths is most of what a subscribe costs, and it
    /// is the one thing that made those cells miss the suite's 8 s cap under
    /// load.
    #[cfg(any(test, feature = "testing"))]
    pub fn with_scripted_watcher(host: VolumeHost, sink: Arc<dyn GitStateSink>) -> Self {
        use crate::watcher::ScriptedWatcherBackend;
        Self::over(
            host,
            sink,
            GitWatcherRegistry::with_backend(Arc::new(ScriptedWatcherBackend::new())),
        )
    }

    fn over(host: VolumeHost, sink: Arc<dyn GitStateSink>, watchers: GitWatcherRegistry) -> Self {
        Self {
            repos: Arc::new(RepoCache::new()),
            watchers,
            sink,
            host,
        }
    }

    /// The open-repo handles this portal's volumes and callers share.
    pub fn repos(&self) -> &RepoCache {
        &self.repos
    }

    /// The app around this portal.
    pub fn host(&self) -> &VolumeHost {
        &self.host
    }

    /// Opens (or reuses) the repository containing `path`, answering its handle
    /// and canonical worktree root.
    pub fn discover(&self, path: &Path) -> Result<(RepoHandle, PathBuf), FriendlyGitError> {
        self.repos.discover(path)
    }

    /// The six category rows for the repo whose worktree root is `worktree_root`,
    /// or nothing when that directory isn't a repository's own root.
    ///
    /// ❗ The rows belong to `worktree_root` only when the repository `gix` finds
    /// is the one whose gitdir this is. Discovery walks UP, so a directory merely
    /// NAMED `.git` inside some repo's working tree would otherwise be handed
    /// that repo's branches.
    ///
    /// The rows carry the CANONICAL root, so each one's path matches what the
    /// route and the watcher's refresh prefixes are built from; a temp dir
    /// reached through a symlink (`/var` → `/private/var` on macOS) would
    /// otherwise cache the child listing under a spelling the watcher can't find.
    pub fn category_rows(&self, worktree_root: &Path) -> Vec<FileEntry> {
        let Ok((handle, canonical_root)) = self.discover(worktree_root) else {
            return Vec::new();
        };
        let listed_root = std::fs::canonicalize(worktree_root).unwrap_or_else(|_| worktree_root.to_path_buf());
        if listed_root != canonical_root {
            return Vec::new();
        }
        virtual_listing::list_categories(&handle, &canonical_root)
    }

    /// Adds a subscriber for the repository at `repo_root`, starting its `.git/*`
    /// watcher on the first one. Answers the current [`RepoInfo`] synchronously,
    /// so a subscriber never sees an empty interim state.
    pub fn subscribe_state(&self, repo_root: &Path) -> Result<RepoInfo, FriendlyGitError> {
        self.watchers
            .subscribe(Arc::clone(&self.repos), Arc::clone(&self.sink), repo_root)
    }

    /// [`subscribe_state`](Self::subscribe_state) without the snapshot: adds a
    /// subscriber and answers the CANONICAL root to release it by.
    ///
    /// What a host arming the watcher on behalf of an open listing asks for. It
    /// has nowhere to put a `RepoInfo`, and reading one walks the worktree for
    /// `is_dirty`, which is the expensive half of the handshake. Pair every call
    /// with one [`unsubscribe_state`](Self::unsubscribe_state) on the root this
    /// answered.
    pub fn watch_repo(&self, repo_root: &Path) -> Result<PathBuf, FriendlyGitError> {
        self.watchers
            .arm(Arc::clone(&self.repos), Arc::clone(&self.sink), repo_root)
            .map(|(_, root)| root)
    }

    /// Drops one subscriber. The last one out stops the watcher and releases
    /// what that repository was holding open.
    pub fn unsubscribe_state(&self, repo_root: &Path) {
        self.watchers.unsubscribe(&self.repos, repo_root);
    }

    /// How many repositories currently have a `.git/*` watcher running.
    ///
    /// A test door, gated so a consumer's suites can assert on it too: the
    /// host's toggle cells check that a `.git/` pane drives a refresh without
    /// any repository being subscribed, which is only a premise worth stating if
    /// it can be read.
    #[cfg(any(test, feature = "testing"))]
    pub fn watched_repo_count(&self) -> usize {
        self.watchers.active_repo_count()
    }

    /// Pretends the repository at `repo_root` just had its `.git/*` state
    /// written, answering whether a watch was armed for it.
    ///
    /// Only a portal built by [`with_scripted_watcher`](Self::with_scripted_watcher)
    /// can answer `true`; the real backend takes its events from the operating
    /// system and has no door in. A test that owns a repository and wants to see
    /// what a change reaches reaches for this instead of writing refs and
    /// waiting out a debounce.
    #[cfg(any(test, feature = "testing"))]
    pub fn fire_watcher(&self, repo_root: &Path) -> bool {
        let Ok((_, root)) = self.discover(repo_root) else {
            return false;
        };
        self.watchers.fire(&root)
    }
}
