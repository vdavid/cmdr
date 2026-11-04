//! A recents list that survives a restart.
//!
//! Three dialogs keep one: Search ("Recent searches"), Selection ("Recent
//! selections"), and Go to path ("Recent paths"). They agree on everything except
//! what an entry IS and what makes two entries the same, so that's all
//! [`RecentEntry`] asks for; [`RecentsFile`] owns the rest: one durable JSON file and
//! a quarantine for a file it can't read, over the list rules every host shares
//! (newest first, dedupe on add, cap from the tail), which are `cmdr-recents`'.
//!
//! Declare one per list as a `static` (`RecentsFile::new()` is `const`) and call it
//! from the IPC layer. The cap is a per-call argument, never a property of the
//! store: Search and Selection read theirs from a live setting, Go to path passes a
//! fixed const.
//!
//! Rationale and the lock discipline: `DETAILS.md`.

mod persistence;

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cmdr_recents::{RecentItem, add_to, dedupe_keeping_newest, remove_from, trim_to};

use crate::ignore_poison::IgnorePoison;

/// One entry in a recents list, plus the handful of facts [`RecentsFile`] needs to
/// keep a file of them. Its id and dedupe key are [`cmdr_recents::RecentItem`]'s, the
/// list rules' half.
pub trait RecentEntry: RecentItem + Clone + Send + Serialize + DeserializeOwned + 'static {
    /// Filename inside `{app_data_dir}/`.
    const FILENAME: &'static str;
    /// `log::warn!` target for this list's diagnostics.
    const LOG_TARGET: &'static str;
    /// How the list names itself mid-sentence in a log line ("search history").
    const LOG_NAME: &'static str;
    /// Which file the recovery notice and the security log name when this list's file can't be
    /// read (`crate::state_recovery`).
    const STATE_FILE: crate::state_recovery::StateFile;
    /// Bump when this list's on-disk shape changes in a way an older build can't
    /// read. A file stamped with an unknown version is quarantined, not migrated.
    const SCHEMA_VERSION: u32 = 1;
}

/// A recents list of `E`, cached in memory and backed by one JSON file in the app
/// data dir.
pub struct RecentsFile<E: RecentEntry> {
    /// The list itself, newest first. Mirrors the file; loaded at startup.
    cached: Mutex<Vec<E>>,
    /// Serializes the flush from cache to disk, so two IPC commands landing
    /// together can't clobber each other's write.
    disk: Mutex<()>,
}

impl<E: RecentEntry> Default for RecentsFile<E> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// The list itself, driven by an explicit path. Production calls the app-bound
// facade further down; taking the path as an argument is what lets a test (this
// module's, and a consumer's) run the whole cycle against a tempdir. A `None` path
// means the app data dir didn't resolve: the in-memory list still updates, only
// the disk write is skipped.
// ---------------------------------------------------------------------------

impl<E: RecentEntry> RecentsFile<E> {
    /// An empty list. `const` so each consumer can declare its list as a `static`.
    pub const fn new() -> Self {
        Self {
            cached: Mutex::new(Vec::new()),
            disk: Mutex::new(()),
        }
    }

    /// A snapshot of the list, newest first. `limit = None` returns all of it.
    pub fn entries(&self, limit: Option<usize>) -> Vec<E> {
        let cached = self.cached.lock_ignore_poison();
        match limit {
            Some(n) => cached.iter().take(n).cloned().collect(),
            None => cached.clone(),
        }
    }

    pub(crate) fn load_at(&self, path: &Path) {
        self.load_seeded_at(path, Vec::new);
    }

    /// [`Self::load_at`], except a list with no file yet starts from `seed`.
    pub(crate) fn load_seeded_at(&self, path: &Path, seed: impl FnOnce() -> Vec<E>) {
        // The disk guard is released before the cache guard is taken: no code path
        // in here ever holds both, which is what keeps two locks deadlock-free.
        let loaded = {
            let _disk = self.disk.lock_ignore_poison();
            persistence::read::<E>(path)
        };
        *self.cached.lock_ignore_poison() = match loaded {
            crate::state_recovery::StateRead::Missing => seed(),
            other => other.into_value().unwrap_or_default(),
        };
    }

    pub(crate) fn add_at(&self, path: Option<&Path>, entry: E, max_count: usize) {
        self.update(path, "an add", |entries| {
            add_to(entries, entry, max_count, || uuid::Uuid::new_v4().to_string());
            true
        });
    }

    pub(crate) fn remove_at(&self, path: Option<&Path>, id: &str) {
        self.update(path, "a remove", |entries| remove_from(entries, id));
    }

    pub(crate) fn clear_at(&self, path: Option<&Path>) {
        self.update(path, "a clear", |entries| {
            entries.clear();
            // Always flush: an empty file is the point, so a later write can't race
            // a missing-file load.
            true
        });
    }

