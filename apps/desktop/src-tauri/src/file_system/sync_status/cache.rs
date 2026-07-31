//! Per-path sync-status cache, keyed by directory.
//!
//! Sync status changes rarely, but the file pane re-asked for every visible path
//! on every listing render and on a 3 s idle poll, so an unchanged Dropbox folder
//! paid a full round of File Provider XPC several times a second. This cache is
//! what turns that into one round per directory.
//!
//! ## Why it's keyed by directory, not by full path
//!
//! Invalidation arrives per directory (`notify_directory_changed`, a cloud action
//! on one file), and it arrives often during a big copy. A flat path map would make
//! every one of those a full scan of the cache; a directory map makes it one hash
//! lookup. Batches are per-directory too, so the grouping costs nothing to build.
//!
//! ## Why the clock is injected
//!
//! TTL behaviour is the whole contract here, and the alternative way to test it is
//! to sleep past a deliberately tiny TTL, which is exactly the flaky pattern
//! `docs/testing.md` bans. The clock closure lets a test step time by an exact
//! amount; production passes `Instant::now`.

use super::SyncStatus;
use cmdr_fs::ignore_poison::IgnorePoison;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long each answer stays good.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ttls {
    /// `Synced` / `OnlineOnly` / `Unknown`: settled answers. A change here is
    /// either a user action (which invalidates explicitly) or an FS event (which
    /// invalidates through the watcher), so this can be generous.
    pub stable: Duration,
    /// `Uploading` / `Downloading`: states that exist to become another state. Short
    /// enough that the badge follows the transfer, long enough to collapse the
    /// pane's render bursts into one query.
    pub transitional: Duration,
}

impl Ttls {
    fn for_status(&self, status: SyncStatus) -> Duration {
        match status {
            SyncStatus::Uploading | SyncStatus::Downloading => self.transitional,
            SyncStatus::Synced | SyncStatus::OnlineOnly | SyncStatus::Unknown => self.stable,
        }
    }
}

type Clock = Box<dyn Fn() -> Instant + Send + Sync>;

pub(super) struct Cache {
    state: Mutex<CacheState>,
    /// Cap on cached entries. Overflow evicts whole directories, least recently
    /// used first: a pane the user left is exactly what we want to forget.
    capacity: usize,
    ttls: Ttls,
    clock: Clock,
}

struct CacheState {
    dirs: HashMap<PathBuf, DirCache>,
    /// Running total across `dirs`, so capacity checks stay O(1).
    entries: usize,
}

struct DirCache {
    last_used: Instant,
    statuses: HashMap<OsString, CacheEntry>,
}

struct CacheEntry {
    status: SyncStatus,
    stored_at: Instant,
}

/// Splits a file path into the (directory, file name) pair the cache keys on.
/// A path with no file name (`/`, `..`) is not cacheable.
fn split(path: &Path) -> Option<(&Path, &std::ffi::OsStr)> {
    Some((path.parent()?, path.file_name()?))
}

impl Cache {
    pub(super) fn new(capacity: usize, ttls: Ttls) -> Self {
        Self::with_clock(capacity, ttls, Box::new(Instant::now))
    }

    pub(super) fn with_clock(capacity: usize, ttls: Ttls, clock: Clock) -> Self {
        Self {
            state: Mutex::new(CacheState {
                dirs: HashMap::new(),
                entries: 0,
            }),
            capacity,
            ttls,
            clock,
        }
    }

    /// The cached status for `path`, or `None` when it's absent or stale. A stale
    /// entry is dropped on the way out so it can't accumulate.
    pub(super) fn get(&self, path: &Path) -> Option<SyncStatus> {
        let (dir, name) = split(path)?;
        let now = (self.clock)();
        let mut state = self.state.lock_ignore_poison();
        let dir_cache = state.dirs.get_mut(dir)?;
        dir_cache.last_used = now;
        let entry = dir_cache.statuses.get(name)?;
        if now.saturating_duration_since(entry.stored_at) < self.ttls.for_status(entry.status) {
            return Some(entry.status);
        }
        dir_cache.statuses.remove(name);
        state.entries -= 1;
        None
    }

