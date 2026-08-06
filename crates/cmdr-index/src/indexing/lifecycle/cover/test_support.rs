//! Helpers every cover test file shares.
//!
//! The fixtures themselves stay with the tests that use them: the temp-tree
//! `Fixture` in `tests.rs`, the `ColdDrive` in `cold_drive_tests.rs`. Only what
//! more than one of them needs lands here.

use super::{CoverOutcome, CoverWalk, CoveredEntry};

/// Drain a walk, collecting every entry it emitted.
pub(super) fn drain(walk: CoverWalk) -> (Vec<CoveredEntry>, CoverOutcome) {
    let mut entries = Vec::new();
    while let Some(batch) = walk.next_batch() {
        entries.extend(batch);
    }
    (entries, walk.finish())
}
