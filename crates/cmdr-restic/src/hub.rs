//! [`ResticRepos`]: the one hub every restic view shares. It holds what's
//! costly to get again (passwords, opened repos, loaded indexes, the snapshot
//! lists) and the memo of which folders are repos, so the volumes over it stay
//! cheap and a routing cache dropping one loses nothing.
//!
//! A value the host parks once, ❌ never a static: a test builds its own.

use crate::detect::is_restic_repo;
use crate::index_load::{Load, LoadSpec, WINDOW, Waiter};
use crate::path::VIRTUAL_ENTRY_NAME;
use crate::repo::{IndexedRepo, ResticError, ResticRepo};
use crate::snapshots::{Snapshot, SnapshotList, snapshot_list_of};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{ListingProgress, Volume, VolumeError};
use index_slot::{IndexSlot, Installed, free_off_the_runtime};
use rustic_core::repofile::SnapshotFile;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// The overlay row's icon key: a reserved id the frontend draws as its own
/// glyph (`FileIcon.svelte`), like the git portal's `git:`-prefixed ones, so
/// the row reads as a way into the backup, ❗ not a plain folder.
const OVERLAY_ROW_ICON_ID: &str = "restic:snapshots";

/// The timings and caps the hub runs on. Tests shorten them; the host never
/// does. The sizes come from a real NAS repo (`docs/notes/restic/open-cost.md`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tuning {
    /// How long nothing may have read under `latest/` before a root refresh
    /// moves `latest` to a newer snapshot.
    pub(crate) latest_quiet: Duration,
    /// How long a "this folder isn't a repo" answer is trusted.
    pub(crate) not_a_repo_for: Duration,
    /// How long confirming a folder may take before it counts as unconfirmed
    /// (and isn't remembered): a wedged share would otherwise hang every path
    /// below it.
    pub(crate) confirm_deadline: Duration,
    /// How long an undecided confirmation is trusted: long enough that every
    /// path below a slow folder doesn't pay the deadline, short enough that a
    /// NAS done waking its disks is asked again soon.
    pub(crate) undecided_for: Duration,
    /// How long an index stays loaded with nothing using it. Getting it back
    /// costs an unlock plus a load (2–4 s on SMB), holding it costs ~0.5 GB.
    pub(crate) index_idle_drop: Duration,
    /// How long after its last use an index still counts as in use: the gap
    /// between one listing of a scan and the next, or between two files of a
    /// copy, when nothing holds it for a moment.
    pub(crate) lease_grace: Duration,
    /// The most repos indexed at once. One index of a 2.4M-blob repo is
    /// ~0.5–0.65 GB, so one, plus overshoot while every one is in use.
    pub(crate) max_indexed: usize,
    /// The most repos opened (unlocked, snapshot list read) at once: a few MB
    /// each.
    pub(crate) max_opened: usize,
    /// The most index bytes a load reads ahead of rustic.
    pub(crate) index_window: u64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            latest_quiet: Duration::from_secs(30),
            not_a_repo_for: Duration::from_secs(30),
            confirm_deadline: Duration::from_secs(5),
            undecided_for: Duration::from_secs(5),
            index_idle_drop: Duration::from_secs(5 * 60),
            lease_grace: Duration::from_secs(2),
            max_indexed: 1,
            max_opened: 8,
            index_window: WINDOW,
        }
    }
}

/// The most confirmation answers the memo keeps. Far above the repos anyone
/// browses; it bounds a walk that meets thousands of folders named
/// [`VIRTUAL_ENTRY_NAME`].
const MAX_CONFIRMATIONS: usize = 1_024;

/// Where a repo's password is filed in the host's credential store. A URL-ish
/// name no SMB server, SFTP host, or WebDAV URL can spell, so an entry here
/// never shares a key with a server's sign-in (the app files every service
/// under one `smb://…` account scheme). The scope is the repo's
/// [`RepoKey::store_scope`].
pub(crate) const KEYCHAIN_SERVICE: &str = "restic://";

/// One repo: the volume it's on, by id, and its folder there, spelled as the
/// path spelled it (❌ no `canonicalize`: that's a `stat`, and on a network
/// parent a slow one).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RepoKey {
    pub(crate) volume_id: String,
    pub(crate) root: PathBuf,
}

