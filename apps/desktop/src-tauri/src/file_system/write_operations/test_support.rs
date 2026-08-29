//! Cross-cutting write-operation test fixtures: isolation for the process-global
//! `WRITE_OPERATION_STATE` map, the one sanctioned "a park is holding" wait, and
//! a real queued operation for suites outside this module.
//!
//! Per-driver fixtures (fake volumes, gated sources, collector sinks) stay in
//! their own module's `test_support`, like `transfer/test_support.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::oneshot;

use super::manager::{OperationDescriptor, OperationSummaryText, manager};
use super::state::{WRITE_OPERATION_STATE, WriteOperationState};
use super::types::{ConflictId, LifecycleStatus, WriteConflictEvent, WriteOperationType};
use crate::file_system::volume::LaneKey;

/// A stand-in `write-conflict` event for a test that arms the conflict slot only
/// to park an operation, and never reads the question back.
///
/// Arming takes the question it is arming for, so a test that cares about
/// nothing but the park still has to supply one. Passing this by name
/// (`slot.arm(tx, placeholder_conflict)`) says exactly that at the call site,
/// and keeps a dozen irrelevant fields out of it.
pub(crate) fn placeholder_conflict(conflict_id: ConflictId) -> WriteConflictEvent {
    WriteConflictEvent {
        operation_id: "op-under-test".to_string(),
        conflict_id,
        source_path: "/src/placeholder.txt".to_string(),
        destination_path: "/dst/placeholder.txt".to_string(),
        source_size: Some(1),
        destination_size: Some(2),
        source_modified: None,
        destination_modified: None,
        destination_is_newer: false,
        size_difference: Some(1),
        source_is_directory: false,
        destination_is_directory: false,
    }
}

/// A `WRITE_OPERATION_STATE` entry registered under a unique-per-test operation
/// id, removed on drop.
///
/// **Why this exists.** `cargo test` runs a crate's tests as threads in ONE
/// process, so `WRITE_OPERATION_STATE` is shared by every write-op test at once.
/// A hardcoded op id (`"op-merge-cancel"`) collides with any sibling test using
/// the same literal, and a manual `remove` placed after the assertions leaks the
/// entry whenever an assertion fails first: the next test's
/// `cancel_all_write_operations` then walks a corpse, and `list_active_operations`
/// counts it. A UNIQUE id plus removal from `Drop` fixes both — `Drop` runs on
/// unwind, so a panicking test cleans up too.
///
/// Mirrors `indexing::tests::stress_test_helpers::TestInstanceGuard`, the same
/// pattern over `INDEX_REGISTRY`. Keep the guard on the stack: a `std::mem::forget`
/// or an `Arc` that outlives the test defeats the whole thing.
pub(crate) struct TestOperationGuard {
    op_id: String,
    state: Arc<WriteOperationState>,
}

impl TestOperationGuard {
    /// Registers a fresh `WriteOperationState` (50 ms progress interval) under a
    /// unique id derived from `tag`.
    pub(crate) fn register(tag: &str) -> Self {
        Self::register_state(tag, Arc::new(WriteOperationState::new(Duration::from_millis(50))))
    }

    /// Registers a caller-built state (the drivers' `make_state()` fixtures) under
    /// a unique id derived from `tag`.
    pub(crate) fn register_state(tag: &str, state: Arc<WriteOperationState>) -> Self {
        Self::register_as(unique_op_id(tag), state)
    }

    /// Registers `state` under an operation id the caller already generated. For
    /// suites with their own id generator (`transfer_driver`'s `unique_op_id`),
    /// where the id threads through the call under test and its assertions.
    pub(crate) fn register_as(op_id: impl Into<String>, state: Arc<WriteOperationState>) -> Self {
        let op_id = op_id.into();
        WRITE_OPERATION_STATE.insert(op_id.clone(), Arc::clone(&state));
        Self { op_id, state }
    }

    /// The unique operation id this state is registered under. Pass it wherever a
    /// test would have used a literal.
    pub(crate) fn id(&self) -> &str {
        &self.op_id
    }

    /// The registered state, for tests that read `intent` / `backend_cancel` /
    /// `pause_gate` directly.
    pub(crate) fn state(&self) -> &Arc<WriteOperationState> {
        &self.state
    }
}

impl Drop for TestOperationGuard {
    fn drop(&mut self) {
        super::state::forget_operation(&self.op_id);
    }
}

/// How long a "the op is parked" window runs. Long enough that a running op would
/// have advanced several units inside it, short enough to keep the suites quick.
pub(crate) const PARK_WINDOW: Duration = Duration::from_millis(40);

