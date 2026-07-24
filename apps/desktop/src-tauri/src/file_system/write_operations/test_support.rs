//! Test isolation for the process-global `WRITE_OPERATION_STATE` map.
//!
//! Only the cross-cutting global-state fixture lives here. Per-driver fixtures
//! (fake volumes, gated sources, collector sinks) stay in their own module's
//! `test_support`, like `transfer/test_support.rs`.

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
