//! What a request to (re)scan a volume answers, and how the walk a volume is
//! waiting for finally runs.
//!
//! A scan start has three doors it can be turned away at, and all three are
//! single-flight questions rather than failures (`../DETAILS.md` § "The two
//! single-flight questions a scan has to ask"). [`ScanStartError`] is how a caller
//! tells them apart without reading a sentence: the wording of a diagnostic is for
//! logs, and classifying control flow by it breaks the moment someone edits it.
//!
//! ## Why a volume waits for one walk
//!
//! A cover walk holds ground for seconds to minutes and a full scan for longer,
//! and the person who clicked "Rescan now" has no way to see when either lets go.
//! Telling them to click again puts the scheduling on the one participant who
//! can't observe the schedule, so the request is remembered and run by whoever was
//! in the way, on their way out.
//!
//! The request itself lives in the claim table (`cover/live.rs`), beside the
//! holders it is waiting for, so "may it start" is one question rather than two
//! that can disagree. What lives here is what the request MEANS and what running
//! it does.

/// Why a (re)scan didn't start.
///
/// The first three variants are single-flight refusals: something else owns this
/// volume right now, and nothing is wrong. ❌ Never classify them by
/// [`Display`](std::fmt::Display) output.
#[derive(Debug)]
pub(crate) enum ScanStartError {
    /// The first-index machine still owes this volume work, so it is being walked
    /// whole, in pieces. The walk the caller asked for is, for practical purposes,
    /// the one already in flight — and there is nothing to wait for, because the
    /// machine composes with everything else on the drive rather than blocking it.
    AlreadyScanning,
    /// Another whole-volume run holds the drive: a full scan, or a journal replay
    /// writing every buffered change. It blanks or rewrites ground the caller's
    /// walk would truncate, and it ENDS, so a manual request waits for it.
    GroundBeingRewritten,
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
            Self::AlreadyScanning => f.write_str("this volume's first index is still being built"),
            Self::GroundBeingRewritten => f.write_str("a whole-volume run already owns this volume"),
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
/// The two deferrals are promises, not refusals: the volume remembers the request
/// and whoever was in the way runs it. A host that shows the user something says
/// so; ❌ nothing may read either as "nothing happened". They stay apart because
/// they answer the user's obvious next question differently — one drive is being
/// searched, the other is already being indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RescanOutcome {
    /// The walk is running now (or one already was).
    Started,
    /// A search walk holds ground, so the scan runs when that walk ends.
    DeferredUntilSearchEnds,
    /// A full scan (or a journal replay) owns the volume, so the scan runs when
    /// that one ends.
    DeferredUntilScanEnds,
}

/// Run the walk this volume is waiting for, now that whoever held its ground has
/// let go.
///
/// ❌ It does NOT decide that the coast is clear: `force_scan` re-asks every
/// single-flight question at the moment it starts, so another holder still on the
/// volume refuses this one exactly as the first did — and re-remembers it, which
/// puts the request behind THAT holder's ending. Nothing here can therefore
/// truncate under a live walk, however many are in flight.
///
/// Spawned rather than run inline: the caller can be a cover walk's own thread,
/// which is not a runtime worker (so the scan prelude's `block_in_place` has
/// nowhere to run) and has no business blocking on a registry lock and a writer
/// flush on its way out. The peek before the spawn keeps the common case — ground
/// nobody was waiting on — free, and it is what makes this safe to call from
/// anywhere: a volume still held answers no.
///
/// ⚠️ **Every whole-volume holder calls this where it stops WRITING**, which is
/// not always where it hands the ground back: a scan's completion task keeps
/// reconciling and stamps `scan_completed_at` after the walk thread is joined, and
/// a truncating rescan landing in that window would stamp the old scan's marker
/// onto the new scan's partial index. Guarded by
/// `tests::every_whole_volume_holder_runs_the_rescan_it_owes`.
pub(in crate::indexing) fn run_if_owed(volume_id: &str) {
    if !super::cover::a_rescan_can_start(volume_id) {
        return;
    }
    let volume_id = volume_id.to_string();
    crate::indexing::host::runtime::spawn(async move { run_owed_now(&volume_id) });
}

/// The scan itself, run on whatever thread calls this. [`run_if_owed`] is the
/// production door; this is what it does once it's off the holder's thread, and
/// what a test drives when it wants the answer without waiting for a task.
pub(in crate::indexing) fn run_owed_now(volume_id: &str) {
    if !super::cover::take_rescan(volume_id) {
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
        Ok(RescanOutcome::Started) => log::info!("Rescan request: '{volume_id}' got the scan it was waiting for"),
        // `force_scan` put the request back, so whoever holds the ground now
        // carries it out.
        Ok(RescanOutcome::DeferredUntilSearchEnds) => {
            log::info!("Rescan request: '{volume_id}' is still being walked; its scan waits for that walk");
        }
        Ok(RescanOutcome::DeferredUntilScanEnds) => {
            log::info!("Rescan request: '{volume_id}' is still being rebuilt; its scan waits for that run");
        }
        Err(e) => log::warn!("Rescan request: '{volume_id}' couldn't take its remembered scan: {e}"),
    }
}

#[cfg(test)]
mod tests {
    /// Every whole-volume holder has to run the walk its volume is waiting for,
    /// and there is no type that can make it: the claim frees the ground on drop,
    /// which is what keeps a panicking run from wedging a drive forever, and a
    /// destructor is exactly where a scan must not be started from.
    ///
    /// So the rule is checked where it lives — in the source. A holder that hands
    /// the ground back and never asks who was waiting leaves a promise nobody
    /// keeps, and the user's "Rescan now" simply never happens.
    #[test]
    fn every_whole_volume_holder_runs_the_rescan_it_owes() {
        let sources = crate::indexing::source_guard::indexing_sources();

        // The spelling all three whole-volume holders share: the claim rides into
        // the task that ends the run and is handed back by name there. Assembled
        // rather than written out, so this file doesn't match its own marker.
        let hands_the_ground_back = concat!("drop(", "ground)");

        let mut holders: Vec<String> = Vec::new();
        for (name, path) in sources {
            let src = std::fs::read_to_string(&path).expect("read source");
            if !src.contains(hands_the_ground_back) {
                continue;
            }
            assert!(
                src.contains("run_if_owed("),
                // allowed-pluralize-noun: `{name}` is a file name, and `hands` is its verb.
                "{name} hands a whole volume's ground back but never runs the rescan that volume \
                 may be waiting for, so a queued \"Rescan now\" would wait there forever"
            );
            holders.push(name);
        }
        holders.sort();
        assert_eq!(
            holders,
            vec![
                "lifecycle/network_scan.rs".to_string(),
                "lifecycle/scan_completion.rs".to_string(),
                "watch/event_loop/replay.rs".to_string(),
            ],
            "the set of whole-volume holders changed; give the new one its `run_if_owed` where it \
             stops WRITING, then update this list"
        );
    }
}