impl RepoKey {
    /// The repo's account in the credential store: `<volume id>:<repo path>`.
    /// The same share through an OS mount and through a direct session is two
    /// volume ids, so two entries.
    pub(crate) fn store_scope(&self) -> String {
        format!("{}:{}", self.volume_id, self.root.display())
    }
}

/// What the memo knows about a folder.
#[derive(Debug, Clone, Copy)]
enum Confirmation {
    Repo,
    NotARepo {
        since: Instant,
    },
    /// The parent didn't answer in time (or failed): no verdict, held briefly.
    Undecided {
        since: Instant,
    },
}

/// The hub of every restic repo the app has met: passwords, opened repos,
/// loaded indexes, snapshot lists, and which folders are repos at all.
///
/// The host builds one and shares it (`Arc`) with every `ResticVolume`. The
/// heavy state lives here so a routing cache evicting a cheap volume never
/// drops an index or forgets a password.
pub struct ResticRepos {
    host: VolumeHost,
    tuning: Tuning,
    /// Shared with each index load's last step, which evicts other repos'
    /// indexes past the cap.
    repos: Arc<Mutex<HashMap<RepoKey, Arc<RepoSlot>>>>,
    confirmations: Mutex<HashMap<RepoKey, Confirmation>>,
}

impl ResticRepos {
    /// A hub with nothing unlocked and nothing confirmed. Its rustic work runs
    /// on `host`'s runtime, and it looks a repo's password up in `host`'s
    /// credential store before asking for one.
    pub fn new(host: VolumeHost) -> Self {
        Self::with_tuning(host, Tuning::default())
    }

    pub(crate) fn with_tuning(host: VolumeHost, tuning: Tuning) -> Self {
        Self {
            host,
            tuning,
            repos: Arc::new(Mutex::new(HashMap::new())),
            confirmations: Mutex::new(HashMap::new()),
        }
    }

    /// Whether `repo_root` on `parent` (registered as `volume_id`) is a restic
    /// repo: the check a host runs before routing a path through the view.
    /// `Some(true)` a repo, `Some(false)` not one, `None` UNDECIDED: the parent
    /// didn't answer inside the deadline or failed. A read may pass through on
    /// `None`; ❗ a write must refuse, or it could put a real `Browse snapshots`
    /// folder inside someone's repo.
    ///
    /// Answered from the memo when it can. A miss costs ONE listing of
    /// `repo_root`, bounded by a deadline; a yes is remembered for good, a no for
    /// a short while (so a real folder named [`VIRTUAL_ENTRY_NAME`] outside a
    /// repo doesn't pay a listing per call), and an undecided answer for a few
    /// seconds (so every path below a slow folder doesn't pay the deadline).
    ///
    /// The listing runs on its own task and a deadline only stops the WAIT: it's
    /// detached, ❌ never dropped, since a transaction abandoned mid-flight wedges
    /// a phone (MTP).
    pub async fn confirm_repo(&self, volume_id: &str, parent: &Arc<dyn Volume>, repo_root: &Path) -> Option<bool> {
        let key = key(volume_id, repo_root);
        if let Some(known) = self.remembered(&key) {
            return known;
        }
        let listing = {
            let (parent, root) = (Arc::clone(parent), repo_root.to_path_buf());
            self.host
                .runtime()
                .spawn(async move { parent.list_directory(&root, None).await })
        };
        let answer = match tokio::time::timeout(self.tuning.confirm_deadline, listing).await {
            Ok(Ok(Ok(entries))) => Some(is_restic_repo(&entries)),
            Ok(Ok(Err(VolumeError::NotFound(_) | VolumeError::NotADirectory(_)))) => Some(false),
            // Wedged, failing, or crashed: no verdict.
            Ok(Ok(Err(_)) | Err(_)) | Err(_) => None,
        };
        self.remember(key, answer);
        answer
    }

    /// What the memo knows about `repo_root` on `volume_id`, with zero I/O:
    /// `Some(true)` confirmed, `Some(false)` recently found not to be a repo,
    /// `None` never asked, undecided, or the "no" went stale. For a synchronous
    /// seam that can't afford a listing.
    pub fn serves_confirmed(&self, volume_id: &str, repo_root: &Path) -> Option<bool> {
        self.remembered(&key(volume_id, repo_root)).flatten()
    }

