// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These three can't go with them: `unused_crate_dependencies` is judged per
// compilation unit (as a package-wide flag every test target would report unused
// externs for deps only the lib uses), and `missing_docs` and `unreachable_pub`
// are this crate's own contract: its API is a deliverable rather than a side
// effect, and the capped surface (`DETAILS.md`) counts only what `lib.rs`
// re-exports, so a `pub` deep in a private module is a promise nobody can keep.
#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]
#![deny(missing_docs)]
// `clippy.toml` refuses an ambient spawn in the engine; a test drives the engine
// from threads and tasks of its own, which is exactly what a host does.
#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests drive the engine from their own threads and tasks, as a host would"
    )
)]

//! The directory listing engine: [`Listings`] and everything a host programs
//! against. The re-exports below ARE the API; every module but `host` (and the
//! `testing`-gated fixtures) is private, so nothing outside can reach past them.
//!
//! A host builds one engine with [`Listings::builder`], answering the engine's
//! questions through the seams in [`host`], and keeps it. See `CLAUDE.md` for the
//! must-knows and `DETAILS.md` for how a listing is read, kept, and published.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_listing` an extern crate of
// its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_listing as _;
//noinspection RsUnusedImport
// Test-only dependencies some partial test builds don't reach.
#[cfg(test)]
use {libc as _, tempfile as _, uuid as _};

mod authoritative;
mod cached_listing;
mod caching;
mod compare;
mod diff;
mod diff_emitter;
mod engine;
mod foreign_path;
mod fuzzy_jump;
pub mod host;
mod index_sizes;
mod listing_id;
mod mutate;
mod name_filter;
mod operations;
mod orphan_reaper;
mod path_index;
mod pipeline;
mod quick_filter;
mod refresh;
mod reread;
mod rt;
mod sorting;
mod stall;
mod stalled_on;
mod stats;
mod streaming;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod visible_rows;

pub use caching::ListingSummary;
pub use cmdr_collation::NameCollator;
pub use compare::{CompareDirectoriesError, CompareDirectoriesMode, CompareDirectoriesResult};
pub use diff::{DiffChange, DiffChangeType, DirectoryDiffBatch};
pub use engine::{Listings, ListingsBuilder, RowFacts};
pub use foreign_path::{entries_as_stored, stored_spellings};
pub use fuzzy_jump::FuzzyJumpError;
pub use index_sizes::IndexSizes;
pub use listing_id::ListingId;
pub use operations::{ListingLookupError, PaneMarks, ResortResult, RowBeside, RowSpan, SelectionSnapshot};
pub use quick_filter::{NameFilterResult, OnNoMatch};
pub use reread::{ReadTicket, RefreshOutcome};
// The sort vocabulary is `cmdr-sort`'s; re-exported so a host names a sort and
// the comparator that applies it from one place.
pub use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};
pub use sorting::{EffectiveSort, SortableEntry, entry_comparator, sort_entries};
pub use stalled_on::StalledOn;
pub use stats::ListingStats;
pub use streaming::ListingRequest;
pub use visible_rows::VisibleRows;

use crate::rt::Instant;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock, Weak};

use crate::rt::Runtime;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use cached_listing::{CachedListing, ListingPath};
use diff_emitter::DiffQueue;
use host::{
    ForegroundLeases, ListingEvents, ListingIndex, ListingObserver, ListingOverlay, ListingVolumes, MountTable,
    ReadingLocale, ScratchVisibility,
};
use refresh::RefreshSlot;
use stall::{HungReads, StallPolicy, StallWatch};
use streaming::StreamingListingState;

