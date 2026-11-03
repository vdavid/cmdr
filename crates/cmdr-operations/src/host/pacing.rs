//! Deliberate slowness for automated runs: a pause after each step of a copy,
//! delete, or archive rewrite, a held start for a scan preview, and a pace for a
//! rollback, so an E2E spec has a known window to press Cancel in. Cmdr's answer
//! is `operations_host::pacing`, over `test_mode`; every real build answers
//! `None` everywhere.

use std::path::Path;
use std::time::Duration;

/// How long the engine waits, where it deliberately waits.
///
/// Every answer is `None` (no pause) or a non-zero `Some`: a zero-length sleep
/// would still yield on every item. Asked per file or per item, beside I/O that
/// already costs more than the call.
pub trait Pacing: Send + Sync {
    /// The pause after each step of a copy, a delete batch, or an archive
    /// rewrite's entry.
    fn copy_throttle(&self) -> Option<Duration>;

    /// [`Self::copy_throttle`], plus any pace the host sets for a file copied
    /// out of `source`'s folder.
    fn copy_throttle_for(&self, source: &Path) -> Option<Duration>;

    /// How long a scan preview's worker waits before it walks.
    fn scan_preview_delay(&self) -> Option<Duration>;

    /// The pause before a rollback reverses each item, and after each item a
    /// Rollback-cancelled transfer's own reversal walks (`WindDown::pace`).
    fn rollback_pace(&self) -> Option<Duration>;
}

/// The answer for a host that never paces: a bench, a tool, an engine before
/// the app wired its own.
pub(super) struct NoPacing;

impl Pacing for NoPacing {
    fn copy_throttle(&self) -> Option<Duration> {
        None
    }

    fn copy_throttle_for(&self, _source: &Path) -> Option<Duration> {
        None
    }

    fn scan_preview_delay(&self) -> Option<Duration> {
        None
    }

    fn rollback_pace(&self) -> Option<Duration> {
        None
    }
}
