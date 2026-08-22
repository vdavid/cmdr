//! What a pane's row index MEANS, and the map that answers it in constant time.
//!
//! A pane numbers its rows over the entries it is SHOWING, so row 7 is the
//! seventh visible entry and not `entries[7]`. Two things can leave an entry out:
//! it's a dotfile and the user hasn't asked for hidden files, or it's scratch a
//! running operation owns (`file_system::staging`). Answering "which entry is
//! row 7" by walking the entries and counting is what a listing accessor used to
//! do, and at the bottom of a 74,144-entry directory that walk, times the ~100
//! rows a visible range covers, times an index event every couple of seconds,
//! stopped the app answering IPC at all
//! (`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`).
//!
//! So a listing materializes the answer once and every accessor indexes into it.
//!
//! ## Why the map splits settled rows from candidates
//!
//! Most of the predicate is stable: a name's dotfile-ness never changes, and
//! `include_hidden` is part of the cache key. The scratch half is NOT — a copy
//! finishing un-hides its leftover with no change to the listing at all, and
//! nothing calls us when that happens (the ownership signal is a `Weak` that just
//! stops upgrading, `cmdr_fs::staging`).
//!
//! Rather than hunt for every event that could flip it, the map keeps the names
//! that could EVER be hidden that way in a short side list and re-asks about
//! those, and only those, on every read. `staging::could_be_hidden_from_listings`
//! is what makes that sound: `is_hidden_from_listings` is gated on it, so a name
//! it rejects is settled forever. In a real directory that side list is empty and
//! a row lookup is one array index.

use std::sync::{RwLock, RwLockReadGuard};

use crate::ignore_poison::RwLockIgnorePoison;

use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::staging;

/// The per-listing store: one map per `include_hidden` value, built on first ask.
///
/// Two slots rather than one keyed slot so a pane that toggles hidden files, or
/// two readers disagreeing about the flag mid-toggle, can never be handed the map
/// built for the other answer.
pub(crate) struct VisibleRowsCache {
    slots: [RwLock<Option<VisibleMap>>; 2],
}

impl VisibleRowsCache {
    pub(crate) fn new() -> Self {
        Self {
            slots: [RwLock::new(None), RwLock::new(None)],
        }
    }

    /// Drops both maps. Called whenever a listing's entries are handed out for
    /// mutation, which is the only thing that can change the settled half.
    pub(crate) fn invalidate(&self) {
        for slot in &self.slots {
            *slot.write_ignore_poison() = None;
        }
    }

    /// The rows a pane with this `include_hidden` is showing.
    ///
    /// ⚠️ Callers must already hold the `LISTING_CACHE` read lock (every accessor
    /// does). That's what makes the map stable for the lifetime of the returned
    /// value: mutation needs the cache WRITE lock, so `entries` cannot move under
    /// a reader and a map that validates here stays valid.
    pub(crate) fn rows<'a>(&'a self, entries: &'a [FileEntry], include_hidden: bool) -> VisibleRows<'a> {
        let slot = &self.slots[usize::from(include_hidden)];
        {
            let map = slot.read_ignore_poison();
            if map.is_some() {
                return VisibleRows::new(entries, map);
            }
        }
        {
            let mut map = slot.write_ignore_poison();
            if map.is_none() {
                *map = Some(VisibleMap::build(entries, include_hidden));
            }
        }
        VisibleRows::new(entries, slot.read_ignore_poison())
    }
}

impl Default for VisibleRowsCache {
    fn default() -> Self {
        Self::new()
    }
}

/// A row whose visibility can still flip while the listing sits unchanged.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    /// Its position in the listing's `entries`.
    entry_index: u32,
    /// How many settled rows come before it. Its row number, once you add the
    /// live candidates that also come before it.
    rows_before: u32,
}

/// Row numbers for one `(listing, include_hidden)` pair.
struct VisibleMap {
    /// Entry indices of the rows nothing can hide any more, in listing order.
    settled: Vec<u32>,
    /// The scratch-named entries, in listing order. Empty in a real directory.
    candidates: Vec<Candidate>,
}

impl VisibleMap {
    fn build(entries: &[FileEntry], include_hidden: bool) -> Self {
        let mut settled: Vec<u32> = Vec::with_capacity(entries.len());
        let mut candidates: Vec<Candidate> = Vec::new();

        for (index, entry) in entries.iter().enumerate() {
            #[cfg(test)]
            scan_probe::record();
            if !include_hidden && entry.name.starts_with('.') {
                continue;
            }
            if staging::could_be_hidden_from_listings(&entry.name) {
                candidates.push(Candidate {
                    entry_index: index as u32,
                    rows_before: settled.len() as u32,
                });
            } else {
                settled.push(index as u32);
            }
        }

        settled.shrink_to_fit();
        Self { settled, candidates }
    }
}