    /// The [`VIRTUAL_ENTRY_NAME`] row a pane's listing of `repo_root` gains, once
    /// the host saw from that listing's entries that it's a repo
    /// (`is_restic_repo`). Also confirms `repo_root` in the memo, for free, so
    /// entering the row costs no confirmation listing.
    pub fn overlay_row(&self, volume_id: &str, repo_root: &Path) -> FileEntry {
        self.remember(key(volume_id, repo_root), Some(true));
        let path = repo_root.join(VIRTUAL_ENTRY_NAME);
        FileEntry {
            icon_id: OVERLAY_ROW_ICON_ID.to_string(),
            ..FileEntry::new(
                VIRTUAL_ENTRY_NAME.to_string(),
                path.to_string_lossy().into_owned(),
                true,
                false,
            )
        }
    }

    /// The memo's answer, `None` when it has none (never asked, or stale);
    /// `Some(None)` is a remembered "undecided".
    fn remembered(&self, key: &RepoKey) -> Option<Option<bool>> {
        match lock(&self.confirmations).get(key)? {
            Confirmation::Repo => Some(Some(true)),
            Confirmation::NotARepo { since } if since.elapsed() < self.tuning.not_a_repo_for => Some(Some(false)),
            Confirmation::Undecided { since } if since.elapsed() < self.tuning.undecided_for => Some(None),
            Confirmation::NotARepo { .. } | Confirmation::Undecided { .. } => None,
        }
    }

    fn remember(&self, key: RepoKey, is_repo: Option<bool>) {
        let mut memo = lock(&self.confirmations);
        if memo.len() >= MAX_CONFIRMATIONS && !memo.contains_key(&key) {
            // "No"s and undecided answers go first; a confirmed repo is worth keeping.
            memo.retain(|_, known| matches!(known, Confirmation::Repo));
        }
        let known = match is_repo {
            Some(true) => Confirmation::Repo,
            Some(false) => Confirmation::NotARepo { since: Instant::now() },
            None => Confirmation::Undecided { since: Instant::now() },
        };
        memo.insert(key, known);
    }

    pub(crate) fn host(&self) -> &VolumeHost {
        &self.host
    }

    /// The slot for one repo, made on first use. A slot is never dropped, so the
    /// password in it outlives every volume over it.
    pub(crate) fn slot(&self, volume_id: &str, repo_root: &Path) -> Arc<RepoSlot> {
        let key = key(volume_id, repo_root);
        let mut repos = lock(&self.repos);
        Arc::clone(
            repos
                .entry(key)
                .or_insert_with_key(|key| Arc::new(RepoSlot::new(key.clone()))),
        )
    }

    // ── The opened repo and its snapshots ──────────────────────────────────

    /// The repo opened for the snapshot list, through `parent`: the one already
    /// open, or a fresh open with the slot's password when there's none or it
    /// was opened through a parent since replaced.
    async fn opened(&self, slot: &RepoSlot, parent: &Arc<dyn Volume>) -> Result<Arc<ResticRepo>, VolumeError> {
        let forgets = slot.forgets();
        let current = |bound: &&Bound<Arc<ResticRepo>>| bound.is_over(parent) && bound.forgets == slot.forgets();
        let password = {
            let opened = slot.opened.lock().await;
            if let Some(bound) = opened.as_ref().filter(current) {
                return Ok(Arc::clone(&bound.value));
            }
            drop(opened);
            self.password(slot).await?
        };
        let mut opened = slot.opened.lock().await;
        if let Some(bound) = opened.as_ref().filter(current) {
            return Ok(Arc::clone(&bound.value));
        }
        let repo = ResticRepo::open(Arc::clone(parent), slot.key.root.clone(), &password, None, &self.host)
            .await
            .map_err(|err| Self::refused(slot, err))?;
        if slot.forgets() != forgets {
            // The person forgot the password while this opened: keep nothing.
            return Err(VolumeError::NeedsPassword { wrong_attempt: false });
        }
        let repo = Arc::new(repo);
        *opened = Some(Bound::new(parent, Arc::clone(&repo), forgets));
        drop(opened);
        self.evict_opened(slot);
        Ok(repo)
    }

