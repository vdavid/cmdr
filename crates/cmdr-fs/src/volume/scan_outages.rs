//! What a copy scan does when one of its reads fails: it asks the scan's
//! owner, who may wait out a dropped session and have the read run again.
//!
//! A scan over a big tree on a share is minutes of listings, and a router
//! restart in the middle of it used to fail the whole transfer before it copied
//! a byte. The patience lives with the owner (a write operation has the budget,
//! the Cancel, the Pause, and the progress UI; a backend has none of them), and
//! the RETRY lives here, next to the walk, because only the walk knows which
//! round trip failed and that running it again is safe.
//!
//! ❗ **Only a read goes through it, and nothing is counted inside it.** A
//! listing or a stat is safe to send twice; the entries it returns reach the
//! [`ScanBoundary`](super::ScanBoundary) only after it succeeds. So a read run
//! again after a wait counts each entry once, and the walk carries on at the
//! directory it was listing, ❌ never from the top.

use std::path::Path;
use std::pin::Pin;

use super::VolumeError;

/// The scan's owner, asked about every read the walk sends.
///
/// Implemented by whoever owns the scan (a write operation); a `Volume` backend
/// never implements it, it only routes its reads through
/// [`ScanBoundary::through_outages`](super::ScanBoundary::through_outages).
pub trait ScanOutages: Sync {
    /// A read of `path` is about to go out. The judge it returns answers that
    /// read's failures, so it takes whatever it judges them by (which session
    /// the volume is on) NOW, before the round trip: after the error, a
    /// reconnect may already have moved it.
    fn read<'a>(&'a self, path: &'a Path) -> Box<dyn ScanReadJudge + Send + 'a>;
}

/// Judges one read's failures, for as many times as it fails.
pub trait ScanReadJudge {
    /// The read failed with `error`. On [`ScanReadNext::Again`] the judge has
    /// taken a fresh snapshot for the read that's about to run again.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn after_failure<'a>(
        &'a mut self,
        error: &'a VolumeError,
    ) -> Pin<Box<dyn Future<Output = ScanReadNext> + Send + 'a>>;
}

/// What a read does after it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanReadNext {
    /// The volume went away and is back: send the read again.
    Again,
    /// The error stands, and ends the scan.
    Fail,
    /// The owner stopped (a Cancel) while it waited: the scan ends as a
    /// stopped one.
    Stopped,
}