/// Asserts a park is HOLDING: waits one [`PARK_WINDOW`] for whatever was already
/// in flight to drain into the park, samples `progress`, then holds a second
/// window and asserts the sample never moved. Returns the parked value.
///
/// "Nothing happened" has no signal to wait on, so a window is the only evidence
/// available. Give the op an unlimited budget first (lift the chunk or file gate)
/// so a frozen count can only mean the park is holding, never a starved source.
///
/// Every frozen-progress check in the write-operation suites routes through here,
/// which is why these are the only two fixed waits left in them: keep it that way
/// rather than sprinkling `sleep` back into the tests.
pub(crate) async fn park_holds_at(progress: impl Fn() -> u64, what: &str) -> u64 {
    // Let whatever was already past its checkpoint finish, so the sample below is the parked
    // value rather than a mid-flight one. No signal marks "the op reached its park".
    // allowed-test-sleep: no signal exists for "the op is now parked"; a window is the only evidence.
    tokio::time::sleep(PARK_WINDOW).await;
    let frozen = progress();
    // The negative assertion itself: a running op would advance several units across this window,
    // so a stable value is what proves the park is holding.
    // allowed-test-sleep: negative assertion over a window; "nothing advanced" has nothing to await.
    tokio::time::sleep(PARK_WINDOW).await;
    assert_eq!(progress(), frozen, "{what}");
    frozen
}

/// Whether this test has the whole process to itself, so it may drive a
/// process-global mutator without stopping another test's work.
///
/// nextest — the sanctioned runner (`docs/testing.md`) — forks a process per
/// test and says so in the environment. Plain `cargo test` runs the crate's
/// tests as threads in ONE process, where a walk-everything mutator like
/// `cancel_all_write_operations` reaches whatever else is in flight.
///
/// ❌ Not a licence to write global-mutating tests: the scoped fixture
/// (`TestOperationGuard`, or a `WriteOperationRegistry` the test owns) is always
/// the answer. This exists only for the handful of assertions whose SUBJECT is
/// the global wiring itself, which have nothing to assert against a private
/// instance.
pub(crate) fn one_test_per_process() -> bool {
    std::env::var("NEXTEST_EXECUTION_MODE").as_deref() == Ok("process-per-test")
}

/// A process-unique operation id. The counter alone would collide across
/// concurrently-running test binaries, so the pid goes in too.
fn unique_op_id(tag: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "test-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// A real `Queued` operation, waiting behind a holder that owns a private lane
/// until this fixture drops.
///
/// **Why this exists.** A suite OUTSIDE this module has no way to produce a
/// queued row (`manager` is private here, and a real transfer needs a busy
/// device), yet the consumers that misread one live out there: the MCP `queue`
/// tool answered "OK: Paused …" for a queued operation that pause is documented
/// to leave alone. Two synthetic operations on one private lane reproduce the
/// state exactly, with no I/O and no reach into another test's operations.
/// Sibling of [`TestOperationGuard`], which covers the state map rather than the
/// manager.
pub(crate) struct QueuedOperationFixture {
    queued_id: String,
    /// Releases, holder first. `Drop` sends both so each operation settles and
    /// the lane frees for the rest of the process.
    releases: Vec<oneshot::Sender<()>>,
}

impl QueuedOperationFixture {
    /// Registers the holder (admitted at once, since its lane is fresh) and then
    /// the operation that queues behind it.
    ///
    /// Admission runs inside `spawn_managed`, so both statuses are already
    /// settled when this returns; it asserts them rather than waiting, which
    /// makes a manager change that breaks the premise fail here instead of
    /// silently weakening whatever test uses the fixture.
    pub(crate) fn park(tag: &str) -> Self {
        let lane = LaneKey::new(unique_op_id(&format!("{tag}-lane")));
        let holder_id = unique_op_id(&format!("{tag}-holder"));
        let queued_id = unique_op_id(&format!("{tag}-queued"));

        let mut releases = Vec::new();
        for id in [&holder_id, &queued_id] {
            let (release_tx, release_rx) = oneshot::channel();
            releases.push(release_tx);
            let settle_id = id.clone();
            manager().spawn_managed(
                OperationDescriptor {
                    operation_id: id.clone(),
                    operation_type: WriteOperationType::Copy,
                    lanes: vec![lane.clone()],
                    volume_ids: vec![],
                    summary: OperationSummaryText::default(),
                    supports_rollback: false,
                    preview_id: None,
                    reverses: None,
                },
                Arc::new(WriteOperationState::new(Duration::from_millis(50))),
                Box::new(move || {
                    Box::pin(async move {
                        let _ = release_rx.await;
                        manager().on_settled(&settle_id);
                    })
                }),
            );
        }

        assert_eq!(
            manager().lifecycle_status(&holder_id),
            Some(LifecycleStatus::Running),
            "the holder takes its own fresh lane, so admission runs it immediately"
        );
        assert_eq!(
            manager().lifecycle_status(&queued_id),
            Some(LifecycleStatus::Queued),
            "the second operation shares the lane, so it waits"
        );

        Self { queued_id, releases }
    }

    /// The id of the operation sitting `Queued`.
    pub(crate) fn queued_id(&self) -> &str {
        &self.queued_id
    }
}

impl Drop for QueuedOperationFixture {
    fn drop(&mut self) {
        for release in self.releases.drain(..) {
            let _ = release.send(());
        }
    }
}