    /// Lets go of the least recently used opened repos past
    /// [`Tuning::max_opened`], keeping `keep`'s. An opened repo in use right now
    /// (its lock held, or its `Arc` out) stays. Its password and snapshot list
    /// stay either way: re-opening costs one unlock.
    fn evict_opened(&self, keep: &RepoSlot) {
        let mut candidates: Vec<(Instant, Arc<RepoSlot>)> = self
            .all_slots()
            .into_iter()
            .filter(|slot| !std::ptr::eq(slot.as_ref(), keep))
            .filter(|slot| slot.opened.try_lock().is_ok_and(|opened| opened.is_some()))
            .map(|slot| (slot.last_used(), slot))
            .collect();
        let mut over = (candidates.len() + 1).saturating_sub(self.tuning.max_opened);
        candidates.sort_by_key(|(used, _)| *used);
        for (_, slot) in candidates {
            if over == 0 {
                break;
            }
            let Ok(mut opened) = slot.opened.try_lock() else {
                continue;
            };
            if opened
                .as_ref()
                .is_some_and(|bound| Arc::strong_count(&bound.value) == 1)
            {
                *opened = None;
                over -= 1;
            }
        }
    }

    fn all_slots(&self) -> Vec<Arc<RepoSlot>> {
        lock(&self.repos).values().cloned().collect()
    }

    /// The repo's snapshots. `refresh` reads the repo's `snapshots/` again
    /// (only the new files), which every listing of the view's root does so a
    /// manual refresh shows a new backup; otherwise the list read last is
    /// reused, and read only when there's none yet.
    ///
    /// A refresh is also the one moment `latest` may move: to the newest
    /// snapshot, once nothing has read under `latest/` for the quiet period, or
    /// at once when the snapshot it showed is gone from the repo.
    pub(crate) async fn snapshots(
        &self,
        slot: &RepoSlot,
        parent: &Arc<dyn Volume>,
        refresh: bool,
    ) -> Result<Arc<SnapshotList>, VolumeError> {
        slot.touch();
        // Every stat and listing inside a snapshot comes through here, so the
        // common answer is one `Arc` clone, never a copy of the files.
        let known = lock(&slot.snapshots).as_ref().map(|known| Arc::clone(&known.list));
        if let Some(list) = known.filter(|_| !refresh) {
            return Ok(list);
        }
        // One refresh at a time per repo: a caller takes the list of a refresh
        // that STARTED after it asked instead of reading `snapshots/` (and
        // rebinding `latest`) again. ❗ Started, not finished: a refresh already
        // running when the caller asked may have listed `snapshots/` before the
        // backup the caller is asking about landed (the app's snapshot watch).
        let asked = Instant::now();
        let _refreshing = slot.refreshing.lock().await;
        let started = Instant::now();
        let fresh = lock(&slot.snapshots)
            .as_ref()
            .filter(|known| known.read_at > asked)
            .map(|known| Arc::clone(&known.list));
        if let Some(list) = fresh {
            return Ok(list);
        }
        let forgets = slot.forgets();
        let repo = self.opened(slot, parent).await?;
        let current = lock(&slot.snapshots)
            .as_ref()
            .map(|known| known.files.clone())
            .unwrap_or_default();
        let files = match repo.refresh_snapshots(current).await {
            Ok(files) => files,
            // The cached open can't read the repo any more: it was replaced in
            // place (`restic init` again, another repo copied over it) under a new
            // master key. Open again, once, with the password held, and read the
            // snapshot list from scratch.
            Err(ResticError::Unreadable { .. }) => {
                slot.drop_opened_repo(&self.host.runtime());
                let repo = self.opened(slot, parent).await?;
                repo.refresh_snapshots(Vec::new())
                    .await
                    .map_err(|err| Self::refused(slot, err))?
            }
            Err(err) => return Err(Self::refused(slot, err)),
        };
        // A repo always has a `config`. No snapshots AND no `config` is a repo
        // deleted after browsing: `NotFound`, ❌ never an empty list that reads
        // as "no backups".
        if files.is_empty() && !config_exists(parent.as_ref(), &slot.key.root).await? {
            slot.drop_opened_repo(&self.host.runtime());
            *lock(&slot.snapshots) = None;
            lock(&self.confirmations).remove(&slot.key);
            return Err(VolumeError::NotFound(slot.key.root.display().to_string()));
        }
        let list = Arc::new(snapshot_list_of(&files));
        let held = slot.is_held_for_transfer();
        lock(&slot.latest).rebind(&list, self.tuning.latest_quiet, held);
        let mut known = lock(&slot.snapshots);
        if slot.forgets() != forgets {
            return Err(VolumeError::NeedsPassword { wrong_attempt: false });
        }
        *known = Some(Snapshots {
            files,
            list: Arc::clone(&list),
            read_at: started,
        });
        Ok(list)
    }

