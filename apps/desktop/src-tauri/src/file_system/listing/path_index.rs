//! Finding a listing entry BY PATH, without walking the listing.
//!
//! Tag enrichment is the caller that makes this matter. The frontend fills
//! Finder tags in chunks of 500 paths and sweeps a whole directory that way, so
//! a path-keyed lookup that scans runs once per updated path per chunk: at
//! 75,000 entries that is 64 ms of listing walking per chunk, under the cache's
//! WRITE lock, times 150 chunks to cover the directory
//! (`docs/notes/listing-wedge-impact-2026-08-22.md`).
//!
//! So a listing materializes the answer once, the same way it does for row
//! numbers (`visible_rows.rs`), and a chunk of updates costs a hash each.

use std::hash::{BuildHasher, RandomState};
use std::sync::RwLock;

use crate::file_system::listing::metadata::FileEntry;
use crate::ignore_poison::RwLockIgnorePoison;

/// Batch size from which building the map beats scanning for each path.
///
/// Derived from both halves being linear in the listing (release build, M1 Max,
/// 2026-08-22, 75,000 synthetic entries): building costs ~65 ns per entry, and
/// one scan costs ~3.4 ns per entry examined, so `k` scans reach the build's
/// price at `k ≈ 2 × 65 / 3.4 ≈ 38` — a constant, because the listing size
/// cancels. Under it a batch is cheaper off a scan; over it the map wins on the
/// batch alone, before any reuse.
///
/// ⚠️ The map is reused by every later batch, so this only ever decides whether
/// the FIRST small batch on an untouched listing pays to build one. A tag toggle
/// on a right-clicked file must not walk a 300,000-entry listing into a map it
/// then uses once.
const BUILD_FROM_BATCH_SIZE: usize = 32;

/// The per-listing map from entry path to entry index, built on first ask.
pub(crate) struct PathIndexCache {
    map: RwLock<Option<PathIndex>>,
}

impl PathIndexCache {
    pub(crate) fn new() -> Self {
        Self { map: RwLock::new(None) }
    }

    /// Drops the map. Called whenever a listing's entries are handed out for
    /// mutation, which is the only thing that can move an entry's index.
    pub(crate) fn invalidate(&self) {
        *self.map.write_ignore_poison() = None;
    }

    /// Entry indices for `paths`, in order, `None` where the listing has no such
    /// path (scrolled away, already removed, or a caller aiming at the wrong
    /// listing).
    ///
    /// ⚠️ Callers must hold the `LISTING_CACHE` lock covering `entries`, which is
    /// what makes an index still true when it comes back: mutation needs the
    /// write lock, so entries can't move under a resolve.
    ///
    /// One build decision per BATCH rather than per path: see
    /// [`BUILD_FROM_BATCH_SIZE`].
    pub(crate) fn resolve<'a>(
        &self,
        entries: &[FileEntry],
        paths: impl ExactSizeIterator<Item = &'a str>,
    ) -> Vec<Option<usize>> {
        if paths.len() >= BUILD_FROM_BATCH_SIZE || self.map.read_ignore_poison().is_some() {
            let mut map = self.map.write_ignore_poison();
            let map = map.get_or_insert_with(|| PathIndex::build(entries));
            return paths.map(|path| map.index_of(entries, path)).collect();
        }
        paths.map(|path| scan_for_path(entries, path)).collect()
    }

    /// The index of one path, riding a map that already exists and otherwise
    /// scanning.
    ///
    /// **A single path never builds a map.** One lookup is far under
    /// [`BUILD_FROM_BATCH_SIZE`], and the watcher's callers arrive one path per
    /// event: making each of them build would have a modify on an untouched
    /// 300,000-entry listing pay ~20 ms for a map it uses once, and (for the
    /// mutating ones) drops on the way out. What it does buy is the sweep's map:
    /// while tag enrichment is walking a big directory, every watcher event
    /// landing in it is a hash rather than a walk.
    ///
    /// ⚠️ Same lock contract as [`Self::resolve`]: the caller holds the
    /// `LISTING_CACHE` lock covering `entries`. ❗ And a MUTATING caller must ask
    /// BEFORE it takes `entries_mut`, which drops the map.
    pub(crate) fn resolve_one(&self, entries: &[FileEntry], path: &str) -> Option<usize> {
        if let Some(map) = self.map.read_ignore_poison().as_ref() {
            return map.index_of(entries, path);
        }
        scan_for_path(entries, path)
    }
}

