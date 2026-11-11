//! Fakes for the seams a test reads back, so a test can build an engine of its
//! own and assert on what the engine reported. `testing::TestOperations` is an
//! engine over one of each.
//!
//! - [`RecordingEngineEvents`]: every engine-wide fact, in order.
//! - [`FakeVolumes`]: a registry the test fills, answering the engine's volume
//!   questions from it alone.
//! - [`RecordingOwnWrites`]: every path the engine announced before writing it.
//! - [`SettablePacing`]: a rollback pace the test sets on its engine alone.
//! - [`pane_order_host`]: a volume host for a backend a test builds, with no
//!   panes but the panes' name order.
//! - [`PaneListings`]: panes a test opens, over a listing engine of its own,
//!   for the cells that seed the fresh-listing oracle.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cmdr_collation::NameCollator;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::SupportedDates;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{DirectoryChange, ResolvedVolume, RoutedKind, Volume};
use cmdr_listing::testing::TestEngine;
use cmdr_listing::{DirectorySortMode, EffectiveSort, SortColumn, SortOrder, sort_entries};
use cmdr_operation_log::rollback::VolumeLookup;

use super::super::types::{LeftoversKeptEvent, OperationsChanged, WriteProgressEvent};
use super::{
    EngineEvents, InstantMutationFact, OperationErrorReport, OperationVolumes, OwnWrites, Pacing, UnregisteredVolume,
    VolumeAnswer,
};
use cmdr_operation_log::rollback::RollbackReport;
use cmdr_operation_log::types::Initiator;

/// Records every engine-wide fact it hears.
// DEFAULT-OK: a recorder that has heard nothing yet is exactly empty lists.
#[derive(Default)]
#[allow(
    missing_docs,
    reason = "a test recorder: each field holds the facts of the `EngineEvents` method it's named after, in order"
)]
pub struct RecordingEngineEvents {
    pub operations_changed: Mutex<Vec<OperationsChanged>>,
    pub volumes_busy_changed: Mutex<Vec<Vec<String>>>,
    pub transfers_started: Mutex<Vec<Vec<String>>>,
    pub transfers_finished: Mutex<Vec<Vec<String>>>,
    pub leftovers_kept: Mutex<Vec<LeftoversKeptEvent>>,
    pub instant_mutations: Mutex<Vec<InstantMutationFact>>,
    /// `(initiator, narrowed, report)` per reversal that ran.
    pub rollbacks: Mutex<Vec<(Initiator, bool, RollbackReport)>>,
    pub error_reports: Mutex<Vec<OperationErrorReport>>,
    pub scan_wait_progress: Mutex<Vec<WriteProgressEvent>>,
}

impl EngineEvents for RecordingEngineEvents {
    fn operations_changed(&self, snapshot: OperationsChanged) {
        self.operations_changed.lock_ignore_poison().push(snapshot);
    }
    fn volumes_busy_changed(&self, volume_ids: Vec<String>) {
        self.volumes_busy_changed.lock_ignore_poison().push(volume_ids);
    }
    fn transfer_started(&self, volume_ids: &[String]) {
        self.transfers_started.lock_ignore_poison().push(volume_ids.to_vec());
    }
    fn transfer_finished(&self, volume_ids: &[String]) {
        self.transfers_finished.lock_ignore_poison().push(volume_ids.to_vec());
    }
    fn leftovers_kept(&self, notice: LeftoversKeptEvent) {
        self.leftovers_kept.lock_ignore_poison().push(notice);
    }
    fn instant_mutation_settled(&self, fact: InstantMutationFact) {
        self.instant_mutations.lock_ignore_poison().push(fact);
    }
    fn rollback_settled(&self, initiator: Initiator, narrowed: bool, report: &RollbackReport) {
        self.rollbacks
            .lock_ignore_poison()
            .push((initiator, narrowed, report.clone()));
    }
    fn error_report(&self, report: OperationErrorReport) {
        self.error_reports.lock_ignore_poison().push(report);
    }
    fn scan_wait_progress(&self, event: WriteProgressEvent) {
        self.scan_wait_progress.lock_ignore_poison().push(event);
    }
}