    // ── The index ──────────────────────────────────────────────────────────

    /// The repo opened and fully indexed, ready to browse `snapshot`'s trees and
    /// read its files. Loaded on first use, and again when `snapshot` is newer
    /// than the loaded index (its trees are in index files the load never read).
    ///
    /// One load per repo at a time, shared by every caller waiting on it, each
    /// hearing its progress through `on_progress`. A caller that stops waiting
    /// (its future dropped: a pane's cancel) leaves the load, and the last one to
    /// leave cancels it. A reload runs beside the index already loaded, which
    /// keeps serving until the new one is in.
    pub(crate) async fn indexed(
        &self,
        slot: &Arc<RepoSlot>,
        parent: &Arc<dyn Volume>,
        snapshot: &Snapshot,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    ) -> Result<Arc<IndexedRepo>, VolumeError> {
        slot.touch_index();
        if let Some(repo) = lock(&slot.index).serving(parent, snapshot) {
            return Ok(repo);
        }
        let forgets = slot.forgets();
        let password = self.password(slot).await?;
        let waiter = {
            let mut index = lock(&slot.index);
            if let Some(repo) = index.serving(parent, snapshot) {
                return Ok(repo);
            }
            match index.joinable(parent) {
                Some(waiter) => waiter,
                None => self.start_load(slot, &mut index, parent, password, forgets),
            }
        };
        let outcome = waiter.outcome(on_progress).await;
        slot.touch_index();
        outcome.map_err(|err| Self::refused(slot, err))
    }

    /// [`Self::indexed`] again after a read through `stale` hit a pack the
    /// parent says is gone ([`ResticError::PackGone`]): a prune elsewhere
    /// repacked blobs under snapshot ids the index still covers. Lets go of
    /// `stale` unless a reader that hit it too already did, then waits on (or
    /// starts) the load, so readers missing together share one reload.
    pub(crate) async fn reindexed(
        &self,
        slot: &Arc<RepoSlot>,
        parent: &Arc<dyn Volume>,
        snapshot: &Snapshot,
        stale: &Arc<IndexedRepo>,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    ) -> Result<Arc<IndexedRepo>, VolumeError> {
        let gone = lock(&slot.index).let_go_of(stale);
        free_off_the_runtime(&self.host.runtime(), gone);
        self.indexed(slot, parent, snapshot, on_progress).await
    }

