//! The git portal: the value that owns everything the virtual `.git` trees are
//! served from, and mints a [`GitPortalVolume`] per repo.
//!
//! One portal per process, parked by the app (see [`portal`]) the way
//! `volume_host` parks the host. It holds the [`RepoCache`] every open repo
//! handle lives in, plus the [`VolumeHost`] its volumes spawn blocking `gix`
//! work onto, so there is one thread pool and one cache rather than a static
//! per subsystem.
//!
//! ❌ Nothing here reaches a global for the cache. `portal()` is where the app
//! parks its instance so an IPC command can pick it up; a volume always holds
//! its own `Arc<GitPortal>`.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use cmdr_fs::volume::host::VolumeHost;

use super::repo::RepoCache;
use super::volume::GitPortalVolume;
use crate::file_system::volume::Volume;

/// Everything the virtual `.git` portal needs to answer, in one value.
pub struct GitPortal {
    repos: RepoCache,
    host: VolumeHost,
}

impl GitPortal {
    /// Builds a portal that asks `host` for what it can't answer itself (today:
    /// the runtime its volumes run blocking `gix` work on).
    pub fn new(host: VolumeHost) -> Self {
        Self {
            repos: RepoCache::new(),
            host,
        }
    }

    /// The open-repo handles this portal's volumes and IPC commands share.
    pub fn repos(&self) -> &RepoCache {
        &self.repos
    }

    /// The app around this portal.
    pub fn host(&self) -> &VolumeHost {
        &self.host
    }

    /// Mints the read-only volume serving `<repo_root>/.git`'s virtual trees.
    ///
    /// `parent` is the volume physically holding the repo: the portal borrows
    /// its lane key and its free space, the way an `ArchiveVolume` borrows the
    /// volume holding the `.zip`. Cheap and infallible; the repo is opened on
    /// first use through the shared cache, and a `.git` that isn't a repository
    /// answers `NotFound` rather than failing to construct.
    pub fn volume_for(self: &Arc<Self>, repo_root: PathBuf, parent: Arc<dyn Volume>) -> GitPortalVolume {
        GitPortalVolume::new(Arc::clone(self), repo_root, parent)
    }
}

/// The portal the app parked, built on first use.
///
/// ❌ Not how a volume finds its portal: a `GitPortalVolume` holds an
/// `Arc<GitPortal>` handed to it at construction. This is for the call sites
/// that predate the portal (repo discovery from an IPC command, the watcher),
/// so they share the one cache instead of opening repos twice.
pub fn portal() -> &'static Arc<GitPortal> {
    static PORTAL: OnceLock<Arc<GitPortal>> = OnceLock::new();
    PORTAL.get_or_init(|| Arc::new(GitPortal::new(crate::volume_host::host())))
}
