//! The listing engine as one value: [`Listings`], the handle that owns every
//! listing, every in-flight read, every pending diff, and the seams it asks the
//! host through.
//!
//! The host builds one with [`Listings::builder`] and keeps it; a test or a tool
//! builds its own with [`Listings::detached`]. Two engines share nothing: a
//! listing one of them holds is invisible to the other.
//!
//! Every engine operation lives on the state, `Inner` (the module root), in the
//! module that owns that part of the engine (`caching.rs`, `mutate.rs`, `operations.rs`,
//! `streaming.rs`, …), as an `impl Inner` block. ❌ Engine code reaches its state
//! through `self`, never through the app's accessor: a method that asks for "the"
//! engine works in the app and quietly reads the wrong one everywhere else.

use crate::rt::Runtime;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use super::diff_emitter::DiffQueue;
use super::host::{
    ForegroundLeases, ListingEvents, ListingIndex, ListingObserver, ListingOverlay, ListingVolumes, MountTable,
    NoEvents, NoIndex, NoLeases, NoLocale, NoMountTable, NoVolumes, ReadingLocale, ScratchVisibility,
};
use super::stall::HungReads;
use super::{Inner, ScratchSettings};

mod housekeeping;
mod intake;
mod open;
mod oracle;
mod reread;
mod rows;
mod settings;

pub use intake::RowFacts;

/// The listing engine: every open listing, and the seams it asks the host
/// through. Cheap to clone; every clone is the same engine.
#[derive(Clone)]
pub struct Listings {
    pub(super) inner: Arc<Inner>,
}

impl Listings {
    /// Starts an engine that lists through `volumes` and reports to `events`, the
    /// two seams no real host can do without: with no volumes it lists nothing,
    /// and with no events every pane waits on "loading" forever. The other seams
    /// default to their no-op answers.
    pub fn builder(volumes: Arc<dyn ListingVolumes>, events: Arc<dyn ListingEvents>) -> ListingsBuilder {
        ListingsBuilder::new(volumes, events)
    }

    /// An engine that answers nothing: no volumes, events dropped, every other
    /// seam a no-op, the shared fallback runtime.
    pub fn detached() -> Self {
        Self::builder(Arc::new(NoVolumes), Arc::new(NoEvents)).build()
    }
}

/// Builds a [`Listings`]. Every seam it isn't handed answers with its no-op.
pub struct ListingsBuilder {
    events: Arc<dyn ListingEvents>,
    volumes: Arc<dyn ListingVolumes>,
    index: Arc<dyn ListingIndex>,
    leases: Arc<dyn ForegroundLeases>,
    locale: Arc<dyn ReadingLocale>,
    mounts: Arc<dyn MountTable>,
    runtime: Runtime,
    overlays: Vec<Arc<dyn ListingOverlay>>,
    observers: Vec<Arc<dyn ListingObserver>>,
}

impl ListingsBuilder {
    fn new(volumes: Arc<dyn ListingVolumes>, events: Arc<dyn ListingEvents>) -> Self {
        Self {
            events,
            volumes,
            index: Arc::new(NoIndex),
            leases: Arc::new(NoLeases),
            locale: Arc::new(NoLocale),
            mounts: Arc::new(NoMountTable),
            runtime: crate::rt::default_runtime(),
            overlays: Vec::new(),
            observers: Vec::new(),
        }
    }

    /// Asks `index` what the drive index knows about a listing's rows.
    pub fn index(mut self, index: Arc<dyn ListingIndex>) -> Self {
        self.index = index;
        self
    }

    /// Takes a foreground hold from `leases` for every streaming read.
    pub fn leases(mut self, leases: Arc<dyn ForegroundLeases>) -> Self {
        self.leases = leases;
        self
    }

    /// Collates names in the language `locale` answers.
    pub fn locale(mut self, locale: Arc<dyn ReadingLocale>) -> Self {
        self.locale = locale;
        self
    }

    /// Reads what a stalled folder is mounted on from `mounts`.
    pub fn mounts(mut self, mounts: Arc<dyn MountTable>) -> Self {
        self.mounts = mounts;
        self
    }

    /// Folds `overlay`'s rows into the listings it applies to, after every
    /// overlay added before it. The first overlay per id wins.
    pub fn overlay(mut self, overlay: Arc<dyn ListingOverlay>) -> Self {
        add_overlay(&mut self.overlays, overlay);
        self
    }

    /// Tells `observer` about every listing open and close, after every
    /// observer added before it. The first observer per id wins.
    pub fn observer(mut self, observer: Arc<dyn ListingObserver>) -> Self {
        add_observer(&mut self.observers, observer);
        self
    }

    /// Spawns the engine's background work onto `handle`, so the host keeps ONE
    /// thread pool. Native only: the browser build always runs on the page's
    /// event loop (`src/rt/`).
    #[cfg(not(target_family = "wasm"))]
    pub fn runtime(mut self, handle: tokio::runtime::Handle) -> Self {
        self.runtime = handle;
        self
    }

    /// The engine, with no listing open.
    pub fn build(self) -> Listings {
        let diffs = Arc::new(DiffQueue::new(Arc::clone(&self.events), self.runtime.clone()));
        Listings {
            inner: Arc::new(Inner {
                events: self.events,
                volumes: self.volumes,
                index: self.index,
                leases: self.leases,
                locale: self.locale,
                mounts: self.mounts,
                runtime: self.runtime,
                overlays: self.overlays,
                observers: self.observers,
                scratch: ScratchSettings::new(ScratchVisibility::default()),
                cache: RwLock::new(HashMap::new()),
                epoch: crate::rt::now(),
                streaming: RwLock::new(HashMap::new()),
                diffs,
                refresh_slots: RwLock::new(HashMap::new()),
                read_generations: AtomicU64::new(0),
                hung_reads: Arc::new(HungReads::new()),
                collators: RwLock::new(HashMap::new()),
                volume_watches: std::sync::Mutex::new(HashMap::new()),
            }),
        }
    }
}

/// The first overlay per id wins; a duplicate is logged and dropped, so a
/// double registration can't contribute the same rows twice.
fn add_overlay(overlays: &mut Vec<Arc<dyn ListingOverlay>>, overlay: Arc<dyn ListingOverlay>) {
    if overlays.iter().any(|o| o.id() == overlay.id()) {
        log::warn!(target: "listing", "listing overlay {} registered twice; keeping the first", overlay.id());
        return;
    }
    log::debug!(target: "listing", "registered listing overlay {}", overlay.id());
    overlays.push(overlay);
}

/// The first observer per id wins, so a double registration can't arm twice.
fn add_observer(observers: &mut Vec<Arc<dyn ListingObserver>>, observer: Arc<dyn ListingObserver>) {
    if observers.iter().any(|o| o.id() == observer.id()) {
        return;
    }
    log::debug!(target: "listing", "registered listing observer {}", observer.id());
    observers.push(observer);
}
