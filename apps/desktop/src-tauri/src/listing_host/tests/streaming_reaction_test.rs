//! The terminal-reaction matrix of one streaming listing: for every way a pane's
//! read can end, which events it emits in which order, and which app bookkeeping
//! runs around it (the MCP recent-errors ring, the analytics category, the
//! TCC-restricted set's record and clear, and the stale-mount check).
//!
//! Each cell builds an engine of its own that reports to the app's real
//! `TauriListingEvents` policy minus the `AppHandle`, with the wire events
//! collected and a recording [`ListingEndEffects`], and drives exactly what a
//! `start` read's task does (`read_as_task`). These cells are the proof that the
//! policy behind the `ListingEvents` seam kept every reaction.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::listing_host::events::{ListingEndEffects, TauriListingEvents, WireCollector, error_analytics_category};
use crate::test_support::{TestDir, wait_until_async};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::unique_test_id;
use cmdr_fs::volume::friendly_error::{ListingError, ListingErrorReason, Provider};
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};
use cmdr_listing::host::{ListingEnd, ListingEvents, ListingOverlay};
use cmdr_listing::testing::{SpelledVolume, StreamingListingState, TestEngine, TestListingGuard, request};
use cmdr_listing::{DirectoryDiffBatch, ListingId, StalledOn};

/// One bookkeeping call, as the app's reaction made it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    /// The MCP ring got an entry, with this typed reason.
    RecentError(String),
    RecordDenial(PathBuf),
    ClearRestricted(PathBuf),
    NoteRootFailure(String),
}

#[derive(Default)]
struct RecordingEffects {
    calls: std::sync::Mutex<Vec<Effect>>,
}

impl RecordingEffects {
    fn calls(&self) -> Vec<Effect> {
        self.calls.lock_ignore_poison().clone()
    }
}

impl ListingEndEffects for Arc<RecordingEffects> {
    fn record_recent_error(
        &self,
        _listing_id: &str,
        _volume_id: &str,
        _path: &Path,
        _message: &str,
        error: &ListingError,
    ) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::RecentError(format!("{:?}", error.reason)));
    }

    fn record_denial(&self, path: &Path) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::RecordDenial(path.to_path_buf()));
    }

    fn clear_restricted(&self, path: &Path) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::ClearRestricted(path.to_path_buf()));
    }

    fn note_root_failure(&self, volume_id: &str, _error: &VolumeError) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::NoteRootFailure(volume_id.to_string()));
    }
}

/// What one listing did, end to end.
struct Ended {
    events: Vec<&'static str>,
    effects: Vec<Effect>,
    /// The typed classification of each `listing-error`, in order.
    errors: Vec<Option<ListingError>>,
    /// The analytics category each `listing-error` reports.
    analytics: Vec<Option<String>>,
    listing: TestListingGuard,
}

/// One cell's engine, reporting through the app's reaction policy over a
/// collected wire and recorded bookkeeping.
struct App {
    engine: TestEngine,
    wire: Arc<WireCollector>,
    effects: Arc<RecordingEffects>,
}

impl App {
    /// An engine serving `volume`.
    fn serving(volume: &Registered) -> Self {
        Self::build(volume, None, |reaction| reaction)
    }

    /// An engine serving `volume`, with `overlay` folded into its reads and
    /// `wrap` in front of the app's reaction.
    fn build(
        volume: &Registered,
        overlay: Option<Arc<dyn ListingOverlay>>,
        wrap: impl FnOnce(Arc<dyn ListingEvents>) -> Arc<dyn ListingEvents>,
    ) -> Self {
        let wire = Arc::new(WireCollector::default());
        let effects = Arc::new(RecordingEffects::default());
        let reaction: Arc<dyn ListingEvents> =
            Arc::new(TauriListingEvents::with(Arc::clone(&wire), Arc::clone(&effects)));
        let mut builder = TestEngine::builder()
            .volume(&volume.0, Arc::clone(&volume.1))
            .events(wrap(reaction));
        if let Some(overlay) = overlay {
            builder = builder.overlay(overlay);
        }
        Self {
            engine: builder.build(),
            wire,
            effects,
        }
    }

    /// A fresh read's state on this engine.
    fn state(&self) -> Arc<StreamingListingState> {
        self.engine.streaming_state()
    }

