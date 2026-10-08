//! A listing whose read stops answering: it reports itself stalled within the
//! deadline, recovers on its own when the volume answers again, stops retrying
//! when cancelled, and never piles up reads on a hung volume.
//!
//! The fake volume stands in for a kernel mount whose server went silent: every
//! read blocks until the test says the server is back, then answers from a script.

use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;

use crate::file_system::listing::caching_test_support::{TestListingGuard, unique_test_id};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::listing::stall::{HungReads, StallPolicy, StallWatch};
use crate::file_system::listing::stalled_on::StalledOn;
use crate::file_system::listing::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{ListingProgress, Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::wait_until_async;

/// Short enough to keep the suite fast, long enough that a healthy read never trips it.
const TEST_POLICY: StallPolicy = StallPolicy {
    stall_after: Duration::from_millis(150),
    first_backoff: Duration::from_millis(20),
    max_backoff: Duration::from_millis(80),
};

/// How long any expected event may take: many times the policy's deadlines, so a
/// timeout means the behavior is missing, never that the machine is busy.
const WITHIN: Duration = Duration::from_secs(5);

/// One scripted read: whether it blocks until the server is back, then what it answers.
#[derive(Clone)]
enum Answer {
    Entries(usize),
    /// `ENOTCONN`: what a silent smbfs mount answers once its server returns.
    NotConnected,
    /// `ETIMEDOUT`: a transient refusal.
    TimedOut,
    NotFound,
}

struct Step {
    hangs: bool,
    answer: Answer,
}

/// A volume whose reads block like a hung kernel mount until `server_back` flips.
struct SilentServerVolume {
    root: PathBuf,
    server_back: watch::Receiver<bool>,
    /// What each read does, in call order; the last step repeats.
    script: Mutex<VecDeque<Step>>,
    calls: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl SilentServerVolume {
    fn new(server_back: watch::Receiver<bool>, script: Vec<Step>) -> Self {
        Self {
            root: PathBuf::from("/"),
            server_back,
            script: Mutex::new(script.into()),
            calls: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
        }
    }

    fn next_step(&self) -> Step {
        let mut script = self.script.lock_ignore_poison();
        if script.len() > 1 {
            script.pop_front().expect("checked non-empty")
        } else {
            let last = script.front().expect("a script has at least one step");
            Step {
                hangs: last.hangs,
                answer: last.answer.clone(),
            }
        }
    }
}

/// Keeps the in-flight count honest however the read ends.
struct InFlight<'a>(&'a AtomicUsize);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn entry(i: usize) -> FileEntry {
    FileEntry::new(format!("file-{i}.txt"), format!("/file-{i}.txt"), false, false)
}

impl Volume for SilentServerVolume {
    fn name(&self) -> &str {
        "Silent server"
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
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            let _in_flight = InFlight(&self.in_flight);
            let step = self.next_step();
            if step.hangs {
                let mut back = self.server_back.clone();
                // Err only when the test dropped the sender: treat that as "never back".
                if back.wait_for(|back| *back).await.is_err() {
                    std::future::pending::<()>().await;
                }
            }
            match step.answer {
                Answer::Entries(n) => Ok((0..n).map(entry).collect()),
                Answer::NotConnected => Err(VolumeError::IoError {
                    message: "Socket is not connected".to_string(),
                    raw_os_error: Some(libc::ENOTCONN),
                }),
                Answer::TimedOut => Err(VolumeError::IoError {
                    message: "Operation timed out".to_string(),
                    raw_os_error: Some(libc::ETIMEDOUT),
                }),
                Answer::NotFound => Err(VolumeError::NotFound("/".to_string())),
            }
        })
    }

    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotFound("not scripted".to_string())) })
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

/// One volume, one sink, and the switch that brings its server back.
struct Rig {
    volume_id: String,
    volume: Arc<SilentServerVolume>,
    server: watch::Sender<bool>,
    sink: Arc<CollectorListingEventSink>,
    watch: StallWatch,
}

impl Rig {
    fn new(script: Vec<Step>) -> Self {
        Self::with_policy(script, TEST_POLICY)
    }

    fn with_policy(script: Vec<Step>, policy: StallPolicy) -> Self {
        let (server, server_back) = watch::channel(false);
        let volume = Arc::new(SilentServerVolume::new(server_back, script));
        let volume_id = format!("test-stall-{}", uuid::Uuid::new_v4());
        get_volume_manager().register(&volume_id, Arc::clone(&volume) as Arc<dyn Volume>);
        Self {
            volume_id,
            volume,
            server,
            sink: Arc::new(CollectorListingEventSink::new()),
            watch: StallWatch {
                policy,
                reads: Arc::new(HungReads::new()),
            },
        }
    }