/// What [`FakeVolumes`] calls with each id it registers.
type ArrivalListener = Arc<dyn Fn(&str) + Send + Sync>;

/// A volume registry the test fills: nothing is an archive, nothing routes but
/// what [`FakeVolumes::route`] adds, and an id it doesn't hold is gone. A
/// registration, and a registered volume's reconnect, announce the id to every
/// arrival listener, inside the call, as the app's registry does.
// DEFAULT-OK: a registry the test hasn't filled holds no volume, which is what it says.
#[derive(Default)]
pub struct FakeVolumes {
    volumes: Mutex<HashMap<String, Arc<dyn Volume>>>,
    arrival_listeners: Mutex<Vec<ArrivalListener>>,
    routes: Mutex<Vec<FakeRoute>>,
    /// Set by [`Self::without_local_disk`]: the browser demo's shape, where no id
    /// is the local disk. `false` (the desktop's `"root"`) by default.
    no_local_disk: AtomicBool,
}

/// A path prefix [`FakeVolumes`] serves through a route rather than its drive.
struct FakeRoute {
    under: PathBuf,
    kind: RoutedKind,
    volume: Arc<dyn Volume>,
}

impl FakeVolumes {
    /// From now on, no volume id is the local disk (the browser demo's host),
    /// so `"root"` is a volume like any other.
    pub fn without_local_disk(&self) {
        self.no_local_disk.store(true, Ordering::Relaxed);
    }

    /// Serves every path strictly under `under` (on any volume id) through
    /// `volume` as a `kind` route, the way the app routes a `.zip`'s inside or a
    /// repo's `.git/<category>/` trees.
    pub fn route(&self, under: impl Into<PathBuf>, kind: RoutedKind, volume: Arc<dyn Volume>) {
        self.routes.lock_ignore_poison().push(FakeRoute {
            under: under.into(),
            kind,
            volume,
        });
    }

    fn route_for(&self, path: &Path) -> Option<(RoutedKind, Arc<dyn Volume>)> {
        self.routes
            .lock_ignore_poison()
            .iter()
            .find(|route| path != route.under && path.starts_with(&route.under))
            .map(|route| (route.kind, Arc::clone(&route.volume)))
    }

    /// Registers `volume` under `volume_id`, then tells every arrival listener.
    pub fn register(&self, volume_id: &str, volume: Arc<dyn Volume>) {
        self.volumes.lock_ignore_poison().insert(volume_id.to_string(), volume);
        self.announce(volume_id);
    }

    /// The volume registered under `volume_id` reconnected in place: tells
    /// every arrival listener, as the app's registry does on a `Connected`
    /// transition. Silent for an id it doesn't hold.
    pub fn reconnected(&self, volume_id: &str) {
        if self.get(volume_id).is_some() {
            self.announce(volume_id);
        }
    }

    fn announce(&self, volume_id: &str) {
        // Outside both locks, so a listener may ask this registry back.
        let listeners = self.arrival_listeners.lock_ignore_poison().clone();
        for listener in listeners {
            listener(volume_id);
        }
    }
}

impl VolumeLookup for FakeVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        self.volumes.lock_ignore_poison().get(volume_id).cloned()
    }
}

