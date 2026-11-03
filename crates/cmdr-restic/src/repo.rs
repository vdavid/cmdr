//! Opening a repo with its password, reading its snapshot list, and loading its
//! index for the trees and file contents.
//!
//! Every `rustic_core` call runs on `spawn_blocking` (the storage adapter
//! `block_on`s the parent volume, which would panic on an async worker), so
//! each method here is async and hops to a blocking thread once.

use crate::backend::{VolumeBackend, pack_gone_in, volume_error_in};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{LockedContainer, Volume, VolumeError};
use rustic_core::repofile::SnapshotFile;
use rustic_core::{
    Credentials, FileType, IndexedFullStatus, OpenStatus, ReadBackend, Repository, RepositoryBackends,
    RepositoryOptions, RusticError,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

/// The threads of the pool a rustic call that fans out runs on
/// ([`on_own_pool`]). rustic's `stream_list` parks one on the channel it
/// drains, so four read and decode at once.
pub(crate) const RUSTIC_POOL_THREADS: usize = 5;

/// What those threads are called, so a sample or a test tells them from
/// rayon's global pool.
pub(crate) const RUSTIC_POOL_PREFIX: &str = "restic-rustic";

/// Runs `work`, a rustic call, inside a small pool of its own, built for it and
/// gone with it. rustic 0.13.0 fans out on whatever pool it runs in:
/// `stream_list` (the index load, a snapshot refresh) does `rayon::spawn` plus
/// `into_par_iter`, each a pool worker blocked in our adapter for a NAS round
/// trip, and the index's `into_index` does a parallel sort. From a plain
/// blocking thread that's rayon's GLOBAL pool, which Cmdr's search and icons
/// share. `install` puts all of it here instead. Blocks: run it on the blocking
/// pool.
pub(crate) fn on_own_pool<T: Send>(work: impl FnOnce() -> Result<T, ResticError> + Send) -> Result<T, ResticError> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(RUSTIC_POOL_THREADS)
        .thread_name(|i| format!("{RUSTIC_POOL_PREFIX}-{i}"))
        .build()
        .map_err(|err| ResticError::Unreadable {
            detail: format!("rustic's pool didn't start: {err}"),
        })?;
    pool.install(work)
}

/// Why a repo couldn't be opened or read. Typed, so the host words each one.
#[derive(Debug, Clone)]
pub(crate) enum ResticError {
    /// The password doesn't unlock any of the repo's keys.
    WrongPassword,
    /// The folder has no restic `config`, so it isn't a repo (any more).
    NotARepository,
    /// The volume the repo lives on couldn't answer (a dropped share, no access).
    Volume(VolumeError),
    /// A pack the index names is gone (the parent's `NotFound`): a prune
    /// elsewhere repacked its blobs since the index was loaded, so a read
    /// reloads the index once and tries again (DETAILS § "Coverage and
    /// reloads"). Through a fresh index, a damaged repo.
    PackGone(VolumeError),
    /// rustic couldn't read the repo for another reason (a damaged or
    /// unsupported repo, a crashed worker).
    Unreadable {
        /// rustic's own message, for the log and the technical details. ❌ Never
        /// matched on.
        detail: String,
    },
}

impl ResticError {
    /// The [`VolumeError`] a volume answers for this, about `path`.
    pub(crate) fn into_volume_error(self, path: &Path) -> VolumeError {
        match self {
            Self::WrongPassword => VolumeError::NeedsPassword {
                container: LockedContainer::ResticRepo,
                wrong_attempt: true,
            },
            Self::NotARepository => VolumeError::NotFound(path.display().to_string()),
            Self::Volume(err) | Self::PackGone(err) => err,
            Self::Unreadable { detail } => VolumeError::IoError {
                message: detail.into(),
                raw_os_error: None,
            },
        }
    }
}

/// The repo options every open shares: no rustic cache unless the host gives a
/// folder for one (❌ never rustic's default location).
fn options(cache_dir: Option<PathBuf>) -> RepositoryOptions {
    let mut options = RepositoryOptions::default();
    options.no_cache = cache_dir.is_none();
    options.cache_dir = cache_dir;
    options
}

/// Opens and unlocks the repo behind `backend`. Runs on the blocking thread.
pub(crate) fn open_blocking(
    backend: &Arc<VolumeBackend>,
    password: &str,
    cache_dir: Option<PathBuf>,
) -> Result<Repository<OpenStatus>, ResticError> {
    let backends = RepositoryBackends::new(Arc::clone(backend) as _, None);
    Repository::new(&options(cache_dir), &backends)
        .and_then(|repo| repo.open(&Credentials::password(password)))
        .map_err(|err| classify(&err, backend))
}

/// An unlocked restic repo, read through the volume it lives on.
///
/// Holds rustic's opened (not indexed) repository: enough for the snapshot list,
/// which never needs the index.
pub(crate) struct ResticRepo {
    repo: Arc<Repository<OpenStatus>>,
    pub(crate) backend: Arc<VolumeBackend>,
    runtime: Handle,
    /// One snapshot refresh's blocking work at a time ([`Self::refresh_snapshots`]).
    refreshing: Arc<Semaphore>,
}