    /// Starts a listing in its own task, the way `list_directory_start_streaming` does.
    fn start(
        &self,
        listing_id: &str,
    ) -> (
        Arc<StreamingListingState>,
        tokio::task::JoinHandle<Result<(), VolumeError>>,
    ) {
        let state = Arc::new(StreamingListingState::with_stall_watch(self.watch.clone()));
        let events: Arc<dyn ListingEventSink> = Arc::clone(&self.sink) as Arc<dyn ListingEventSink>;
        let task = {
            let state = Arc::clone(&state);
            let volume_id = self.volume_id.clone();
            let listing_id = listing_id.to_string();
            tokio::spawn(async move {
                read_directory_with_progress(
                    &events,
                    &listing_id,
                    &state,
                    &volume_id,
                    Path::new("/"),
                    true,
                    SortColumn::Name,
                    SortOrder::Ascending,
                    DirectorySortMode::LikeFiles,
                )
                .await
            })
        };
        (state, task)
    }

    async fn wait_for_stall(&self, listing_id: &str) {
        wait_until_async(WITHIN, "the listing to report itself stalled", || {
            self.sink
                .stalled
                .lock_ignore_poison()
                .iter()
                .any(|(id, _)| id == listing_id)
        })
        .await;
    }

    fn bring_server_back(&self) {
        self.server.send_replace(true);
    }

    fn completed(&self, listing_id: &str) -> Option<usize> {
        self.sink
            .complete
            .lock_ignore_poison()
            .iter()
            .find(|(id, _)| id == listing_id)
            .map(|(_, count)| *count)
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.volume_id);
    }
}

fn hang_then(answer: Answer) -> Step {
    Step { hangs: true, answer }
}

fn prompt(answer: Answer) -> Step {
    Step { hangs: false, answer }
}

/// The regression: a read that never answers used to leave the pane on a
/// spinner, and `cmdr://state` on an empty list, for as long as the kernel held it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_that_never_answers_reports_the_listing_stalled_within_the_deadline() {
    let rig = Rig::new(vec![hang_then(Answer::Entries(1))]);
    let listing = TestListingGuard::adopt(unique_test_id("stall-never"));

    let (state, task) = rig.start(listing.id());
    rig.wait_for_stall(listing.id()).await;

    assert!(rig.sink.complete.lock_ignore_poison().is_empty());
    assert!(rig.sink.errors.lock_ignore_poison().is_empty());
    // The event names what the folder lives on: this volume is a plain filesystem
    // backend reading `/`, which is a local disk on every machine the tests run on.
    assert_eq!(
        rig.sink.stalled.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), StalledOn::Drive)]
    );

    state.cancel.cancel();
    let result = task.await.expect("listing task must not panic");
    assert!(result.is_ok(), "a cancel is not a refusal, got {result:?}");
    assert_eq!(
        rig.sink.cancelled.lock_ignore_poison().as_slice(),
        [listing.id().to_string()]
    );
}

/// The server comes back and the stuck read answers with the stale refusal a
/// silent smbfs mount gives (`ENOTCONN`). The listing asks again on its own and
/// lands, under the same listing id, with no error shown in between.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stalled_listing_recovers_on_its_own_when_the_server_comes_back() {
    let rig = Rig::new(vec![hang_then(Answer::NotConnected), hang_then(Answer::Entries(2))]);
    let listing = TestListingGuard::adopt(unique_test_id("stall-recover"));

    let (_state, task) = rig.start(listing.id());
    rig.wait_for_stall(listing.id()).await;
    rig.bring_server_back();

    let result = task.await.expect("listing task must not panic");
    assert!(result.is_ok(), "got {result:?}");
    assert_eq!(rig.completed(listing.id()), Some(2));
    assert!(
        rig.sink.errors.lock_ignore_poison().is_empty(),
        "a stuck read's refusal is stale: it must not reach the pane"
    );
}

/// After a stall, a prompt transient refusal (the server answering "timed out")
/// is retried with backoff until the folder lists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stalled_listing_retries_prompt_transient_refusals_until_it_lands() {
    let rig = Rig::new(vec![
        hang_then(Answer::TimedOut),
        prompt(Answer::TimedOut),
        prompt(Answer::TimedOut),
        prompt(Answer::Entries(3)),
    ]);
    let listing = TestListingGuard::adopt(unique_test_id("stall-transient"));

    let (_state, task) = rig.start(listing.id());
    rig.wait_for_stall(listing.id()).await;
    rig.bring_server_back();

    let result = task.await.expect("listing task must not panic");
    assert!(result.is_ok(), "got {result:?}");
    assert_eq!(rig.completed(listing.id()), Some(3));
    assert!(rig.volume.calls.load(Ordering::SeqCst) >= 4);
}