    /// Runs one streaming listing of `path` on `volume_id` through the spawned
    /// task's two steps, given the read's state.
    async fn run(&self, volume_id: &str, path: &Path, state: Arc<StreamingListingState>) -> Ended {
        let listing = TestListingGuard::adopt(&self.engine, unique_test_id("reaction"));
        self.engine
            .read_as_task(&state, &request(listing.id(), volume_id, path))
            .await;
        let errors: Vec<Option<ListingError>> = self
            .wire
            .classified
            .lock_ignore_poison()
            .iter()
            .map(|(_, e)| e.clone())
            .collect();
        let analytics = errors.iter().map(|e| error_analytics_category(e.as_ref())).collect();
        Ended {
            events: self.wire.sequence(),
            effects: self.effects.calls(),
            errors,
            analytics,
            listing,
        }
    }
}

/// Runs one listing of `path` on `volume` through a fresh app engine, given a
/// way to set up the read's state.
async fn run_listing(volume: &Registered, path: &Path, setup: impl FnOnce(&StreamingListingState)) -> Ended {
    let app = App::serving(volume);
    let state = app.state();
    setup(&state);
    app.run(&volume.0, path, state).await
}

/// A read with nothing done to its state first.
fn as_is(_: &StreamingListingState) {}

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/{name}"), false, false)
}

/// A volume under a fresh id, for a cell's engine to serve.
struct Registered(String, Arc<dyn Volume>);

impl Registered {
    fn new(tag: &str, volume: Arc<dyn Volume>) -> Self {
        Self(unique_test_id(tag), volume)
    }
}

fn refused_with(tag: &str, error: VolumeError) -> Registered {
    Registered::new(
        tag,
        Arc::new(SpelledVolume::new(InMemoryVolume::new("Refusing")).listings_refused_with(error)),
    )
}

/// A path inside a Dropbox folder, which provider detection reads lexically.
fn cloud_path(name: &str) -> PathBuf {
    dirs::home_dir()
        .expect("a test runner has a home directory")
        .join("Library/CloudStorage/Dropbox-seam-pins")
        .join(name)
}

fn reason_of(ended: &Ended) -> String {
    let [Some(error)] = ended.errors.as_slice() else {
        panic!("expected one classified error, got {:?}", ended.errors);
    };
    format!("{:?}", error.reason)
}

// ============================================================================
// Success
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_that_lands_completes_and_clears_its_path_from_the_restricted_set() {
    let volume = Registered::new(
        "reaction-ok",
        Arc::new(InMemoryVolume::with_entries("Ok", vec![entry("a.txt")])),
    );
    let ended = run_listing(&volume, Path::new("/"), as_is).await;

    assert_eq!(ended.events, ["opening", "read_complete", "complete"]);
    assert_eq!(ended.effects, [Effect::ClearRestricted(PathBuf::from("/"))]);
    assert!(ended.analytics.is_empty(), "no error, no analytics");
    assert!(ended.listing.is_cached());
}

// ============================================================================
// Cancel, at each of its four points
// ============================================================================