    /// Rewrites entries in place (`change` answers whether it changed one), then
    /// drops any later entry whose dedupe key an earlier one now holds, so two rows
    /// a rewrite made equal stay one, the newer.
    pub(crate) fn rewrite_at(&self, path: Option<&Path>, mut change: impl FnMut(&mut E) -> bool) {
        self.update(path, "a rewrite", |entries| {
            let mut changed = false;
            for entry in entries.iter_mut() {
                changed |= change(entry);
            }
            if changed {
                dedupe_keeping_newest(entries);
            }
            changed
        });
    }

    pub(crate) fn apply_max_count_at(&self, path: Option<&Path>, max_count: usize) {
        self.update(path, "a cap change", |entries| trim_to(entries, max_count));
    }

    /// Mutates the cached list, then flushes it. `change` returns `false` to say
    /// nothing moved, which skips the disk write.
    ///
    /// The cache guard is born and dies inside this function and the snapshot it
    /// hands on is owned, so no disk work can run while the guard is alive. That's
    /// structural: there's no discipline left for a caller to remember.
    fn update(&self, path: Option<&Path>, op: &str, change: impl FnOnce(&mut Vec<E>) -> bool) {
        let snapshot = {
            let mut cached = self.cached.lock_ignore_poison();
            if !change(&mut cached) {
                return;
            }
            cached.clone()
        };

        let Some(path) = path else {
            return;
        };

        let _disk = self.disk.lock_ignore_poison();
        if let Err(e) = persistence::write::<E>(path, &snapshot) {
            log::warn!(target: E::LOG_TARGET, "Couldn't write {} after {op}: {e}", E::LOG_NAME);
        }
    }
}

// ---------------------------------------------------------------------------
// The same operations against the app data dir. The IPC layer calls these, and
// they're the only part of this module that knows about Tauri.
// ---------------------------------------------------------------------------