impl ResticRepo {
    /// Opens the repo in the folder `root` on `parent`, and unlocks it with
    /// `password`.
    ///
    /// The key derivation (scrypt) takes about a second by design, on a blocking
    /// thread. `cache_dir` is where rustic may keep its local cache of snapshot
    /// and index files; `None` runs without one. Opening never writes to the repo
    /// and never takes a restic lock.
    pub(crate) async fn open(
        parent: Arc<dyn Volume>,
        root: PathBuf,
        password: &str,
        cache_dir: Option<PathBuf>,
        host: &VolumeHost,
    ) -> Result<Self, ResticError> {
        let runtime = host.runtime();
        let backend = Arc::new(VolumeBackend::new(parent, root, runtime.clone()));
        let password = Zeroizing::new(password.to_string());
        let task_backend = Arc::clone(&backend);
        let repo = runtime
            .spawn_blocking(move || open_blocking(&task_backend, &password, cache_dir))
            .await
            .map_err(worker_crashed)??;
        Ok(Self {
            repo: Arc::new(repo),
            backend,
            runtime,
            refreshing: Arc::new(Semaphore::new(1)),
        })
    }

    /// Every snapshot in the repo, given the ones already known: rustic keeps
    /// those, reads only the snapshot files it hasn't seen, and drops the ones
    /// gone from the repo. Pass an empty `current` for a first read. Never reads
    /// the index. rustic reads the new files in parallel, so on its own pool
    /// ([`on_own_pool`]).
    ///
    /// ❗ The blocking work holds a permit until it ENDS, ❌ never just until its
    /// caller stops waiting: a refresh stuck in a parent read that never returns
    /// keeps its threads (rustic's pool plus a blocking one), and a pane's cancel
    /// releases only the caller. So the next refresh waits for it (cancellably)
    /// rather than build another pool beside it, and a hung parent costs one
    /// stuck refresh per repo, not one per attempt.
    pub(crate) async fn refresh_snapshots(&self, current: Vec<SnapshotFile>) -> Result<Vec<SnapshotFile>, ResticError> {
        let repo = Arc::clone(&self.repo);
        let backend = Arc::clone(&self.backend);
        let permit = Arc::clone(&self.refreshing)
            .acquire_owned()
            .await
            .map_err(|closed| ResticError::Unreadable {
                detail: format!("the refresh permit closed: {closed}"),
            })?;
        self.runtime
            .spawn_blocking(move || {
                let _one_at_a_time = permit;
                on_own_pool(|| {
                    repo.update_all_snapshots(current)
                        .map_err(|err| classify(&err, &backend))
                })
            })
            .await
            .map_err(worker_crashed)?
    }
}

impl std::fmt::Debug for ResticRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResticRepo")
            .field("name", &self.repo.name)
            .finish_non_exhaustive()
    }
}

/// A repo opened AND indexed: what browsing a snapshot's trees and reading its
/// files need (`IndexedFull`).
///
/// Its own open, separate from the [`ResticRepo`] the snapshot list keeps:
/// `to_indexed` consumes the opened repo, so sharing one would mean prying it
/// out from under in-flight listings, and a failed load would lose it.
pub(crate) struct IndexedRepo {
    pub(crate) repo: Repository<IndexedFullStatus>,
    pub(crate) backend: Arc<VolumeBackend>,
}

// Concurrent reads go through `&IndexedRepo` from several blocking threads at
// once (a copy reads files in parallel), which is sound only while rustic's
// indexed repository stays `Send + Sync` (true in 0.13.0: every field is an
// `Arc<dyn … + Send + Sync>`, the index an `Arc`, the blob cache a
// `quick_cache::sync::Cache`). A bump that breaks it fails to compile here.
const _: fn() = || {
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<Repository<IndexedFullStatus>>();
};

impl IndexedRepo {
    /// Loads the full index of `open`, the repo opened through `backend`. Runs
    /// on the blocking thread; reads every index file rustic doesn't get from
    /// the load's feed (`index_load.rs`).
    pub(crate) fn index(open: Repository<OpenStatus>, backend: Arc<VolumeBackend>) -> Result<Self, ResticError> {
        let repo = open.to_indexed().map_err(|err| classify(&err, &backend))?;
        Ok(Self { repo, backend })
    }

    /// Sorts a failure of a read through this repo. Runs on the blocking thread.
    pub(crate) fn classify(&self, err: &RusticError) -> ResticError {
        classify(err, &self.backend)
    }
}

/// Sorts a rustic failure into a [`ResticError`] by type, never by text.
///
/// rustic marks a wrong password with its own error code, read through
/// `is_incorrect_password()`. A parent failure rides in the error's source chain
/// as the typed [`VolumeError`] the adapter put there, wrapped as
/// [`PackGone`](crate::backend::PackGone) when it's a pack the parent says is gone. Anything else is a missing
/// repo when the config is gone (asked of the parent again, which is cheap and
/// typed), and unreadable otherwise. Runs on the blocking thread.
fn classify(err: &RusticError, backend: &VolumeBackend) -> ResticError {
    if err.is_incorrect_password() {
        return ResticError::WrongPassword;
    }
    if let Some(gone) = pack_gone_in(err) {
        return ResticError::PackGone(gone.0.clone());
    }
    if let Some(volume_error) = volume_error_in(err) {
        return ResticError::Volume(volume_error.clone());
    }
    if backend
        .list_with_size(FileType::Config)
        .is_ok_and(|config| config.is_empty())
    {
        return ResticError::NotARepository;
    }
    ResticError::Unreadable {
        detail: err.display_log(),
    }
}

/// A blocking task that panicked or was dropped: a typed error, ❌ never a crash.
pub(crate) fn worker_crashed(err: tokio::task::JoinError) -> ResticError {
    ResticError::Unreadable {
        detail: format!("the blocking worker stopped: {err}"),
    }
}

#[cfg(test)]
#[path = "repo_tests.rs"]
mod tests;