/// One reader's view of a listing's rows: the settled map, plus the candidates
/// that are visible at this instant.
///
/// Holds the map's read lock, so keep it only as long as the read takes. Every
/// answer it gives comes from one snapshot of the live scratch state, which is
/// why a count and a row read through the same value can never disagree.
pub(crate) struct VisibleRows<'a> {
    entries: &'a [FileEntry],
    map: RwLockReadGuard<'a, Option<VisibleMap>>,
    /// The candidates a reader can see right now, in listing order.
    live: Vec<Candidate>,
}

impl<'a> VisibleRows<'a> {
    fn new(entries: &'a [FileEntry], map: RwLockReadGuard<'a, Option<VisibleMap>>) -> Self {
        let live = map
            .as_ref()
            .expect("the map is filled before the read guard is taken")
            .candidates
            .iter()
            .copied()
            .filter(|candidate| {
                entries
                    .get(candidate.entry_index as usize)
                    .is_some_and(|entry| !staging::is_hidden_from_listings(&entry.name))
            })
            .collect();
        Self { entries, map, live }
    }

    fn map(&self) -> &VisibleMap {
        self.map
            .as_ref()
            .expect("the map is filled before the read guard is taken")
    }

    /// How many rows the pane is showing.
    pub(crate) fn len(&self) -> usize {
        self.map().settled.len() + self.live.len()
    }

    /// How many live candidates sit at a row number below `row`.
    ///
    /// The `k`-th live candidate's row number is `rows_before + k` (the settled
    /// rows ahead of it, plus the candidates ahead of it), which increases with
    /// `k`, so this binary-searches. The list is empty in a real directory, so
    /// this is normally a single length check.
    fn live_before(&self, row: usize) -> usize {
        let (mut low, mut high) = (0usize, self.live.len());
        while low < high {
            let mid = low + (high - low) / 2;
            if self.live[mid].rows_before as usize + mid < row {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low
    }

    /// The entry shown at `row`, or `None` when the pane isn't showing that many.
    ///
    /// `None` is a legitimate answer, not a bug: the frontend iterates over a row
    /// count it cached before a `directory-diff` shrank the listing, and asks for
    /// rows that briefly no longer exist.
    pub(crate) fn get(&self, row: usize) -> Option<&'a FileEntry> {
        let ahead = self.live_before(row);
        if let Some(candidate) = self.live.get(ahead)
            && candidate.rows_before as usize + ahead == row
        {
            return self.entries.get(candidate.entry_index as usize);
        }
        let entry_index = *self.map().settled.get(row.checked_sub(ahead)?)?;
        self.entries.get(entry_index as usize)
    }

    /// Every shown entry, in row order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &'a FileEntry> + '_ {
        (0..self.len()).map(|row| self.get(row).expect("row is within len"))
    }

    /// The row number of the entry named `name`, or `None` when the pane isn't
    /// showing it. A directory has no duplicate names, so this is unambiguous.
    pub(crate) fn row_of(&self, name: &str) -> Option<usize> {
        self.iter().position(|entry| entry.name == name)
    }
}

/// Counts entries examined by the visibility predicate on THIS thread, so a test
/// can pin how many times an accessor walks the listing.
///
/// The wedge this guards against is a scan count, not a duration: a per-row
/// accessor that re-walks the whole listing is invisible in a unit test's wall
/// clock on a small fixture and stops the app answering IPC on a real one.
/// Counting is the only measurement that survives both.
///
/// Thread-local, because `cargo test` runs the crate's tests as threads in one
/// process and a process-wide counter would mix every concurrent listing test's
/// walking into the reading.
#[cfg(test)]
pub(crate) mod scan_probe {
    use std::cell::Cell;

    thread_local! {
        static EXAMINED: Cell<u64> = const { Cell::new(0) };
    }

    /// One entry passed through the visibility predicate.
    pub(crate) fn record() {
        EXAMINED.with(|c| c.set(c.get() + 1));
    }

    /// Entries this thread has examined so far. Take a `before` and an `after`
    /// and subtract.
    pub(crate) fn examined() -> u64 {
        EXAMINED.with(Cell::get)
    }
}