impl<E: RecentEntry> RecentsFile<E> {
    /// Loads the file into the cache. Call once at startup; safe to call again to
    /// refresh when the cache is suspected stale.
    pub fn load<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        if let Some(path) = self.path(app) {
            self.load_at(&path);
        }
    }

    /// [`Self::load`], except a list with no file yet (a new install) starts from
    /// `seed`, newest first. It's written with the first change, like any entry; a
    /// file that's there, emptied or set aside, never takes the seed.
    pub fn load_seeded<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, seed: impl FnOnce() -> Vec<E>) {
        if let Some(path) = self.path(app) {
            self.load_seeded_at(&path, seed);
        }
    }

    /// Puts `entry` on top, dropping any earlier entry with the same
    /// [`RecentItem::dedupe_key`], and trims the tail to `max_count`.
    /// `max_count = 0` empties the list and drops the entry.
    ///
    /// The disk write is best-effort: a failure is logged and the in-memory list
    /// stays consistent either way.
    pub fn add<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, entry: E, max_count: usize) {
        self.add_at(self.path(app).as_deref(), entry, max_count);
    }

    /// Removes the entry with this id. Does nothing, disk included, when it's absent.
    pub fn remove<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, id: &str) {
        self.remove_at(self.path(app).as_deref(), id);
    }

    /// Empties the list. The file is rewritten empty rather than deleted, so a later
    /// write can't race a missing-file load.
    pub fn clear<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        self.clear_at(self.path(app).as_deref());
    }

    /// Applies a freshly-tuned cap, for a setting the user just changed. The file is
    /// rewritten only when the cap actually drops entries.
    pub fn apply_max_count<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, max_count: usize) {
        self.apply_max_count_at(self.path(app).as_deref(), max_count);
    }

    /// Where this list lives, or `None` when the app data dir can't be resolved.
    fn path<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> Option<PathBuf> {
        crate::config::resolved_app_data_dir(app)
            .ok()
            .map(|dir| dir.join(E::FILENAME))
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// A minimal entry: dedupes on `key`, which is all the store looks at.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
    pub(crate) struct TestEntry {
        pub id: String,
        pub key: String,
    }

    impl RecentEntry for TestEntry {
        const FILENAME: &'static str = "test-recents.json";
        const LOG_TARGET: &'static str = "recents::test";
        const LOG_NAME: &'static str = "test recents";
        const STATE_FILE: crate::state_recovery::StateFile = crate::state_recovery::StateFile::SearchHistory;
    }

    impl RecentItem for TestEntry {
        fn id(&self) -> &str {
            &self.id
        }

        fn set_id(&mut self, id: String) {
            self.id = id;
        }

        fn dedupe_key(&self) -> String {
            self.key.clone()
        }
    }

    /// An entry with a fresh id and the given dedupe key.
    pub(crate) fn entry(key: &str) -> TestEntry {
        TestEntry {
            id: uuid::Uuid::new_v4().to_string(),
            key: key.to_string(),
        }
    }

    /// The dedupe keys of a list, newest first: what most assertions care about.
    pub(crate) fn keys(entries: &[TestEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.key.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{TestEntry, entry, keys};
    use super::*;

    fn list() -> RecentsFile<TestEntry> {
        RecentsFile::new()
    }

    // -- entries(limit) --

    #[test]
    fn entries_honors_a_limit_and_stays_newest_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        let list = list();
        for i in 0..5 {
            list.add_at(Some(&path), entry(&format!("q{i}")), 10);
        }

        assert_eq!(keys(&list.entries(Some(2))), ["q4", "q3"]);
        assert_eq!(list.entries(Some(99)).len(), 5, "a limit past the end returns all");
        assert_eq!(list.entries(None).len(), 5);
    }

    // -- the full cycle against a real file --

    #[test]
    fn an_add_persists_and_reloads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        let writer = list();
        writer.add_at(Some(&path), entry("a"), 10);
        writer.add_at(Some(&path), entry("b"), 10);

        // A second list over the same file sees what the first one wrote.
        let reader = list();
        reader.load_at(&path);
        assert_eq!(keys(&reader.entries(None)), ["b", "a"]);
    }

    #[test]
    fn a_remove_persists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        let writer = list();
        writer.add_at(Some(&path), entry("a"), 10);
        writer.add_at(Some(&path), entry("b"), 10);
        let doomed = writer.entries(None)[0].id.clone();
        writer.remove_at(Some(&path), &doomed);

        let reader = list();
        reader.load_at(&path);
        assert_eq!(keys(&reader.entries(None)), ["a"]);
    }

    #[test]
    fn removing_an_absent_id_leaves_the_file_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        let list = list();
        list.add_at(Some(&path), entry("a"), 10);
        // Delete the file so a write of any kind shows up as the file coming back.
        std::fs::remove_file(&path).expect("remove");

        list.remove_at(Some(&path), "not-in-the-list");

        assert!(!path.exists(), "a no-op remove shouldn't rewrite the file");
    }

    #[test]
    fn a_clear_persists_as_an_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        let writer = list();
        writer.add_at(Some(&path), entry("a"), 10);
        writer.clear_at(Some(&path));

        assert!(path.exists(), "clearing rewrites the file, it doesn't delete it");
        let reader = list();
        reader.load_at(&path);
        assert!(reader.entries(None).is_empty());
    }

    #[test]
    fn a_tightened_cap_persists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        let writer = list();
        for i in 0..5 {
            writer.add_at(Some(&path), entry(&format!("q{i}")), 10);
        }
        writer.apply_max_count_at(Some(&path), 2);

        let reader = list();
        reader.load_at(&path);
        assert_eq!(keys(&reader.entries(None)), ["q4", "q3"]);
    }

    #[test]
    fn a_cap_that_drops_nothing_leaves_the_file_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        let list = list();
        list.add_at(Some(&path), entry("a"), 10);
        // Delete the file so a write of any kind shows up as the file coming back.
        std::fs::remove_file(&path).expect("remove");

        list.apply_max_count_at(Some(&path), 10);

        assert!(!path.exists(), "a cap that drops nothing shouldn't rewrite the file");
    }

    // -- load_seeded_at: a seed for a list that has no file yet --

    #[test]
    fn a_list_with_no_file_starts_from_its_seed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        let list = list();
        list.load_seeded_at(&path, || vec![entry("seed-a"), entry("seed-b")]);

        assert_eq!(keys(&list.entries(None)), ["seed-a", "seed-b"]);
        assert!(
            !path.exists(),
            "the seed is written with the first real change, not on load"
        );
    }

    #[test]
    fn a_seeded_list_keeps_its_seed_once_something_is_added() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        let writer = list();
        writer.load_seeded_at(&path, || vec![entry("seed")]);
        writer.add_at(Some(&path), entry("a"), 10);

        let reader = list();
        reader.load_seeded_at(&path, || vec![entry("other")]);
        assert_eq!(keys(&reader.entries(None)), ["a", "seed"]);
    }

    #[test]
    fn an_emptied_list_is_never_reseeded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        let writer = list();
        writer.load_seeded_at(&path, || vec![entry("seed")]);
        writer.clear_at(Some(&path));

        let reader = list();
        reader.load_seeded_at(&path, || vec![entry("seed")]);
        assert!(reader.entries(None).is_empty());
    }

    #[test]
    fn a_set_aside_list_is_not_seeded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        std::fs::write(&path, "{not json").expect("write garbage");
        let _notices = crate::state_recovery::testing::capture_recoveries();

        let list = list();
        list.load_seeded_at(&path, || vec![entry("seed")]);
        assert!(list.entries(None).is_empty());
    }

    #[test]
    fn an_unresolvable_path_still_updates_the_list_in_memory() {
        // The app data dir failing to resolve costs persistence, never the session.
        let list = list();
        list.add_at(None, entry("a"), 10);
        assert_eq!(keys(&list.entries(None)), ["a"]);
    }
}