impl OperationVolumes for FakeVolumes {
    fn resolve<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, ResolvedVolume> {
        let resolved = match self.route_for(path) {
            Some((kind, volume)) => ResolvedVolume {
                volume: Some(volume),
                path: path.to_path_buf(),
                routed: Some(kind),
                unconfirmed: None,
            },
            None => ResolvedVolume::passthrough(self.get(volume_id), path),
        };
        Box::pin(async move { resolved })
    }
    fn routes_over_its_parent(&self, path: &Path) -> bool {
        self.route_for(path).is_some()
    }
    fn path_is_inside_archive<'a>(&'a self, _volume_id: &'a str, _path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(async { false })
    }
    fn path_crosses_archive_boundary<'a>(&'a self, _volume_id: &'a str, _path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(async { false })
    }
    fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        self.volumes
            .lock_ignore_poison()
            .iter()
            .find(|(_, volume)| volume.root() == root)
            .map(|(id, volume)| (id.clone(), Arc::clone(volume)))
    }
    fn forget_archive_index(&self, _archive_path: &Path) {}
    fn on_volume_arrival(&self, listener: Box<dyn Fn(&str) + Send + Sync>) {
        self.arrival_listeners.lock_ignore_poison().push(Arc::from(listener));
    }
    fn unregistered<'a>(&'a self, _volume_id: &'a str) -> VolumeAnswer<'a, UnregisteredVolume> {
        Box::pin(async { UnregisteredVolume::Gone })
    }

    fn local_disk_id(&self) -> Option<&str> {
        (!self.no_local_disk.load(Ordering::Relaxed)).then_some(cmdr_fs::volume::DEFAULT_VOLUME_ID)
    }
}

/// Records every path the engine announced, in order, with whether anything
/// (file, folder, or symlink) sat there at that moment: an announce that came
/// after its syscall shows as a creation that already existed.
// DEFAULT-OK: a recorder that has heard no announce yet is exactly an empty list.
#[derive(Default)]
pub struct RecordingOwnWrites {
    /// Each announced path, and whether anything sat there when it was announced.
    pub announced: Mutex<Vec<(PathBuf, bool)>>,
}

impl RecordingOwnWrites {
    /// Every announce under `root`, its path relative to `root`, in call order.
    pub fn announced_under(&self, root: &Path) -> Vec<(PathBuf, bool)> {
        self.announced
            .lock_ignore_poison()
            .iter()
            .filter_map(|(path, existed)| Some((path.strip_prefix(root).ok()?.to_path_buf(), *existed)))
            .collect()
    }
}

impl OwnWrites for RecordingOwnWrites {
    fn announce(&self, path: &Path) {
        let existed = path.symlink_metadata().is_ok();
        self.announced.lock_ignore_poison().push((path.to_path_buf(), existed));
    }
}

/// Paces each rollback item by what the test set, and nothing else: one
/// engine's pace, where the app's (`test_mode`'s throttle) is the process's.
// DEFAULT-OK: a pace nobody set is no pause, which is every real build's answer.
#[derive(Default)]
pub struct SettablePacing {
    rollback_millis: AtomicU64,
}

impl SettablePacing {
    /// Paces each rollback item by `pace` from now on; `None` stops pacing.
    pub fn set_rollback_pace(&self, pace: Option<Duration>) {
        let millis = pace.map_or(0, |pace| u64::try_from(pace.as_millis()).unwrap_or(u64::MAX));
        self.rollback_millis.store(millis, Ordering::SeqCst);
    }
}

impl Pacing for SettablePacing {
    fn copy_throttle(&self) -> Option<Duration> {
        None
    }

    fn copy_throttle_for(&self, _source: &Path) -> Option<Duration> {
        None
    }

    fn scan_preview_delay(&self) -> Option<Duration> {
        None
    }

    fn rollback_pace(&self) -> Option<Duration> {
        match self.rollback_millis.load(Ordering::SeqCst) {
            0 => None,
            millis => Some(Duration::from_millis(millis)),
        }
    }
}

/// A volume host for a backend a test builds (a `LocalPosixVolume`, an archive
/// volume): no panes, so every change goes nowhere and every oracle question
/// misses, but a local listing comes back in the panes' name order, as under
/// the app's host. The engine's walkers take a listing's order as is, so a
/// scenario keeps running in the order it was written against.
pub fn pane_order_host() -> VolumeHost {
    VolumeHost::builder()
        .listings(Arc::new(PaneOrderListings::new()))
        .build()
}