    pub(super) fn put(&self, path: &Path, status: SyncStatus) {
        let Some((dir, name)) = split(path) else {
            return;
        };
        let now = (self.clock)();
        let mut state = self.state.lock_ignore_poison();
        let dir_cache = state.dirs.entry(dir.to_path_buf()).or_insert_with(|| DirCache {
            last_used: now,
            statuses: HashMap::new(),
        });
        dir_cache.last_used = now;
        let replaced = dir_cache
            .statuses
            .insert(name.to_os_string(), CacheEntry { status, stored_at: now });
        if replaced.is_none() {
            state.entries += 1;
        }
        self.evict_if_over_capacity(&mut state, dir);
    }

    /// Forgets everything cached for the files directly inside `dir`. Call this
    /// whenever that directory's contents changed.
    pub(super) fn invalidate_dir(&self, dir: &Path) {
        let mut state = self.state.lock_ignore_poison();
        if let Some(removed) = state.dirs.remove(dir) {
            state.entries -= removed.statuses.len();
        }
    }

    /// Forgets one file's status. Call this after an action that changes it
    /// without necessarily producing an FS event (evict, request download).
    pub(super) fn invalidate_path(&self, path: &Path) {
        let Some((dir, name)) = split(path) else {
            return;
        };
        let mut state = self.state.lock_ignore_poison();
        if let Some(dir_cache) = state.dirs.get_mut(dir)
            && dir_cache.statuses.remove(name).is_some()
        {
            state.entries -= 1;
        }
    }

    #[cfg(test)]
    pub(super) fn entry_count(&self) -> usize {
        self.state.lock_ignore_poison().entries
    }

