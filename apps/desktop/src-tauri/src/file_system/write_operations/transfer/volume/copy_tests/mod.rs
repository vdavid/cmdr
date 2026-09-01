//! Tests for `copy.rs` (`volume::copy::tests`), split into one child per
//! behavior under test. Shared imports and the `make_state` / `make_volumes`
//! fixtures live here; every child starts with `use super::*;`. Sibling
//! `#[path]` suites reach the fixtures as `super::tests::make_state`.

use super::super::super::conflict_responder_test_support::await_prompted_clash;
use super::super::cleanup::{TreeRemoval, remove_tree};
use super::super::transfer_error::{PathRole, map_volume_error};
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{CopyScanResult, InMemoryVolume, ListingProgress, LocalPosixVolume};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{
    ConflictResolution, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteSourceItemDoneEvent,
};
use crate::test_support::TestDir;
use std::sync::atomic::AtomicU8;

// `pub(super)` so the sibling `volume_copy_crashsafe_tests` and
// `volume_copy_rollback_tests` modules can share these fixtures without
// duplicating them.
pub(super) fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
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
mod error_mapping;
mod progress;
mod remove_tree;
mod scan;
mod scan_hints;
