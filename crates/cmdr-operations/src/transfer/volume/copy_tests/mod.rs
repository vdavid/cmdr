//! Tests for `copy.rs` (`volume::copy::tests`), split into one child per
//! behavior under test. Shared imports and the `make_state` / `make_volumes`
//! fixtures live here; every child starts with `use super::*;`. Sibling
//! `#[path]` suites reach the fixtures as `super::tests::make_state`.

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::{ConflictResolution, WriteConflictResolvedEvent, WriteErrorEvent, WriteSourceItemDoneEvent};
use cmdr_fs::volume::{InMemoryVolume, ListingProgress};

// `pub(super)` so the sibling `volume_copy_crashsafe_tests` and
// `volume_copy_rollback_tests` modules can share these fixtures without
// duplicating them.
pub(super) fn make_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(50)))
}

pub(super) fn make_volumes() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    (
        Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)),
        Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
    )
}

mod cancellation;
mod conflicts;
mod destination;
mod policy_latch;
mod progress;
mod scan;
