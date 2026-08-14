//! What a request to (re)scan a volume answers, and the one request a volume
//! remembers.
//!
//! A scan start has two doors it can be turned away at, and both are single-flight
//! questions rather than failures (`../DETAILS.md` § "The two single-flight
//! questions a scan has to ask"). [`ScanStartError`] is how a caller tells them
//! apart without reading a sentence: the wording of a diagnostic is for logs, and
//! classifying control flow by it breaks the moment someone edits it.
//!
//! ## Why a volume remembers one request
//!
//! A cover walk holds ground for seconds to minutes, and the person who clicked
//! "Rescan now" has no way to see when it lets go. Telling them to click again
//! puts the scheduling on the one participant who can't observe the schedule, so
//! the request is remembered here and run by the walk that blocked it
//! (`cover::start`'s thread, once its claim is gone).
//!
//! Deliberately ONE request per volume rather than a queue: the request carries no
//! payload beyond "this volume wants a full walk", so a second click describes the
//! same work, and a set of volume ids is the whole state. It lives in memory only,
//! so quitting drops it; a teardown drops it too, since a volume that stopped
//! indexing is owed nothing.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;

/// Why a (re)scan didn't start.
///
/// The first two variants are single-flight refusals: some other walk owns this
/// volume right now, and nothing is wrong. ❌ Never classify them by
/// [`Display`](std::fmt::Display) output.
#[derive(Debug)]
pub(crate) enum ScanStartError {
    /// A full scan is already running on this volume. The walk the caller asked
    /// for is, for practical purposes, the one already in flight.
    AlreadyScanning,
    /// A search-driven cover walk holds ground on this volume. A truncating
    /// rescan under one blanks the rows it is still inserting, so the scan waits
    /// for the walk instead.
    GroundBeingWalked,
    /// Something below the scan start went wrong (a walker that couldn't spawn, a
    /// volume that unmounted mid-call). Log-only detail; ❌ never branch on the
    /// text.
    Internal(String),
}

impl std::fmt::Display for ScanStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyScanning => f.write_str("a scan is already running on this volume"),
            Self::GroundBeingWalked => f.write_str("a search walk is covering ground on this volume"),
            Self::Internal(diagnostic) => f.write_str(diagnostic),
        }
    }
}

impl From<String> for ScanStartError {
    /// The bridge for internals that report a formatted diagnostic. A cause worth
    /// acting on gets a variant instead.
    fn from(diagnostic: String) -> Self {
        Self::Internal(diagnostic)
    }
}

/// What a manual rescan request did.
///
/// [`Deferred`](RescanOutcome::Deferred) is a promise, not a refusal: the volume
/// remembers the request and the walk that blocked it runs it. A host that shows
/// the user something says so; ❌ nothing may read it as "nothing happened".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RescanOutcome {
    /// The walk is running now (or one already was).
    Started,
    /// A search walk holds ground, so the scan runs when that walk ends.
    Deferred,
}

/// The volumes owed a full walk, one entry each.
static OWED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Remember that this volume is owed a full walk. Idempotent: a second click
/// describes the same work.
pub(in crate::indexing) fn remember(volume_id: &str) {
    let inserted = OWED.lock_ignore_poison().insert(volume_id.to_string());
    if inserted {
        log::info!("Rescan request: '{volume_id}' is owed a scan once the search walk on it ends");
    }
}

/// Take the request, if there is one. Taking is what running it looks like, so a
/// caller that takes and then can't start has to [`remember`] again.
pub(in crate::indexing) fn take(volume_id: &str) -> bool {
    OWED.lock_ignore_poison().remove(volume_id)
}

/// Whether this volume is owed a walk, asked without spending the request.
fn is_owed(volume_id: &str) -> bool {
    OWED.lock_ignore_poison().contains(volume_id)
}

/// Drop what a volume was owed, because it stopped indexing. Called from the one
/// teardown choke point every stop path goes through.
pub(in crate::indexing) fn forget(volume_id: &str) {
    if OWED.lock_ignore_poison().remove(volume_id) {
        log::info!("Rescan request: '{volume_id}' stopped indexing, so the scan it was owed is dropped");
    }
}

/// Run the walk this volume is owed, now that a cover walk has let its ground go.
///
/// ❌ It does NOT decide that the coast is clear: `force_scan` re-asks both
/// single-flight questions at the moment it starts, so a second walk still holding
/// ground refuses this one exactly as the first did — and re-remembers it, which
/// puts the request behind that walk's ending instead. Nothing here can therefore
/// truncate under a live walk, however many are in flight.
///
/// Spawned rather than run inline: the caller is a cover walk's own thread, which
/// is not a runtime worker (so the scan prelude's `block_in_place` has nowhere to
/// run) and has no business blocking on a registry lock and a writer flush on its
/// way out. The peek before the spawn keeps the common case — a walk nobody was
/// waiting on — free.
pub(in crate::indexing) fn run_if_owed(volume_id: &str) {
    if !is_owed(volume_id) {
        return;
    }
    let volume_id = volume_id.to_string();
    crate::indexing::host::runtime::spawn(async move { run_owed_now(&volume_id) });
}

/// The scan itself, run on whatever thread calls this. [`run_if_owed`] is the
/// production door; this is what it does once it's off the walk's thread, and what
/// a test drives when it wants the answer without waiting for a task.
pub(in crate::indexing) fn run_owed_now(volume_id: &str) {
    if !take(volume_id) {
        return;
    }
    // The master switch outranks a request made before it went off. Dropping the
    // request rather than re-remembering it matches what master-off does to every
    // other pending intent: the volumes to resume are recomputed from per-drive
    // intent when it comes back on.
    if !super::master::master_enabled() {
        log::info!("Rescan request: dropping '{volume_id}'s remembered scan, drive indexing is off");
        return;
    }
    match super::state::force_scan(volume_id) {
        Ok(RescanOutcome::Started) => log::info!("Rescan request: '{volume_id}' got the scan it was owed"),
        // `force_scan` put the request back, so the walk that's holding ground now
        // carries it out.
        Ok(RescanOutcome::Deferred) => {
            log::info!("Rescan request: '{volume_id}' is still being walked; its scan waits for that walk");
        }
        Err(e) => log::warn!("Rescan request: '{volume_id}' couldn't take its remembered scan: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One request per volume, and taking it is what spends it.
    #[test]
    fn a_volume_is_owed_at_most_one_scan() {
        remember("owed-one-vol");
        remember("owed-one-vol");
        assert!(take("owed-one-vol"), "the request is there");
        assert!(!take("owed-one-vol"), "and a second click didn't queue a second scan");
    }

    /// A volume that stopped indexing is owed nothing.
    #[test]
    fn a_torn_down_volume_keeps_no_request() {
        remember("owed-teardown-vol");
        forget("owed-teardown-vol");
        assert!(!take("owed-teardown-vol"));
    }

    /// Requests are per volume: one drive's click doesn't rescan another.
    #[test]
    fn two_volumes_are_owed_independently() {
        remember("owed-vol-one");
        assert!(!take("owed-vol-two"));
        assert!(take("owed-vol-one"));
    }
}
