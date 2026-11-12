//! The rename that sets an Overwrite's destination aside, when the share drops
//! under it.
//!
//! A rename the transport cut off may have been performed by the server, so
//! the user's file may already sit under the aside's scratch name. The record
//! that says what that file is must outlive the error: the rename waits for the
//! share and settles, and a wait that ends without the share leaves the record
//! for the sweep that next reaches it, which puts the file back.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::faulty_volume::{FaultyOp, FaultyVolume};
use super::*;
use crate::in_flight_temps::Ledger;
use crate::outage_slot::OutagePolicy;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;

const NAS_ID: &str = "displaced-destination-nas";
const ORIGINAL: &str = "/photos/holiday.jpg";
const BYTES: &[u8] = b"the user's photo";
const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

/// What the share does while the rename waits for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meanwhile {
    ComesBack,
    StaysGone,
}

/// A share holding the user's file at [`ORIGINAL`], and an operation writing
/// to it that records into a ledger of its own.
struct Share {
    engine: TestOperations,
    inner: Arc<InMemoryVolume>,
    nas: Arc<OutageVolume>,
    ledger: Ledger,
    state: Arc<WriteOperationState>,
}

async fn share(lane: &str, meanwhile: Meanwhile) -> Share {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("Testnas"));
    inner.create_directory(Path::new("/photos")).await.unwrap();
    inner.create_file(Path::new(ORIGINAL), BYTES).await.unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&inner), lane);
    let ledger = Ledger::for_test();
    let budget = match meanwhile {
        Meanwhile::StaysGone => Duration::from_millis(200),
        Meanwhile::ComesBack => Duration::from_secs(30),
    };
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("displaced-destination-source".into(), NAS_ID.into())
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    );
    if meanwhile == Meanwhile::ComesBack {
        let state = Arc::clone(&state);
        let nas = Arc::clone(&nas);
        tokio::spawn(async move {
            wait_until_async(WAIT, "the rename to wait for the share", || {
                state.outage.waiting_for().is_some()
            })
            .await;
            nas.become_reachable();
        });
    }
    Share {
        engine,
        inner,
        nas,
        ledger,
        state,
    }
}

async fn read_all(volume: &dyn Volume, path: &Path) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(path).await.ok()?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        buf.extend_from_slice(&chunk.ok()?);
    }
    Some(buf)
}

impl Share {
    fn volume(&self) -> Arc<dyn Volume> {
        Arc::clone(&self.nas) as Arc<dyn Volume>
    }

    async fn displace(&self) -> Result<Option<DisplacedDestination>, PathedVolumeError> {
        tokio::time::timeout(
            WAIT,
            displace_destination(&self.state, &self.volume(), Path::new(ORIGINAL), false),
        )
        .await
        .expect("the rename aside ends, one way or the other")
    }

    /// Every `.cmdr-temp-` aside beside the original.
    async fn asides(&self) -> Vec<PathBuf> {
        self.inner
            .list_directory(Path::new("/photos"), None)
            .await
            .unwrap()
            .into_iter()
            .filter(|entry| entry.name.contains(cmdr_fs::staging::STAGING_ASIDE_MARKER))
            .map(|entry| PathBuf::from(entry.path))
            .collect()
    }

    /// The one aside, holding the user's bytes.
    async fn the_aside(&self) -> PathBuf {
        let asides = self.asides().await;
        let [aside] = asides.as_slice() else {
            panic!("exactly one aside holds the user's file, got {asides:?}");
        };
        assert_eq!(
            read_all(&*self.inner, aside).await.as_deref(),
            Some(BYTES),
            "❗ the aside holds the user's file"
        );
        aside.clone()
    }

    fn records(&self, aside: &Path) -> bool {
        self.ledger.live_paths().iter().any(|path| path == aside)
    }

    async fn original(&self) -> Option<Vec<u8>> {
        read_all(&*self.inner, Path::new(ORIGINAL)).await
    }

    /// The share answers again and registers, so its arrival sweep runs.
    async fn arrives(&self) {
        self.nas.reconnected_elsewhere();
        self.engine.volumes().register(NAS_ID, self.volume());
    }
}