    /// Starts a load of `slot`'s index through `parent` and joins it. Runs under
    /// the slot's index lock, which it only takes to file the load: the load
    /// itself runs on its own task.
    fn start_load(
        &self,
        slot: &Arc<RepoSlot>,
        index: &mut IndexSlot,
        parent: &Arc<dyn Volume>,
        password: Zeroizing<String>,
        forgets: u64,
    ) -> Waiter {
        // Restic writes a backup's index files before its snapshot file, so
        // every snapshot listed now has its trees in the index read next.
        let covers: HashSet<String> = lock(&slot.snapshots)
            .as_ref()
            .map(|known| known.list.snapshots().iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default();
        index.generation += 1;
        let generation = index.generation;
        let spec = LoadSpec {
            parent: Arc::clone(parent),
            root: slot.key.root.clone(),
            password,
            runtime: self.host.runtime(),
            window: self.tuning.index_window,
        };
        let installed = Installed {
            slot: Arc::downgrade(slot),
            repos: Arc::downgrade(&self.repos),
            parent: Arc::clone(parent),
            covers,
            generation,
            forgets,
            tuning: self.tuning,
            runtime: self.host.runtime(),
        };
        let load = Load::start(spec, move |outcome| installed.finish(outcome));
        let waiter = load
            .join()
            .expect("a load that just started has nobody to have cancelled it");
        index.loading = Some(load);
        waiter
    }

    /// How many callers wait on `slot`'s load in flight.
    #[cfg(test)]
    pub(crate) fn load_waiters(slot: &RepoSlot) -> usize {
        lock(&slot.index).loading.as_ref().map_or(0, |load| load.waiters())
    }

    /// Whether `slot`'s index is loaded right now.
    #[cfg(test)]
    pub(crate) fn is_indexed(slot: &RepoSlot) -> bool {
        lock(&slot.index).ready.is_some()
    }

    /// How many writes rustic tried through the repos open right now (every one
    /// refused).
    #[cfg(test)]
    pub(crate) fn write_attempts(&self) -> usize {
        self.all_slots()
            .iter()
            .map(|slot| {
                let opened = slot
                    .opened
                    .try_lock()
                    .ok()
                    .and_then(|o| o.as_ref().map(|b| b.value.backend.write_attempts()));
                let indexed = lock(&slot.index)
                    .ready
                    .as_ref()
                    .map(|ready| ready.repo.backend.write_attempts());
                opened.unwrap_or(0) + indexed.unwrap_or(0)
            })
            .sum()
    }
}

/// Whether the repo at `root` on `parent` still has its `config`.
async fn config_exists(parent: &dyn Volume, root: &Path) -> Result<bool, VolumeError> {
    match parent.get_metadata(&root.join("config")).await {
        Ok(_) => Ok(true),
        Err(VolumeError::NotFound(_)) => Ok(false),
        Err(err) => Err(err),
    }
}

fn key(volume_id: &str, repo_root: &Path) -> RepoKey {
    RepoKey {
        volume_id: volume_id.to_string(),
        root: repo_root.to_path_buf(),
    }
}

/// Locks a std mutex, taking the data over from a thread that panicked holding
/// it: every value here stays whole between statements.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything the hub keeps about one repo.
///
/// The async mutexes are per repo, ❌ never the hub's: holding one across an
/// open or a Keychain lookup keeps a second caller for the SAME repo from
/// starting another, and no other repo waits on it.
pub(crate) struct RepoSlot {
    key: RepoKey,
    /// The password that last unlocked it (or came from the store). Wiped on drop.
    password: Mutex<Option<Zeroizing<String>>>,
    /// The credential store had nothing (or a password that stopped working):
    /// not asked again until an unlock.
    store_missed: AtomicBool,
    /// Held while the credential store is asked, so it's asked once.
    store_lookup: tokio::sync::Mutex<()>,
    opened: tokio::sync::Mutex<Option<Bound<Arc<ResticRepo>>>>,
    /// Held across a refresh of the snapshot list, so concurrent refreshes share one.
    refreshing: tokio::sync::Mutex<()>,
    pub(crate) index: Mutex<IndexSlot>,
    snapshots: Mutex<Option<Snapshots>>,
    latest: Mutex<LatestPin>,
    /// When anything last listed, stat'ed, or opened something in this repo.
    last_used: Mutex<Instant>,
    /// When the loaded index last served (or was installed or released by) a
    /// read, apart from `last_used` so a root listing, which never needs the
    /// index, doesn't push its idle drop out.
    index_used: Mutex<Instant>,
    /// How many times the person forgot its password ([`Self::forgets`]).
    forgets: AtomicU64,
}

impl RepoSlot {
    fn new(key: RepoKey) -> Self {
        Self {
            key,
            password: Mutex::new(None),
            store_missed: AtomicBool::new(false),
            store_lookup: tokio::sync::Mutex::new(()),
            opened: tokio::sync::Mutex::new(None),
            refreshing: tokio::sync::Mutex::new(()),
            index: Mutex::new(IndexSlot::default()),
            snapshots: Mutex::new(None),
            latest: Mutex::new(LatestPin::default()),
            last_used: Mutex::new(Instant::now()),
            index_used: Mutex::new(Instant::now()),
            forgets: AtomicU64::new(0),
        }
    }

    /// How many times the person forgot this repo's password. Work that read
    /// it before a forget (an open, a refresh, an index load, a store read, an
    /// unlock) commits only while it's unchanged, so the forget wins over
    /// anything started before it.
    pub(crate) fn forgets(&self) -> u64 {
        self.forgets.load(Ordering::SeqCst)
    }

    fn password_in_memory(&self) -> Option<Zeroizing<String>> {
        lock(&self.password).clone()
    }

    /// Lets go of the opened repo and the loaded index: the repo changed under
    /// them (replaced or deleted). The password stays; the next call opens anew.
    fn drop_opened_repo(&self, runtime: &tokio::runtime::Handle) {
        if let Ok(mut opened) = self.opened.try_lock() {
            *opened = None;
        }
        let gone = lock(&self.index).ready.take();
        free_off_the_runtime(runtime, gone);
    }

