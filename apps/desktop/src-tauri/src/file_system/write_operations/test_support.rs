//! Cross-cutting write-operation test fixtures: isolation for the process-global
//! `WRITE_OPERATION_STATE` map, and the one sanctioned "a park is holding" wait.
//!
//! Per-driver fixtures (fake volumes, gated sources, collector sinks) stay in
//! their own module's `test_support`, like `transfer/test_support.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::ignore_poison::RwLockIgnorePoison;

use super::state::{WRITE_OPERATION_STATE, WriteOperationState};

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
        WRITE_OPERATION_STATE
            .write_ignore_poison()
            .insert(op_id.clone(), Arc::clone(&state));
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
        WRITE_OPERATION_STATE.write_ignore_poison().remove(&self.op_id);
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
    // allowed-test-sleep: lets whatever was already past its checkpoint finish, so the sample
    // below is the parked value rather than a mid-flight one. No signal marks "the op reached
    // its park".
    tokio::time::sleep(PARK_WINDOW).await;
    let frozen = progress();
    // allowed-test-sleep: the negative assertion itself. A running op would advance several
    // units across this window, so a stable value is what proves the park is holding.
    tokio::time::sleep(PARK_WINDOW).await;
    assert_eq!(progress(), frozen, "{what}");
    frozen
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