/// [`pane_order_host`]'s listings, for a walk a test hands listings directly
/// (`WalkContext::listings`).
pub static PANE_ORDER_LISTINGS: PaneOrderListings = PaneOrderListings::new();

/// [`pane_order_host`]'s listings: the app's `sort_listing` over English
/// collation (what `EnglishNameOrder` gives a host with no panes), and nothing
/// else.
pub struct PaneOrderListings {
    collator: OnceLock<Arc<NameCollator>>,
}

impl PaneOrderListings {
    /// No panes; the collator is built on first use.
    const fn new() -> Self {
        Self {
            collator: OnceLock::new(),
        }
    }
}

impl ListingHost for PaneOrderListings {
    fn directory_changed(&self, _volume_id: &str, _parent_path: &Path, _change: DirectoryChange) {}

    fn authoritative_listing(&self, _volume_id: &str, _path: &Path) -> Option<Vec<FileEntry>> {
        None
    }

    fn volumes_with_open_listings(&self, _volume_id_prefix: &str) -> Vec<String> {
        Vec::new()
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        _volume_id: &'a str,
        _archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    fn sort_listing(&self, entries: &mut [FileEntry]) {
        let collator = self.collator.get_or_init(|| Arc::new(NameCollator::new("en")));
        sort_entries(
            collator,
            entries,
            // A name sort reads no date, so no volume's dates change it.
            EffectiveSort::resolve(
                SortColumn::Name,
                SortOrder::Ascending,
                DirectorySortMode::LikeFiles,
                SupportedDates::NONE,
            ),
        );
    }
}

/// The panes a test shows, answered the way the app answers them
/// (`listing_host::backends::AppListings`): by a real listing engine, here one
/// of the test's own. The fresh-listing oracle is the engine's, so it answers
/// only for a listing the test inserted ([`Self::panes`] with
/// `TestListing::insert`) on a volume the test showed it ([`Self::show_volume`])
/// whose watch covers every writer.
///
/// What the app's adds and this leaves out: a change also reaching the drive
/// index and the cloud badges, and the FSEvents watch behind a local pane's
/// coverage (no test arms one, so the app answers `None` there too).
pub struct PaneListings {
    panes: TestEngine,
}

impl PaneListings {
    /// No pane open yet.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            panes: TestEngine::new(),
        })
    }

    /// The listing engine behind the panes: insert a pane's listing here.
    pub fn panes(&self) -> &TestEngine {
        &self.panes
    }

    /// Lets the panes ask `volume` how far its watch reaches, as the app's
    /// registry lets the app's listing engine: the oracle declines for a volume
    /// it can't ask.
    pub fn show_volume(&self, volume_id: &str, volume: Arc<dyn Volume>) {
        self.panes.volumes.register(volume_id, volume);
    }

    /// A volume host over these panes, for a backend or an engine a test builds.
    pub fn host(self: &Arc<Self>) -> VolumeHost {
        VolumeHost::builder()
            .listings(Arc::clone(self) as Arc<dyn ListingHost>)
            .build()
    }
}

impl ListingHost for PaneListings {
    fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange) {
        self.panes.directory_changed(volume_id, parent_path, change);
    }

    fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        self.panes.authoritative_listing(volume_id, path)
    }

    fn volumes_with_open_listings(&self, volume_id_prefix: &str) -> Vec<String> {
        self.panes.volume_ids_with_listings(volume_id_prefix)
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        volume_id: &'a str,
        archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move { self.panes.refresh_archive_listings(volume_id, archive_path).await })
    }

    fn sort_listing(&self, entries: &mut [FileEntry]) {
        sort_entries(
            &self.panes.collator(),
            entries,
            // A name sort reads no date, so no volume's dates change it.
            EffectiveSort::resolve(
                SortColumn::Name,
                SortOrder::Ascending,
                DirectorySortMode::LikeFiles,
                SupportedDates::NONE,
            ),
        );
    }
}