/// A prompt, lasting answer after the stall is the answer: a folder that's gone
/// once the server is back ends the listing, so the pane can walk up.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_prompt_lasting_refusal_after_a_stall_ends_the_listing() {
    // A long backoff keeps the background probe out of the way: this test is about
    // what the second, prompt answer means.
    let policy = StallPolicy {
        first_backoff: Duration::from_secs(60),
        max_backoff: Duration::from_secs(60),
        ..TEST_POLICY
    };
    let rig = Rig::with_policy(vec![hang_then(Answer::NotConnected), prompt(Answer::NotFound)], policy);
    let listing = TestListingGuard::adopt(unique_test_id("stall-gone"));

    let (_state, task) = rig.start(listing.id());
    rig.wait_for_stall(listing.id()).await;
    rig.bring_server_back();

    let result = task.await.expect("listing task must not panic");
    assert!(matches!(result, Err(VolumeError::NotFound(_))), "got {result:?}");
    assert!(rig.completed(listing.id()).is_none());
}

/// Leaving the folder stops the background retries: no read starts after the cancel.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_stalled_listing_stops_its_retries() {
    let rig = Rig::new(vec![hang_then(Answer::TimedOut), prompt(Answer::TimedOut)]);
    let listing = TestListingGuard::adopt(unique_test_id("stall-cancel"));

    let (state, task) = rig.start(listing.id());
    rig.wait_for_stall(listing.id()).await;
    rig.bring_server_back();
    wait_until_async(WITHIN, "a few background retries", || {
        rig.volume.calls.load(Ordering::SeqCst) >= 4
    })
    .await;

    state.cancel.cancel();
    task.await
        .expect("listing task must not panic")
        .expect("a cancel is not a refusal");
    let calls_at_cancel = rig.volume.calls.load(Ordering::SeqCst);
    // allowed-test-sleep: asserting that NOTHING happens needs a window to watch; it spans
    // several max backoffs, so a retry loop that outlived the cancel would show in it
    tokio::time::sleep(TEST_POLICY.max_backoff * 5).await;
    assert_eq!(rig.volume.calls.load(Ordering::SeqCst), calls_at_cancel);
}

/// Every read on a hung mount pins a blocking thread until the kernel lets go,
/// so a volume known to be hung takes only a few at once, however many times the
/// user retries or navigates.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reads_on_a_hung_volume_stay_bounded_however_many_listings_start() {
    let rig = Rig::new(vec![hang_then(Answer::Entries(1))]);
    let first = TestListingGuard::adopt(unique_test_id("stall-bound-first"));
    let (_first_state, _first_task) = rig.start(first.id());
    rig.wait_for_stall(first.id()).await;

    let mut more = Vec::new();
    for i in 0..10 {
        let guard = TestListingGuard::adopt(unique_test_id(&format!("stall-bound-{i}")));
        let (state, task) = rig.start(guard.id());
        more.push((guard, state, task));
    }
    // Every later listing stalls too, which is the moment each one has started
    // (or been refused) the reads it's going to start.
    for (guard, _, _) in &more {
        rig.wait_for_stall(guard.id()).await;
    }

    let peak = rig.volume.max_in_flight.load(Ordering::SeqCst);
    assert!(
        peak <= crate::file_system::listing::stall::MAX_READS_ON_A_HUNG_VOLUME,
        "{peak} reads in flight on one hung volume"
    );

    // And they all land once the server is back.
    rig.bring_server_back();
    for (guard, _, task) in more {
        task.await
            .expect("listing task must not panic")
            .expect("lists once back");
        assert_eq!(rig.completed(guard.id()), Some(1));
    }
}

/// A slow folder that keeps producing entries is busy, not stalled.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_that_keeps_making_progress_never_reports_a_stall() {
    struct SlowButSteady {
        root: PathBuf,
    }
    impl Volume for SlowButSteady {
        fn name(&self) -> &str {
            "Slow but steady"
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
            on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
            Box::pin(async move {
                let steps = 8;
                for files in 1..=steps {
                    // allowed-test-sleep: this fake backend IS a slow read; each wait is one
                    // batch of entries arriving, shorter than the stall deadline
                    tokio::time::sleep(TEST_POLICY.stall_after / 3).await;
                    if let Some(report) = on_progress {
                        report(ListingProgress {
                            files,
                            dirs: 0,
                            bytes: 0,
                        });
                    }
                }
                Ok((0..steps).map(entry).collect())
            })
        }
        fn get_metadata<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotFound("not scripted".to_string())) })
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

    let volume_id = format!("test-steady-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(
        &volume_id,
        Arc::new(SlowButSteady {
            root: PathBuf::from("/"),
        }) as Arc<dyn Volume>,
    );
    let listing = TestListingGuard::adopt(unique_test_id("stall-steady"));
    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = Arc::new(StreamingListingState::with_stall_watch(StallWatch {
        policy: TEST_POLICY,
        reads: Arc::new(HungReads::new()),
    }));

    let result = read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        &volume_id,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;
    get_volume_manager().unregister(&volume_id);

    assert!(result.is_ok(), "got {result:?}");
    assert!(
        sink.stalled.lock_ignore_poison().is_empty(),
        "a steady read must not stall"
    );
    assert_eq!(sink.complete.lock_ignore_poison().len(), 1);
}
