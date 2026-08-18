//! A recents list that survives a restart.
//!
//! Three dialogs keep one: Search ("Recent searches"), Selection ("Recent
//! selections"), and Go to path ("Recent paths"). They agree on everything except
//! what an entry IS and what makes two entries the same, so that's all
//! [`RecentEntry`] asks for; [`RecentsFile`] owns the rest: newest first, dedupe on
//! add, cap from the tail, one durable JSON file, and a quarantine for a file it
//! can't read.
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

use crate::ignore_poison::IgnorePoison;

/// One entry in a recents list, plus the handful of facts [`RecentsFile`] needs to
/// keep a list of them.
pub trait RecentEntry: Clone + Send + Serialize + DeserializeOwned + 'static {
    /// Filename inside `{app_data_dir}/`.
    const FILENAME: &'static str;
    /// `log::warn!` target for this list's diagnostics.
    const LOG_TARGET: &'static str;
    /// How the list names itself mid-sentence in a log line ("search history").
    const LOG_NAME: &'static str;
    /// Bump when this list's on-disk shape changes in a way an older build can't
    /// read. A file stamped with an unknown version is quarantined, not migrated.
    const SCHEMA_VERSION: u32 = 1;

    /// The entry's unique id, which the frontend removes by.
    fn id(&self) -> &str;

    /// Replaces the id. Called only when a caller hands over an id the list already
    /// holds, so two rows can't share one.
    fn set_id(&mut self, id: String);

    /// What makes two entries "the same thing". Adding an entry drops every earlier
    /// one with an equal key, so the newest copy ends up on top.
    fn dedupe_key(&self) -> String;
}

/// A recents list of `E`, cached in memory and backed by one JSON file in the app
/// data dir.
pub struct RecentsFile<E: RecentEntry> {
    /// The list itself, newest first. Mirrors the file; loaded at startup.
    cached: Mutex<Vec<E>>,
    /// Serializes the cache to disk flush, so two IPC commands landing together
    /// can't clobber each other's write.
    disk: Mutex<()>,
}

