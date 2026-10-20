//! A compress's planning walk over a share that drops mid-walk waits for it,
//! the way the copy's pre-flight scan does (`transfer/volume/scan_wait.rs`).
//!
//! The walk's reads (the selected item's type and stat, each folder's
//! listing) are safe to send twice, and an entry joins the plan only after its
//! parent's listing answers, so a listing run again after the wait plans every
//! entry once, in tree order.

use std::sync::Mutex;

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::outage_slot::OutagePolicy;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::OutageVolume;
use crate::transfer::volume::{FaultyOp, FaultyVolume};
use crate::types::{TransferWaitReason, WriteOperationPhase};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;

const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

fn outage_state(engine: &TestOperations, budget: Duration) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::ZERO)
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    )
}

/// A share holding `/photos` (two files) and `/photos/trip` (two more).
async fn nas(lane: &str) -> Arc<OutageVolume> {
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    inner.create_directory(Path::new("/photos")).await.unwrap();
    inner.create_directory(Path::new("/photos/trip")).await.unwrap();
    for path in [
        "/photos/a.jpg",
        "/photos/b.jpg",
        "/photos/trip/c.jpg",
        "/photos/trip/d.jpg",
    ] {
        inner.create_file(Path::new(path), b"pixels").await.unwrap();
    }
    OutageVolume::wrap(inner, lane)
}

/// The plan the walk makes with no drop: every entry once, in tree order.
const PLANNED: &[&str] = &[
    "photos/",
    "photos/trip/",
    "photos/trip/c.jpg",
    "photos/trip/d.jpg",
    "photos/a.jpg",
    "photos/b.jpg",
];

async fn plan(
    state: &Arc<WriteOperationState>,
    source: &Arc<dyn Volume>,
    events: &CollectorEventSink,
) -> Result<FreshPlan, EditError> {
    let ticks = Mutex::new(0usize);
    let on_progress = |_tick: PlanProgress<'_>| *ticks.lock().unwrap() += 1;
    plan_sources(
        source,
        &[PathBuf::from("/photos")],
        ConflictResolution::Stop,
        state,
        events,
        "op-plan-outage",
        Path::new("/out/photos.zip"),
        &on_progress,
    )
    .await
}

fn describe(error: &EditError) -> String {
    match error {
        EditError::Cancelled => "Cancelled".to_owned(),
        EditError::Op(error) => format!("{error:?}"),
    }
}

fn names(plan: &FreshPlan) -> Vec<&str> {
    plan.entries.iter().map(|entry| entry.name.as_str()).collect()
}

/// The share drops as the walk lists `/photos/trip`. The compress waits,
/// saying so on its progress, lists that folder again once the share is back,
/// and plans every entry once, in the order a walk with no drop plans them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_planning_walk_the_share_drops_under_waits_and_plans_every_entry_once() {
    let engine = TestOperations::new();
    let nas = nas("outage-plan-waits").await;
    nas.drop_at_listing_of(Path::new("/photos/trip"));
    let source = Arc::clone(&nas) as Arc<dyn Volume>;
    let state = outage_state(&engine, Duration::from_secs(30));
    {
        let state = Arc::clone(&state);
        let nas = Arc::clone(&nas);
        tokio::spawn(async move {
            wait_until_async(WAIT, "the walk to wait for the share", || {
                state.outage.waiting_for().is_some()
            })
            .await;
            nas.become_reachable();
        });
    }
    let events = CollectorEventSink::new();

    let plan = match plan(&state, &source, &events).await {
        Ok(plan) => plan,
        Err(error) => panic!(
            "a share that comes back within the budget must not fail the compress: {}",
            describe(&error)
        ),
    };

    assert_eq!(names(&plan), PLANNED, "every entry once, in tree order");
    assert_eq!(plan.source_bytes, 4 * "pixels".len() as u64);
    assert!(nas.reconnect_attempts() >= 1, "the walk reconnected the share itself");
    assert_eq!(
        nas.listings_of(Path::new("/photos")),
        1,
        "the walk carried on, never from the top"
    );
    assert_eq!(
        nas.listings_of(Path::new("/photos/trip")),
        2,
        "the dropped listing ran again"
    );
    let progress = events.progress.lock().unwrap();
    assert!(
        progress.iter().any(|event| event.phase == WriteOperationPhase::Scanning
            && event
                .activity
                .as_ref()
                .is_some_and(|activity| activity.waiting_on == TransferWaitReason::Reconnecting)),
        "the walk's progress said it was waiting for the share: {progress:?}"
    );
}

/// The share never comes back: the compress stops with "Lost the connection
/// to Naspolya", the folder it was listing as context.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_planning_walk_whose_share_stays_gone_says_it_lost_the_connection() {
    let engine = TestOperations::new();
    let nas = nas("outage-plan-gone").await;
    nas.drop_at_listing_of(Path::new("/photos/trip"));
    let source = Arc::clone(&nas) as Arc<dyn Volume>;
    let state = outage_state(&engine, Duration::from_millis(300));

    let Err(error) = plan(&state, &source, &CollectorEventSink::new()).await else {
        panic!("a share that never comes back still fails the compress");
    };

    assert!(
        matches!(
            &error,
            EditError::Op(WriteOperationError::ServerConnectionLost { path, volume_name, gave_up_after_secs: Some(_) })
                if path == "/photos/trip" && volume_name == "Naspolya"
        ),
        "the connection is the cause, named with the wait: {}",
        describe(&error)
    );
    assert!(nas.reconnect_attempts() > 1, "after really trying");
}

/// A server that refuses a listing with its session up gave an answer, not an
/// outage: the walk fails at once, with nothing dialed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_the_server_refuses_fails_the_planning_walk_at_once() {
    let engine = TestOperations::new();
    let nas = nas("outage-plan-refused").await;
    let faulty = FaultyVolume::wrapping(Arc::clone(&nas))
        .failing_call_at(
            FaultyOp::ListDirectory,
            "/photos/trip",
            1,
            VolumeError::PermissionDenied {
                path: "/photos/trip".to_owned(),
                raw_os_error: None,
            },
        )
        .arc();
    let source = Arc::clone(&faulty) as Arc<dyn Volume>;
    let state = outage_state(&engine, Duration::from_secs(30));

    let Err(error) = plan(&state, &source, &CollectorEventSink::new()).await else {
        panic!("a refusal fails the walk");
    };

    assert!(faulty.fault_fired(FaultyOp::ListDirectory), "the refusal happened");
    assert!(
        matches!(&error, EditError::Op(WriteOperationError::ReadError { path, .. }) if path == "/photos/trip"),
        "the walk's own read error, as before: {}",
        describe(&error)
    );
    assert_eq!(nas.reconnect_attempts(), 0, "nothing waited");
}