/// The engine's state: the seams it asks, and everything it keeps between two
/// calls. One per [`Listings`]; nothing here is process-wide.
///
/// Defined here at the module root, the parent of every module that implements
/// a part of it (`impl Inner` in `caching.rs`, `mutate.rs`, `refresh.rs`, `streaming.rs`, …), so the
/// engine's modules hang off one hub instead of circling through `engine/`.
pub(crate) struct Inner {
    /// Where the engine reports what happened to a listing.
    events: Arc<dyn ListingEvents>,
    /// Which volume serves a path.
    volumes: Arc<dyn ListingVolumes>,
    /// What the drive index knows.
    index: Arc<dyn ListingIndex>,
    /// Who hands out foreground holds.
    leases: Arc<dyn ForegroundLeases>,
    /// Which language names collate in.
    locale: Arc<dyn ReadingLocale>,
    /// What a filesystem path is mounted on.
    mounts: Arc<dyn MountTable>,
    /// The runtime every spawn goes to. ❌ Never `tokio::spawn` in the engine:
    /// it INHERITS an ambient runtime, and a watcher's OS thread has none, so it
    /// panics there.
    runtime: Runtime,
    /// Rows a pane sees that no volume holds, in the order the host added them.
    overlays: Vec<Arc<dyn ListingOverlay>>,
    /// Who hears every open and close, in the order the host added them.
    observers: Vec<Arc<dyn ListingObserver>>,
    /// The scratch files the user asked to see.
    scratch: ScratchSettings,
    /// Every open listing, by listing id.
    cache: RwLock<HashMap<String, CachedListing>>,
    /// What a listing's `last_accessed_ms` counts from.
    epoch: Instant,
    /// Every streaming read still running, by listing id: its cancel token and
    /// stall watch.
    streaming: RwLock<HashMap<String, Arc<StreamingListingState>>>,
    /// Committed diff batches waiting for their coalesced flush.
    diffs: Arc<DiffQueue>,
    /// Per-directory refresh turnstiles (`refresh.rs`).
    refresh_slots: RwLock<HashMap<(String, PathBuf), RefreshSlot>>,
    /// The last read generation handed out (`reread.rs`).
    read_generations: AtomicU64,
    /// Every volume's listing reads, and which of them are hung (`stall.rs`).
    hung_reads: Arc<HungReads>,
    /// One collator per locale tag (`Inner::collator`).
    collators: RwLock<HashMap<String, Arc<NameCollator>>>,
    /// What each open listing's volume keeps alive while it's shown
    /// (`Volume::watch_listing`), by listing id; dropped on close
    /// (`host/observers.rs`).
    volume_watches: std::sync::Mutex<HashMap<String, cmdr_fs::volume::ListingWatch>>,
}

impl Inner {
    /// Which scratch files the user asked to see, read once per operation and
    /// passed down as a value, ❗ before any cache lock is taken.
    pub(crate) fn scratch_visibility(&self) -> ScratchVisibility {
        self.scratch.get()
    }

    /// Milliseconds since this engine's epoch: what `last_accessed_ms` holds.
    pub(crate) fn epoch_millis_now(&self) -> u64 {
        rt::since(self.epoch).as_millis() as u64
    }

    /// `path` in the spelling `volume_id`'s volume keys listings under.
    ///
    /// ❗ Call it before taking the cache: it asks the host's volume seam.
    pub(crate) fn listing_path(&self, volume_id: &str, path: &std::path::Path) -> ListingPath {
        ListingPath::on_volume(self.volumes.as_ref(), volume_id, path)
    }

    /// Which language names collate in, as the host answers it now.
    fn locale(&self) -> String {
        self.locale.locale()
    }

    /// The collator for the locale the user reads now, built once per locale.
    ///
    /// A language switch is picked up here rather than pushed: the host's
    /// [`ReadingLocale`] answers the language the user reads now, so the next sort
    /// asks for a collator for the new language. Listings already sorted keep their
    /// order until something re-reads them, the same way they do when the sort
    /// column changes.
    ///
    /// Locales are a bounded set (14 catalogs ship), so the cache never grows past
    /// a handful of entries.
    ///
    /// ❗ Ask before taking the listing cache: it asks the host.
    pub(crate) fn collator(&self) -> Arc<NameCollator> {
        let locale = self.locale();

        if let Some(cached) = self.collators.read_ignore_poison().get(&locale) {
            return Arc::clone(cached);
        }

        let collator = Arc::new(NameCollator::new(&locale));
        self.collators
            .write_ignore_poison()
            .insert(locale, Arc::clone(&collator));
        collator
    }