impl<E: RecentEntry> Default for RecentsFile<E> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// The list itself, driven by an explicit path. Production calls the app-bound
// facade further down; taking the path as an argument is what lets a test — this
// module's and a consumer's — run the whole cycle against a tempdir. A `None` path
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
        // The disk guard is released before the cache guard is taken: no code path
        // in here ever holds both, which is what keeps two locks deadlock-free.
        let loaded = {
            let _disk = self.disk.lock_ignore_poison();
            persistence::read::<E>(path)
        };
        *self.cached.lock_ignore_poison() = loaded;
    }

    pub(crate) fn add_at(&self, path: Option<&Path>, entry: E, max_count: usize) {
        self.update(path, "an add", |entries| {
            add_to(entries, entry, max_count);
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

    pub(crate) fn apply_max_count_at(&self, path: Option<&Path>, max_count: usize) {
        self.update(path, "a cap change", |entries| {
            let before = entries.len();
            trim_to(entries, max_count);
            entries.len() != before
        });
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

    /// Puts `entry` on top, dropping any earlier entry with the same
    /// [`RecentEntry::dedupe_key`], and trims the tail to `max_count`.
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

// ---------------------------------------------------------------------------
// Pure list operations, so dedupe and cap are testable without a file.
// ---------------------------------------------------------------------------

/// Puts `entry` on top, dropping earlier entries with the same dedupe key, and
/// enforces `max_count` from the tail (the oldest end).
fn add_to<E: RecentEntry>(entries: &mut Vec<E>, mut entry: E, max_count: usize) {
    // A cap of zero means the user wants no list at all: drop what's there and the
    // new entry with it.
    if max_count == 0 {
        entries.clear();
        return;
    }

    // Drop every earlier copy, not just the first: a list that somehow grew two
    // rows for one key heals here.
    let key = entry.dedupe_key();
    entries.retain(|e| e.dedupe_key() != key);

    // Keep ids unique even when the caller hands over one we already hold.
    if entries.iter().any(|e| e.id() == entry.id()) {
        entry.set_id(uuid::Uuid::new_v4().to_string());
    }

    entries.insert(0, entry);
    trim_to(entries, max_count);
}

/// Drops entries past `max_count`, oldest first. A cap of zero empties the list.
fn trim_to<E>(entries: &mut Vec<E>, max_count: usize) {
    if entries.len() > max_count {
        entries.truncate(max_count);
    }
}

/// Returns whether anything was actually removed.
fn remove_from<E: RecentEntry>(entries: &mut Vec<E>, id: &str) -> bool {
    let before = entries.len();
    entries.retain(|e| e.id() != id);
    entries.len() != before
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

    // -- add_to: dedupe, move-to-top, cap --

    #[test]
    fn add_puts_the_newest_entry_on_top() {
        let mut entries = Vec::new();
        add_to(&mut entries, entry("a"), 10);
        add_to(&mut entries, entry("b"), 10);
        assert_eq!(keys(&entries), ["b", "a"]);
    }

    #[test]
    fn add_dedupes_by_key_and_moves_to_top() {
        let mut entries = Vec::new();
        add_to(&mut entries, entry("a"), 10);
        add_to(&mut entries, entry("b"), 10);

        let again = entry("a");
        let again_id = again.id.clone();
        add_to(&mut entries, again, 10);

        assert_eq!(keys(&entries), ["a", "b"], "the duplicate should have collapsed");
        assert_eq!(entries[0].id, again_id, "the newest copy should win");
    }

    #[test]
    fn add_collapses_every_earlier_copy_of_a_key() {
        // Self-heal: a list that somehow holds two copies of one key comes back with
        // one, not one fewer.
        let mut entries = vec![entry("a"), entry("a"), entry("b")];
        add_to(&mut entries, entry("a"), 10);
        assert_eq!(keys(&entries), ["a", "b"]);
    }

    #[test]
    fn add_enforces_the_cap_by_dropping_the_oldest() {
        let mut entries = Vec::new();
        for i in 0..5 {
            add_to(&mut entries, entry(&format!("q{i}")), 3);
        }
        assert_eq!(keys(&entries), ["q4", "q3", "q2"]);
    }

    #[test]
    fn add_with_a_zero_cap_empties_the_list() {
        let mut entries = vec![entry("a")];
        add_to(&mut entries, entry("b"), 0);
        assert!(entries.is_empty(), "a zero cap should clear the list");
    }

    #[test]
    fn add_assigns_a_fresh_id_when_the_caller_collides() {
        let mut entries = Vec::new();
        let mut first = entry("a");
        first.id = "fixed-id".to_string();
        add_to(&mut entries, first, 10);

        let mut second = entry("b");
        second.id = "fixed-id".to_string();
        add_to(&mut entries, second, 10);

        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].id, entries[1].id);
    }

    // -- trim_to --

    #[test]
    fn trim_drops_the_oldest() {
        let mut entries = vec![entry("c"), entry("b"), entry("a")];
        trim_to(&mut entries, 2);
        assert_eq!(keys(&entries), ["c", "b"]);
    }

    #[test]
    fn trim_to_zero_empties_the_list() {
        let mut entries = vec![entry("a")];
        trim_to(&mut entries, 0);
        assert!(entries.is_empty());
    }

    #[test]
    fn trim_leaves_a_list_under_the_cap_alone() {
        let mut entries = vec![entry("b"), entry("a")];
        trim_to(&mut entries, 10);
        assert_eq!(keys(&entries), ["b", "a"]);
    }

    // -- remove_from --

    #[test]
    fn remove_reports_whether_it_found_anything() {
        let mut entries = vec![entry("a")];
        entries[0].id = "abc".to_string();

        assert!(remove_from(&mut entries, "abc"));
        assert!(entries.is_empty());
        assert!(!remove_from(&mut entries, "abc"), "a second remove finds nothing");
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

    #[test]
    fn an_unresolvable_path_still_updates_the_list_in_memory() {
        // The app data dir failing to resolve costs persistence, never the session.
        let list = list();
        list.add_at(None, entry("a"), 10);
        assert_eq!(keys(&list.entries(None)), ["a"]);
    }
}