/// Every cancel ends `Ok`, so the task clears the path from the restricted set
/// exactly as a success does, and nothing reaches the error bookkeeping.
fn assert_cancelled(ended: &Ended, events: &[&str], path: &Path) {
    assert_eq!(ended.events, events);
    assert_eq!(ended.effects, [Effect::ClearRestricted(path.to_path_buf())]);
    assert!(ended.analytics.is_empty());
    assert!(!ended.listing.is_cached(), "a cancelled read never lands in the cache");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_before_the_read_ends_the_listing_unread() {
    let volume = Registered::new(
        "reaction-cancel-before",
        Arc::new(InMemoryVolume::with_entries("Before", vec![entry("a.txt")])),
    );
    let ended = run_listing(&volume, Path::new("/"), |state| state.cancel.cancel()).await;

    assert_cancelled(&ended, &["opening", "cancelled"], Path::new("/"));
}

/// A volume whose read waits until the listing is cancelled, and says when it
/// started.
struct WaitsForCancel {
    root: PathBuf,
    started: Arc<AtomicBool>,
}

impl Volume for WaitsForCancel {
    fn name(&self) -> &str {
        "Waits for cancel"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let cancel = cancel.cloned().expect("a pane listing hands its read the cancel token");
        let started = Arc::clone(&self.started);
        Box::pin(async move {
            started.store(true, Ordering::SeqCst);
            // Bounded, so a lost cancel fails the test instead of hanging the suite.
            let _ = tokio::time::timeout(Duration::from_secs(30), cancel.cancelled()).await;
            Ok(Vec::new())
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(true) })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_read_ends_the_listing_without_waiting_for_it() {
    let started = Arc::new(AtomicBool::new(false));
    let volume = Registered::new(
        "reaction-cancel-during",
        Arc::new(WaitsForCancel {
            root: PathBuf::from("/"),
            started: Arc::clone(&started),
        }),
    );
    let app = Arc::new(App::serving(&volume));
    let state = app.state();

    let run = {
        let app = Arc::clone(&app);
        let volume_id = volume.0.clone();
        let state = Arc::clone(&state);
        tokio::spawn(async move { app.run(&volume_id, Path::new("/"), state).await })
    };
    wait_until_async(Duration::from_secs(10), "the read to start", || {
        started.load(Ordering::SeqCst)
    })
    .await;
    state.cancel.cancel();
    let ended = run.await.expect("the listing task doesn't panic");

    assert_cancelled(&ended, &["opening", "cancelled"], Path::new("/"));
}

/// Cancels the listing the moment the read reports complete: after the read,
/// before the after-read check. Delegates every report to the app's reaction.
struct CancelOnReadComplete {
    inner: Arc<dyn ListingEvents>,
    /// The read's token, set once the engine handed out the read's state.
    cancel: Arc<OnceLock<CancellationToken>>,
}

impl ListingEvents for CancelOnReadComplete {
    fn opening(&self, id: &ListingId) {
        self.inner.opening(id);
    }
    fn stalled(&self, id: &ListingId, on: StalledOn) {
        self.inner.stalled(id, on);
    }
    fn progress(&self, id: &ListingId, loaded: usize) {
        self.inner.progress(id, loaded);
    }
    fn read_complete(&self, id: &ListingId, total: usize) {
        self.inner.read_complete(id, total);
        if let Some(cancel) = self.cancel.get() {
            cancel.cancel();
        }
    }
    fn finished(&self, id: &ListingId, end: ListingEnd) {
        self.inner.finished(id, end);
    }
    fn diff(&self, id: &ListingId, batches: Vec<DirectoryDiffBatch>) {
        self.inner.diff(id, batches);
    }
    fn respelled(&self, id: &ListingId, path: &Path) {
        self.inner.respelled(id, path);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_after_the_read_ends_the_listing_before_it_is_cached() {
    let volume = Registered::new(
        "reaction-cancel-after",
        Arc::new(InMemoryVolume::with_entries("After", vec![entry("a.txt")])),
    );
    let cancel = Arc::new(OnceLock::new());
    let app = App::build(&volume, None, |reaction| {
        Arc::new(CancelOnReadComplete {
            inner: reaction,
            cancel: Arc::clone(&cancel),
        })
    });
    let state = app.state();
    let _ = cancel.set(state.cancel.clone());

    let ended = app.run(&volume.0, Path::new("/"), state).await;
    let (collector, effects, listing) = (&app.wire, &app.effects, &ended.listing);

    assert_eq!(collector.sequence(), ["opening", "read_complete", "cancelled"]);
    assert_eq!(effects.calls(), [Effect::ClearRestricted(PathBuf::from("/"))]);
    assert!(!listing.is_cached(), "a cancelled read never lands in the cache");
}

/// Cancels the read of the volume named `volume_name` the moment the pipeline
/// reaches the overlays: after the after-read cancel check, before the insert.
struct CancelAtDecorate {
    volume_name: String,
    cancel: Arc<OnceLock<CancellationToken>>,
}

impl ListingOverlay for CancelAtDecorate {
    fn id(&self) -> &'static str {
        "cancel-at-decorate"
    }

    fn applies_to(&self, volume: &dyn Volume, _path: &Path) -> bool {
        if volume.name() == self.volume_name
            && let Some(cancel) = self.cancel.get()
        {
            cancel.cancel();
        }
        false
    }

    fn extra_entries(&self, _volume: &dyn Volume, _path: &Path) -> Vec<FileEntry> {
        Vec::new()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_at_the_cache_insert_leaves_nothing_cached() {
    let volume_name = unique_test_id("reaction-cancel-insert-volume");
    let volume = Registered::new(
        "reaction-cancel-insert",
        Arc::new(InMemoryVolume::with_entries(volume_name.as_str(), vec![entry("a.txt")])),
    );
    // Fires at the overlay step: past the after-read check, before the insert.
    let cancel = Arc::new(OnceLock::new());
    let app = App::build(
        &volume,
        Some(Arc::new(CancelAtDecorate {
            volume_name: volume_name.clone(),
            cancel: Arc::clone(&cancel),
        })),
        |reaction| reaction,
    );
    let state = app.state();
    let _ = cancel.set(state.cancel.clone());

    let ended = app.run(&volume.0, Path::new("/"), state).await;

    assert_cancelled(&ended, &["opening", "read_complete", "cancelled"], Path::new("/"));
}

// ============================================================================
// Refusals and errors
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_permission_denied_listing_records_the_denial() {
    let path = Path::new("/denied");
    let volume = refused_with(
        "reaction-denied",
        VolumeError::PermissionDenied {
            path: "/denied".to_string(),
            raw_os_error: Some(libc::EACCES),
        },
    );

    let ended = run_listing(&volume, path, as_is).await;

    assert_eq!(ended.events, ["opening", "error"]);
    let reason = reason_of(&ended);
    assert_eq!(
        ended.effects,
        [
            Effect::RecordDenial(path.to_path_buf()),
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason),
        ]
    );
    assert_eq!(ended.analytics, [Some("needs_action".to_string())]);
}

/// Under a cloud provider's folder on purpose: a password prompt names no
/// provider, where a generic error there would.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_that_needs_a_password_asks_for_it() {
    let path = cloud_path("locked.zip");
    let volume = refused_with("reaction-password", VolumeError::NeedsPassword { wrong_attempt: false });

    let ended = run_listing(&volume, &path, as_is).await;

    assert_eq!(ended.events, ["opening", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                reason: ListingErrorReason::ArchiveNeedsPassword { wrong_attempt: false },
                provider: None,
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(
        ended.effects,
        [
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason_of(&ended)),
        ]
    );
    assert_eq!(ended.analytics, [Some("needs_action".to_string())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_damaged_archive_reads_as_unreadable() {
    let scratch = TestDir::new("reaction-damaged-archive");
    let archive = scratch.join("broken.zip");
    std::fs::write(&archive, b"not a zip at all").expect("scratch dir is writable");
    let volume = refused_with("reaction-damaged", VolumeError::NotSupported);

    let ended = run_listing(&volume, &archive, as_is).await;

    assert_eq!(ended.events, ["opening", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                reason: ListingErrorReason::ArchiveUnreadable,
                provider: None,
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(
        ended.effects,
        [
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason_of(&ended)),
        ]
    );
    assert_eq!(ended.analytics, [Some("serious".to_string())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_generic_error_names_the_cloud_provider_behind_the_path() {
    let path = cloud_path("folder");
    let volume = refused_with(
        "reaction-provider",
        VolumeError::IoError {
            message: "Operation timed out".into(),
            raw_os_error: Some(libc::ETIMEDOUT),
        },
    );

    let ended = run_listing(&volume, &path, as_is).await;

    assert_eq!(ended.events, ["opening", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                provider: Some(Provider::Dropbox),
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(
        ended.effects,
        [
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason_of(&ended)),
        ]
    );
    assert_eq!(ended.analytics, [Some("transient".to_string())]);
}

// ============================================================================
// The restricted-empty root
// ============================================================================

/// iCloud Drive without Full Disk Access lists as an empty root. The read lands
/// it as complete, and the app's reaction turns that into a `listing-error`,
/// records it in the MCP ring, and ALSO clears the path from the restricted set,
/// the same as a success.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_icloud_root_reads_as_restricted_yet_clears_the_path() {
    // The literal id `listing_error_for_restricted_empty_root` matches.
    let volume = Registered(
        "cloud-icloud".to_string(),
        Arc::new(InMemoryVolume::new("iCloud Drive")),
    );

    let ended = run_listing(&volume, Path::new("/"), as_is).await;

    assert_eq!(ended.events, ["opening", "read_complete", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                reason: ListingErrorReason::EmptyRootICloud,
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(
        ended.effects,
        [
            Effect::RecentError(reason_of(&ended)),
            Effect::ClearRestricted(PathBuf::from("/")),
        ],
        "the restricted root goes into the MCP ring once, before the path clears"
    );
    assert_eq!(ended.analytics, [Some("needs_action".to_string())]);
    assert!(ended.listing.is_cached(), "the empty listing still lands");
}