    /// Drops least-recently-used directories until the entry count fits. `keep` is
    /// the directory just written to, which would otherwise be a plausible victim
    /// of its own insert on a tiny cache.
    fn evict_if_over_capacity(&self, state: &mut CacheState, keep: &Path) {
        while state.entries > self.capacity {
            let victim = state
                .dirs
                .iter()
                .filter(|(dir, _)| dir.as_path() != keep)
                .min_by_key(|(_, dir_cache)| dir_cache.last_used)
                .map(|(dir, _)| dir.clone());
            let Some(victim) = victim else {
                return;
            };
            if let Some(removed) = state.dirs.remove(&victim) {
                state.entries -= removed.statuses.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const TTLS: Ttls = Ttls {
        stable: Duration::from_secs(60),
        transitional: Duration::from_secs(2),
    };

    /// A clock the test steps by hand, so TTL assertions never depend on wall time.
    struct TestClock(Arc<Mutex<Instant>>);

    impl TestClock {
        fn new() -> (Self, Clock) {
            let shared = Arc::new(Mutex::new(Instant::now()));
            let read = Arc::clone(&shared);
            (Self(shared), Box::new(move || *read.lock_ignore_poison()))
        }

        fn advance(&self, by: Duration) {
            let mut now = self.0.lock_ignore_poison();
            *now += by;
        }
    }

    fn cache_with_clock(capacity: usize) -> (Cache, TestClock) {
        let (clock, read) = TestClock::new();
        (Cache::with_clock(capacity, TTLS, read), clock)
    }

    #[test]
    fn a_stored_status_reads_back() {
        let (cache, _clock) = cache_with_clock(64);
        cache.put(Path::new("/cloud/a.txt"), SyncStatus::Synced);
        assert_eq!(cache.get(Path::new("/cloud/a.txt")), Some(SyncStatus::Synced));
        assert_eq!(cache.get(Path::new("/cloud/b.txt")), None, "an unseen path is a miss");
    }

    /// Both TTL tiers expire, and the transitional one expires first: an
    /// `Uploading` badge must not outlive the upload by a minute.
    #[test]
    fn transitional_entries_expire_before_stable_ones() {
        let (cache, clock) = cache_with_clock(64);
        cache.put(Path::new("/cloud/up.txt"), SyncStatus::Uploading);
        cache.put(Path::new("/cloud/done.txt"), SyncStatus::Synced);

        clock.advance(TTLS.transitional + Duration::from_millis(1));
        assert_eq!(
            cache.get(Path::new("/cloud/up.txt")),
            None,
            "the uploading entry aged out"
        );
        assert_eq!(
            cache.get(Path::new("/cloud/done.txt")),
            Some(SyncStatus::Synced),
            "the settled entry is still good"
        );

        clock.advance(TTLS.stable);
        assert_eq!(
            cache.get(Path::new("/cloud/done.txt")),
            None,
            "the settled entry aged out"
        );
    }

    /// A read of a stale entry drops it, so an abandoned pane's paths can't sit in
    /// the cache forever holding capacity.
    #[test]
    fn reading_a_stale_entry_drops_it() {
        let (cache, clock) = cache_with_clock(64);
        cache.put(Path::new("/cloud/up.txt"), SyncStatus::Uploading);
        assert_eq!(cache.entry_count(), 1);
        clock.advance(TTLS.transitional + Duration::from_millis(1));
        assert_eq!(cache.get(Path::new("/cloud/up.txt")), None);
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn invalidate_dir_forgets_that_directory_only() {
        let (cache, _clock) = cache_with_clock(64);
        cache.put(Path::new("/cloud/a.txt"), SyncStatus::Synced);
        cache.put(Path::new("/cloud/b.txt"), SyncStatus::OnlineOnly);
        cache.put(Path::new("/other/c.txt"), SyncStatus::Synced);

        cache.invalidate_dir(Path::new("/cloud"));

        assert_eq!(cache.get(Path::new("/cloud/a.txt")), None);
        assert_eq!(cache.get(Path::new("/cloud/b.txt")), None);
        assert_eq!(cache.get(Path::new("/other/c.txt")), Some(SyncStatus::Synced));
        assert_eq!(cache.entry_count(), 1);
    }

    #[test]
    fn invalidate_path_forgets_one_file_only() {
        let (cache, _clock) = cache_with_clock(64);
        cache.put(Path::new("/cloud/a.txt"), SyncStatus::Synced);
        cache.put(Path::new("/cloud/b.txt"), SyncStatus::Synced);

        cache.invalidate_path(Path::new("/cloud/a.txt"));

        assert_eq!(cache.get(Path::new("/cloud/a.txt")), None);
        assert_eq!(cache.get(Path::new("/cloud/b.txt")), Some(SyncStatus::Synced));
        assert_eq!(cache.entry_count(), 1);
    }

    /// The cache is bounded: browsing many folders evicts the ones left behind,
    /// never the one being written to.
    #[test]
    fn evicts_least_recently_used_directories_at_capacity() {
        let (cache, clock) = cache_with_clock(4);
        for dir in 0..8 {
            cache.put(Path::new(&format!("/dir{dir}/file.txt")), SyncStatus::Synced);
            clock.advance(Duration::from_millis(1));
        }
        assert!(
            cache.entry_count() <= 4,
            "cache holds {} entries, capacity is 4",
            cache.entry_count()
        );
        assert_eq!(
            cache.get(Path::new("/dir7/file.txt")),
            Some(SyncStatus::Synced),
            "the newest directory survived"
        );
        assert_eq!(cache.get(Path::new("/dir0/file.txt")), None, "the oldest was evicted");
    }

    #[test]
    fn a_path_without_a_file_name_is_not_cached() {
        let (cache, _clock) = cache_with_clock(64);
        cache.put(Path::new("/"), SyncStatus::Synced);
        assert_eq!(cache.entry_count(), 0);
        assert_eq!(cache.get(Path::new("/")), None);
    }
}