    /// The stall policy production reads use, and this engine's registry of
    /// hung reads.
    fn stall_watch(&self) -> StallWatch {
        StallWatch {
            policy: StallPolicy::PRODUCTION,
            reads: Arc::clone(&self.hung_reads),
        }
    }

    /// The overlays, in registration order.
    fn overlays(&self) -> &[Arc<dyn ListingOverlay>] {
        &self.overlays
    }

    /// The observers, in registration order.
    fn observers(&self) -> &[Arc<dyn ListingObserver>] {
        &self.observers
    }

    /// A weak handle, for a background loop that must not keep the engine alive.
    fn downgrade(self: &Arc<Self>) -> Weak<Self> {
        Arc::downgrade(self)
    }
}

/// [`ScratchVisibility`] behind two atomics, so a read never takes a lock.
struct ScratchSettings {
    show_staging_temps: AtomicBool,
    show_safe_save_files: AtomicBool,
}

impl ScratchSettings {
    fn new(visibility: ScratchVisibility) -> Self {
        Self {
            show_staging_temps: AtomicBool::new(visibility.show_staging_temps),
            show_safe_save_files: AtomicBool::new(visibility.show_safe_save_files),
        }
    }

    fn get(&self) -> ScratchVisibility {
        ScratchVisibility {
            show_staging_temps: self.show_staging_temps.load(Ordering::Relaxed),
            show_safe_save_files: self.show_safe_save_files.load(Ordering::Relaxed),
        }
    }

    fn set(&self, visibility: ScratchVisibility) {
        self.show_staging_temps
            .store(visibility.show_staging_temps, Ordering::Relaxed);
        self.show_safe_save_files
            .store(visibility.show_safe_save_files, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod caching_oracle_test;
#[cfg(test)]
mod caching_reaper_test;
#[cfg(test)]
mod caching_replacement_test;
#[cfg(test)]
mod caching_test;
#[cfg(test)]
mod caching_update_test;
// Name order itself, split from `sorting_test` because it reads each test's
// own fixed language.
#[cfg(test)]
mod collation_test;
#[cfg(test)]
mod compare_test;
#[cfg(test)]
mod diff_emitter_test;
#[cfg(test)]
mod diff_test;
#[cfg(test)]
mod effective_sort_test;
#[cfg(test)]
mod engine_test;
#[cfg(test)]
mod find_file_index_test;
#[cfg(test)]
mod fuzzy_jump_test;
#[cfg(test)]
mod hidden_files_test;
#[cfg(test)]
mod index_sizes_test;
#[cfg(test)]
mod lifecycle_order_test;
#[cfg(test)]
mod name_filter_test;
#[cfg(test)]
mod operations_test;
#[cfg(test)]
mod overlay_read_paths_test;
#[cfg(test)]
mod packages_test;
#[cfg(test)]
mod pane_diff_test;
#[cfg(test)]
mod path_index_test;
#[cfg(test)]
mod probe_test_support;
#[cfg(test)]
mod read_generation_test;
#[cfg(test)]
mod row_beside_test;
#[cfg(test)]
mod selection_test;
#[cfg(test)]
mod sorting_dir_mode_test;
#[cfg(test)]
mod sorting_effective_test;
#[cfg(test)]
mod sorting_test;
#[cfg(test)]
mod sorting_test_support;
#[cfg(test)]
mod staging_temps_test;
#[cfg(test)]
mod stall_test;
#[cfg(test)]
mod stats_test;
#[cfg(test)]
mod streaming_events_test;
#[cfg(test)]
mod streaming_test;
#[cfg(test)]
mod visible_rows_test;
