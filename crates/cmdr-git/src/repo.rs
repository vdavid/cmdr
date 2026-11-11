//! Repo discovery and info (branch, upstream, ahead/behind, dirty).
//!
//! `discover_repo(path)` walks up from `path` looking for `.git` (dir or
//! gitlink file). It rejects bare repos because the whole UX is anchored on a
//! working tree. `repo_info(repo)` collects the chip-relevant state.
//!
//! Handles live in a [`RepoCache`], which is a VALUE the [`GitPortal`] owns
//! (`portal.rs`), ❌ never a static of its own. Every caller opens a repository
//! through a portal's cache, so one process shares one set of open handles.
//!
//! [`GitPortal`]: crate::portal::GitPortal

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Instant;

use cmdr_fs::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use cmdr_fs::mapped_file_faults;
use serde::{Deserialize, Serialize};

use cmdr_fs::volume::git_error::{FriendlyGitError, FriendlyGitErrorKind};

use crate::fault_guard::{GuardedRead, volume_gone};

/// Snapshot of mutable repo state for the breadcrumb chip.
///
/// Computed at portal entry and re-emitted by the watcher on every relevant
/// `.git/*` change. The frontend never polls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
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

/// Computes branch / detached / dirty / ahead-behind for a discovered repo.
///
/// Answers `VolumeGone` when the repo's disk went away while it was read, or
/// its volume stopped the dirty walk because it's about to unmount
/// (`fault_guard.rs`).
pub fn repo_info(handle: &RepoHandle, repo_root: &Path) -> Result<RepoInfo, FriendlyGitError> {
    let read = GuardedRead::begin(repo_root);
    read.vouch(read_repo_info(handle, repo_root, &read.stop_flag()))
}