impl Default for PathIndexCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Path hashes paired with the entry they belong to, sorted by hash.
///
/// **Twelve bytes per entry, and no second copy of the paths.** A
/// `HashMap<String, usize>` would hold every path twice (~30 MB against a
/// 300,000-entry listing whose entries are themselves ~65 MB); this is 3.6 MB,
/// and it builds in one sequential pass plus an integer sort, so it costs the
/// same whatever order the pane sorts by.
///
/// **Colliding hashes are resolved, not assumed away.** Equal hashes land
/// adjacent, so a lookup walks that run and compares the real path. A collision
/// therefore costs an extra comparison and nothing else.
struct PathIndex {
    by_hash: Vec<(u64, u32)>,
    /// The hasher the entries were indexed with, so lookups hash the same way.
    hasher: RandomState,
}

impl PathIndex {
    fn build(entries: &[FileEntry]) -> Self {
        let hasher = RandomState::new();
        let mut by_hash: Vec<(u64, u32)> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                #[cfg(test)]
                lookup_probe::record_examined();
                (hasher.hash_one(entry.path.as_str()), index as u32)
            })
            .collect();
        by_hash.sort_unstable();
        #[cfg(test)]
        lookup_probe::record_build();
        Self { by_hash, hasher }
    }

    fn index_of(&self, entries: &[FileEntry], path: &str) -> Option<usize> {
        let hash = self.hasher.hash_one(path);
        let first = self.by_hash.partition_point(|&(candidate, _)| candidate < hash);
        self.by_hash[first..]
            .iter()
            .take_while(|&&(candidate, _)| candidate == hash)
            .map(|&(_, index)| index as usize)
            .find(|&index| {
                #[cfg(test)]
                lookup_probe::record_examined();
                entries[index].path == path
            })
    }
}

/// The whole-listing walk, for a batch too small to pay for a map.
fn scan_for_path(entries: &[FileEntry], path: &str) -> Option<usize> {
    entries.iter().position(|entry| {
        #[cfg(test)]
        lookup_probe::record_examined();
        entry.path == path
    })
}

/// Counts entries a path lookup touched on THIS thread, and maps built, so a
/// test can pin what a batch of updates costs.
///
/// The defect these guard is a scan COUNT, invisible in a unit test's wall clock
/// on a small fixture and 64 ms of write-locked listing walking per enrichment
/// chunk on a real one. Thread-local for the same reason
/// `visible_rows::scan_probe` is: `cargo test` runs the crate's tests as threads
/// in one process, and a process-wide counter would mix every concurrent
/// listing test's work into the reading.
#[cfg(test)]
pub(crate) mod lookup_probe {
    use std::cell::Cell;

    thread_local! {
        static EXAMINED: Cell<u64> = const { Cell::new(0) };
        static BUILDS: Cell<u64> = const { Cell::new(0) };
    }

    /// One entry hashed into a map, or compared against a wanted path.
    pub(crate) fn record_examined() {
        EXAMINED.with(|c| c.set(c.get() + 1));
    }

    /// One map built from scratch.
    pub(crate) fn record_build() {
        BUILDS.with(|c| c.set(c.get() + 1));
    }

    /// Entries this thread has examined so far. Take a `before` and an `after`
    /// and subtract.
    pub(crate) fn examined() -> u64 {
        EXAMINED.with(Cell::get)
    }

    /// Maps this thread has built so far.
    pub(crate) fn builds() -> u64 {
        BUILDS.with(Cell::get)
    }
}