/// The server set the file aside and the drop swallowed the reply: once the
/// share is back, the aside is what the rename did, so it's held (and
/// recorded), ❌ never forgotten under its scratch name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_aside_whose_reply_was_lost_settles_as_set_aside() {
    let share = share("displaced-reply-lost", Meanwhile::ComesBack).await;
    share.nas.drop_at_rename(0, RenameDrop::AfterTheServerPerformsIt);

    let displaced = share
        .displace()
        .await
        .expect("the aside settles once the share is back");

    assert!(share.nas.reconnect_attempts() > 0, "the share dropped");
    let displaced = displaced.expect("the server had set the file aside");
    let aside = share.the_aside().await;
    assert_eq!(share.original().await, None, "the name is free for the replacement");
    assert!(share.records(&aside), "the aside stays recorded while it's held");

    assert_eq!(displaced.restore(&share.volume()).await, None, "and it comes home");
    assert_eq!(share.original().await.as_deref(), Some(BYTES));
    assert!(!share.records(&aside), "nothing left recorded once it's home");
}

/// The drop hit before the server saw the rename: once the share is back, the
/// file is still at its name, so the rename runs again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_aside_the_server_never_saw_runs_again_once_the_share_is_back() {
    let share = share("displaced-unsent", Meanwhile::ComesBack).await;
    share.nas.drop_at_rename(0, RenameDrop::BeforeTheServerSeesIt);

    let displaced = share.displace().await.expect("the aside lands once the share is back");

    assert!(share.nas.reconnect_attempts() > 0, "the share dropped");
    assert!(displaced.is_some(), "the file was set aside");
    let aside = share.the_aside().await;
    assert_eq!(share.original().await, None);
    assert!(share.records(&aside));
}

/// ❗ The server set the file aside, and the share never came back within the
/// wait: the rename fails, but the record stays, so the share's next arrival
/// puts the file back at its name. Retiring it on the error left the user's
/// file under a scratch name nothing knew about.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_stays_gone_after_the_rename_aside_keeps_its_record_for_the_arrival_sweep() {
    let share = share("displaced-stays-gone", Meanwhile::StaysGone).await;
    share.nas.drop_at_rename(0, RenameDrop::AfterTheServerPerformsIt);

    let failure = share.displace().await.expect_err("the wait gave up");

    assert!(
        matches!(
            failure.cause.as_deref(),
            Some(super::super::transfer_error::FailureCause::LostConnection(_))
        ),
        "the failure says it was the connection: {failure:?}"
    );
    let aside = share.the_aside().await;
    assert!(share.records(&aside), "❗ the record outlives the error");

    share.arrives().await;
    wait_until_async(WAIT, "the arrival sweep to put the file back", || {
        share.ledger.live_paths().is_empty()
    })
    .await;
    assert_eq!(share.original().await.as_deref(), Some(BYTES), "the file is home");
    assert_eq!(share.asides().await, Vec::<PathBuf>::new());
}

/// The share never came back, and the server never saw the rename: the record
/// can't know that while the share is out, so it stays, and the arrival sweep
/// finds no aside and retires it, ❌ instead of deferring it at every arrival.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_aside_the_server_never_saw_retires_its_record_on_the_next_arrival() {
    let share = share("displaced-unsent-stays-gone", Meanwhile::StaysGone).await;
    share.nas.drop_at_rename(0, RenameDrop::BeforeTheServerSeesIt);

    share.displace().await.expect_err("the wait gave up");
    assert_eq!(share.asides().await, Vec::<PathBuf>::new(), "nothing was set aside");
    assert_eq!(share.ledger.live_paths().len(), 1, "unknown while the share is out");

    share.arrives().await;
    wait_until_async(WAIT, "the arrival sweep to retire the record", || {
        share.ledger.live_paths().is_empty()
    })
    .await;
    assert_eq!(share.original().await.as_deref(), Some(BYTES), "the file never moved");
}

/// A refusal is no outage: it fails at once, and the record of an aside that
/// never happened goes with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_rename_aside_fails_at_once_and_retires_its_record() {
    let share = share("displaced-refused", Meanwhile::ComesBack).await;
    let refusing = FaultyVolume::wrapping(Arc::clone(&share.nas))
        .failing_call(
            FaultyOp::Rename,
            1,
            VolumeError::PermissionDenied {
                path: ORIGINAL.to_owned(),
                raw_os_error: None,
            },
        )
        .arc();
    let volume = Arc::clone(&refusing) as Arc<dyn Volume>;

    let failure = displace_destination(&share.state, &volume, Path::new(ORIGINAL), false)
        .await
        .expect_err("a refusal fails the rename");

    assert!(refusing.fault_fired(FaultyOp::Rename));
    assert!(matches!(failure.error, VolumeError::PermissionDenied { .. }));
    assert_eq!(share.nas.reconnect_attempts(), 0, "nothing to wait for");
    assert_eq!(share.original().await.as_deref(), Some(BYTES));
    assert!(share.ledger.live_paths().is_empty(), "nothing claimed that isn't there");
}