fn read_repo_info(handle: &RepoHandle, repo_root: &Path, stop: &Arc<AtomicBool>) -> Result<RepoInfo, FriendlyGitError> {
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

    let is_dirty = if unborn { false } else { is_dirty(&repo, stop) };

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

/// Whether anything is staged or changed in the worktree, untracked files aside,
/// a submodule's own edits included.
///
/// gix's own `Repository::is_dirty` asks submodules for their worktree status
/// "as configured", which opens each one with its OWN config, filter drivers
/// included ([`without_filter_drivers`]). This is the same question with
/// [`submodule_status`] plus [`dirty_submodule_paths`]. The walk stops when
/// `stop` is set.
fn is_dirty(repo: &gix::Repository, stop: &Arc<AtomicBool>) -> bool {
    let Ok(platform) = repo.status(gix::progress::Discard) else {
        return false;
    };
    let Ok(mut items) = platform
        .should_interrupt_owned(Arc::clone(stop))
        .index_worktree_rewrites(None)
        .index_worktree_submodules(submodule_status(true))
        .index_worktree_options_mut(|opts| opts.dirwalk_options = None)
        .into_iter(std::iter::empty::<gix::bstr::BString>())
    else {
        return false;
    };
    items.next().is_some_and(|item| item.is_ok()) || !dirty_submodule_paths(repo, stop).is_empty()
}

/// The worktree paths (`/`-separated, relative to `repo`'s) of submodules whose
/// own worktree has changes, nested ones included.
///
/// Each is opened and then stripped by [`without_filter_drivers`] before its
/// worktree is walked, which is what gix's built-in submodule status can't do.
/// A submodule configured `ignore = dirty` or `all` is left out, as git would.
/// Each walk stops when `stop` is set.
pub(crate) fn dirty_submodule_paths(repo: &gix::Repository, stop: &Arc<AtomicBool>) -> Vec<String> {
    let Ok(Some(modules)) = repo.submodules() else {
        return Vec::new();
    };
    modules
        .filter_map(|sm| {
            let ignored = matches!(
                sm.ignore(),
                Ok(Some(
                    gix::submodule::config::Ignore::Dirty | gix::submodule::config::Ignore::All
                ))
            );
            if ignored {
                return None;
            }
            let path = sm.path().ok()?.to_string();
            // Opening reads config and runs nothing; the walk below is what could.
            let opened = sm.open().ok()??;
            let stripped = without_filter_drivers(opened.into_sync()).ok()?.to_thread_local();
            is_dirty(&stripped, stop).then_some(path)
        })
        .collect()
}

/// How far gix's status looks into a submodule: its checked-out commit against
/// the one the parent records, ❌ never its worktree. A worktree status hashes
/// files, which runs that repo's filter drivers, and gix opens a submodule with
/// its own config, out of reach of [`without_filter_drivers`].
/// [`dirty_submodule_paths`] answers the worktree half safely.
pub(crate) fn submodule_status(check_dirty: bool) -> gix::status::Submodule {
    gix::status::Submodule::Given {
        ignore: gix::submodule::config::Ignore::Dirty,
        check_dirty,
    }
}

/// `repo` with every `filter.<driver>` section dropped from its in-memory config.
///
/// ❗ A repository's own config can name a command (`filter.x.clean`,
/// `.smudge`, `.process`) that its `.gitattributes` applies to files, and gix
/// runs it whenever status has to hash a file whose stat changed. A repo the user
/// downloaded or unzipped is owned by them, so gix trusts that config in full:
/// without this, showing such a folder in a pane runs its author's command.
/// Cmdr only reads, so a filter buys nothing here. Nothing is written to disk.
fn without_filter_drivers(repo: gix::ThreadSafeRepository) -> Result<gix::ThreadSafeRepository, FriendlyGitError> {
    let mut local = repo.to_thread_local();
    let mut config = local.config_snapshot_mut();
    let ids: Vec<_> = config
        .sections_and_ids_by_name("filter")
        .map(|sections| sections.map(|(_, id)| id).collect())
        .unwrap_or_default();
    for id in ids {
        config.remove_section_by_id(id);
    }
    config
        .commit()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    Ok(local.into_sync())
}

/// The largest single allocation a repository's own object data may cause: twice the largest
/// blob Cmdr reads (`tree::MAX_BLOB_BYTES`), which leaves room for a delta's base.
///
/// A size field decoded from a page that went away reads as whatever the zeros make of it, and
/// without a cap a corrupt one sizes an allocation.
const OBJECT_ALLOC_LIMIT: u64 = 2 * crate::tree::MAX_BLOB_BYTES;

/// How every repository is opened: gix's defaults, plus [`OBJECT_ALLOC_LIMIT`] for a repo the
/// user owns. ❌ Only `full`: a repo opened with reduced trust already gets gix's own 16 MiB
/// cap, and an explicit limit would raise it.
fn open_options() -> gix::sec::trust::Mapping<gix::open::Options> {
    let mut trust = gix::sec::trust::Mapping::<gix::open::Options>::default();
    trust.full = trust
        .full
        .config_overrides([format!("gitoxide.objects.allocLimit={OBJECT_ALLOC_LIMIT}")]);
    trust
}

/// Whether `path` is on the disk the system or the user's home lives on, which can't go away
/// under a mapping while Cmdr runs. Anything else (a USB drive, an SD card, a network share, a
/// second internal volume) counts as one that can, which only costs reading packed-refs into
/// memory.
#[cfg(unix)]
fn is_on_the_startup_disk(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    static STARTUP_DEVICES: std::sync::OnceLock<Vec<u64>> = std::sync::OnceLock::new();
    let startup = STARTUP_DEVICES.get_or_init(|| {
        [Some(PathBuf::from("/")), std::env::home_dir()]
            .into_iter()
            .flatten()
            .filter_map(|dir| std::fs::metadata(dir).ok().map(|meta| meta.dev()))
            .collect()
    });
    std::fs::metadata(path).is_ok_and(|meta| startup.contains(&meta.dev()))
}

/// No device number to compare off Unix, so every disk counts as one that can go away: the
/// cautious answer, which only costs reading packed-refs into memory.
#[cfg(not(unix))]
fn is_on_the_startup_disk(_path: &Path) -> bool {
    false
}

fn map_discover_err(err: gix::Error) -> FriendlyGitError {
    // The upward search found no repository (or only an untrusted one): its own
    // typed enum. A `.git` that turned out not to be a real repo is classified
    // `NotFound` by gix's open. Anything else, including a NotFound buried under
    // a corruption (which outranks it), is a damaged repo.
    let not_a_repo = err.downcast_any_ref::<gix::discover::upwards::Error>().is_some()
        || err.dominant_class() == Some(gix::error::Class::NotFound);
    let kind = if not_a_repo {
        FriendlyGitErrorKind::NotARepo
    } else {
        FriendlyGitErrorKind::CorruptRepo
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
    let merge_base = repo.merge_base_with_graph(local, upstream, &mut graph).ok()??.detach();
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

/// Open `gix` repositories, keyed by canonical worktree root.
///
/// A VALUE, ❌ never a static: the [`GitPortal`](crate::portal::GitPortal) owns
/// the one the process shares, so a test (and, later, the crate) can hold its
/// own without reaching a global.
pub struct RepoCache {
    inner: RwLock<std::collections::HashMap<PathBuf, RepoHandle>>,
    /// The mapped-file recovery generation the cached handles were opened under.
    /// When it moved, a disk went away under SOME mapping, and every handle may
    /// be holding a dead one, so the whole cache is dropped.
    opened_at_fault_generation: AtomicU64,
    /// Volumes about to unmount (`let_go_under`), each with when its refusal ends.
    /// No repo under one is opened or cached meanwhile.
    leaving: Mutex<Vec<(PathBuf, Instant)>>,
}

impl RepoCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(std::collections::HashMap::new()),
            opened_at_fault_generation: AtomicU64::new(mapped_file_faults::generation()),
            leaving: Mutex::new(Vec::new()),
        }
    }

    /// Opens (or reuses) the repo containing `path`, returning its handle and
    /// canonical worktree root. Bare repos are refused: without a working tree
    /// there's nothing for the file manager to anchor on. A repo on a volume
    /// that's about to unmount is refused as `VolumeGone`.
    pub fn discover(&self, path: &Path) -> Result<(RepoHandle, PathBuf), FriendlyGitError> {
        self.forget_all_after_a_fault();
        if self.is_leaving(path) {
            return Err(volume_gone(path));
        }
        if let Some((handle, root)) = self.lookup_for_path(path) {
            return Ok((handle, root));
        }

        let opened = gix::ThreadSafeRepository::discover_opts(path, Default::default(), open_options())
            .map_err(map_discover_err)?;
        let mut repo = without_filter_drivers(opened)?;
        if !is_on_the_startup_disk(repo.refs.common_dir_resolved()) {
            // ❗ gix maps a packed-refs file over 32 KiB, and this one sits on a disk that
            // can go away under the mapping; a few hundred KiB in memory is the cheaper risk.
            repo.refs.set_packed_buffer_mmap_threshold(u64::MAX);
        }
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
        self.insert(canonical_root.clone(), handle.clone());
        Ok((handle, canonical_root))
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
        // An open that started before its volume began leaving finishes, but
        // its handle isn't kept past the read that asked for it.
        if self.is_leaving(&root) {
            return;
        }
        if let Ok(mut inner) = self.inner.write() {
            inner.insert(root, handle);
        }
    }

    /// Lets go of every repo under `prefix`, a volume that's about to unmount:
    /// drops their handles, and refuses to open or cache one there until
    /// `refuse_until`. Answers each dropped repo's object store, which the
    /// caller watches until the last read holding it is done, since that's
    /// when its pack mappings close.
    pub(crate) fn let_go_under(&self, prefix: &Path, refuse_until: Instant) -> Vec<Weak<gix::odb::Store>> {
        {
            let mut leaving = self.leaving.lock_ignore_poison();
            leaving.retain(|(root, _)| root != prefix);
            leaving.push((prefix.to_path_buf(), refuse_until));
        }
        let mut inner = self.inner.write_ignore_poison();
        let under: Vec<PathBuf> = inner.keys().filter(|root| root.starts_with(prefix)).cloned().collect();
        under
            .iter()
            .filter_map(|root| inner.remove(root))
            .map(|handle| Arc::downgrade(&handle.objects))
            .collect()
    }

    /// Whether `path` is on a volume that's still leaving, spelled as given or
    /// canonically (a mount root is canonical; a caller's path may run through
    /// a symlink). The `canonicalize` runs only while some volume is leaving.
    fn is_leaving(&self, path: &Path) -> bool {
        let now = Instant::now();
        let leaving: Vec<PathBuf> = {
            let mut leaving = self.leaving.lock_ignore_poison();
            leaving.retain(|(_, until)| now < *until);
            leaving.iter().map(|(root, _)| root.clone()).collect()
        };
        if leaving.is_empty() {
            return false;
        }
        let under = |path: &Path| leaving.iter().any(|root| path.starts_with(root));
        under(path) || path.canonicalize().is_ok_and(|canonical| under(&canonical))
    }

    /// Drops a cached handle. Called when the last `git-state` subscriber for a
    /// repo goes away; no idle timer, no LRU.
    pub fn evict(&self, root: &Path) {
        if let Ok(mut inner) = self.inner.write() {
            inner.remove(root);
        }
    }

    /// Drops every cached handle once a mapped page anywhere was recovered since
    /// they were opened.
    ///
    /// Address-free, like the rest of the recovery (`fault_guard.rs`): nothing
    /// says WHICH repo's disk went away, and a handle whose pack mapping is dead
    /// would fault on every later read. Reopening a healthy repo costs ~10 ms,
    /// and a recovery happens only when a disk vanishes.
    fn forget_all_after_a_fault(&self) {
        let now = mapped_file_faults::generation();
        if self.opened_at_fault_generation.swap(now, Ordering::SeqCst) == now {
            return;
        }
        self.inner.write_ignore_poison().clear();
    }

    /// How many repositories have an open handle. For the cells that prove a
    /// fault empties the cache.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner.read_ignore_poison().len()
    }
}