    /// Forgets the password held in memory (the person cancelled the prompt).
    /// The credential store's entry, the opened repo, and the index stay.
    pub(crate) fn forget_password(&self) {
        *lock(&self.password) = None;
    }

    pub(crate) fn touch(&self) {
        *lock(&self.last_used) = Instant::now();
    }

    pub(crate) fn last_used(&self) -> Instant {
        *lock(&self.last_used)
    }

    /// Counts as a use of the repo AND of its loaded index.
    pub(crate) fn touch_index(&self) {
        let now = Instant::now();
        *lock(&self.last_used) = now;
        *lock(&self.index_used) = now;
    }

    pub(crate) fn index_last_used(&self) -> Instant {
        *lock(&self.index_used)
    }

    /// The snapshot the `latest` folder shows, in `list`, for a path under
    /// `latest/`: a read there, which holds `latest` in place.
    pub(crate) fn latest_in<'l>(&self, list: &'l SnapshotList) -> Option<&'l Snapshot> {
        self.touch_latest();
        self.latest_shown(list)
    }

    /// The snapshot the `latest` row shows, in `list`, without counting as a
    /// read: listing the root only shows `latest`, and a person refreshing to
    /// see a new backup must see it move.
    pub(crate) fn latest_shown<'l>(&self, list: &'l SnapshotList) -> Option<&'l Snapshot> {
        list.by_id(lock(&self.latest).id()?)
    }

    /// Notes that something read under `latest/` just now, which holds it in
    /// place for the quiet period.
    pub(crate) fn touch_latest(&self) {
        lock(&self.latest).touch();
    }
}

/// Where reads under one snapshot get its index: a read stream keeps one, so a
/// pack gone mid-file loads the index again ([`ResticRepos::reindexed`]).
#[derive(Clone)]
pub(crate) struct IndexSource {
    pub(crate) hub: Arc<ResticRepos>,
    pub(crate) slot: Arc<RepoSlot>,
    pub(crate) parent: Arc<dyn Volume>,
    pub(crate) snapshot: Snapshot,
}

impl IndexSource {
    /// The index holding the snapshot ([`ResticRepos::indexed`]).
    pub(crate) async fn get(
        &self,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    ) -> Result<Arc<IndexedRepo>, VolumeError> {
        self.hub
            .indexed(&self.slot, &self.parent, &self.snapshot, on_progress)
            .await
    }

    /// A fresh index after a read through `stale` found a pack gone
    /// ([`ResticRepos::reindexed`]).
    pub(crate) async fn reload(
        &self,
        stale: &Arc<IndexedRepo>,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    ) -> Result<Arc<IndexedRepo>, VolumeError> {
        self.hub
            .reindexed(&self.slot, &self.parent, &self.snapshot, stale, on_progress)
            .await
    }
}

/// A value opened through one particular parent volume. A parent can be
/// replaced under the same volume id (an SMB share reconnecting), and a value
/// opened through the old one would read through a retired volume forever.
struct Bound<T> {
    parent: Arc<dyn Volume>,
    value: T,
    /// The slot's forget count when it was made: one made before a forget is
    /// never served after it.
    forgets: u64,
}

impl<T> Bound<T> {
    fn new(parent: &Arc<dyn Volume>, value: T, forgets: u64) -> Self {
        Self {
            parent: Arc::clone(parent),
            value,
            forgets,
        }
    }

    fn is_over(&self, parent: &Arc<dyn Volume>) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.parent), Arc::as_ptr(parent))
    }
}

/// The snapshot list read last: rustic's files (handed back on a refresh, so
/// only new ones are read) and the named list built from them.
struct Snapshots {
    files: Vec<SnapshotFile>,
    list: Arc<SnapshotList>,
    /// When the refresh that read it STARTED (took the refresh lock), so only
    /// a caller that asked before then may share it.
    read_at: Instant,
}

// The index the hub keeps (install, cap, idle drop). A child, not a sibling:
// it reads the slot's own state, and a sibling would weld the two modules into
// a cycle (`module-cycles`).
#[path = "index_slot.rs"]
mod index_slot;

// Which snapshot `latest` shows. A child, like `index_slot`.
#[path = "latest_pin.rs"]
mod latest_pin;
use latest_pin::LatestPin;

// Unlocking, filing, forgetting, and looking up a repo's password. A child, so
// it reads the slot's own state.
#[path = "passwords.rs"]
mod passwords;

#[cfg(test)]
#[path = "hub_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
