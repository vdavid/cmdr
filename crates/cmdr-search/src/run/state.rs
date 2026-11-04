//! The run-state data a live search reports: what it's doing, how its walk ended,
//! which ground it drew from, and what it couldn't answer for. Plain specta DTOs:
//! the host wraps them in its own event envelopes.

use serde::{Deserialize, Serialize};

/// Which part of a live search produced an event.
///
/// Four honest waits rather than one spinner: resolving coverage can mean a
/// multi-second arena load, being queued behind another walk is somebody else's
/// clock entirely, reading the index is fast, and the walk is unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SearchPhase {
    /// Asking the index what it can't answer for yet, and loading its arena.
    ResolvingCoverage,
    /// Coverage IS resolved, and every bit of uncovered ground this run needs is
    /// already held by a walk in flight. One walk per patch of ground
    /// (`lifecycle/cover/live.rs`), so all this run can do is wait for that one and
    /// read what it wrote. ❌ Not [`Walking`](Self::Walking): no folder of this
    /// run's is being read, and saying otherwise puts "0 folders scanned" under a
    /// sentence about walking.
    WaitingForAnotherWalk,
    /// Scanning the arena: the half the index already covers.
    ReadingIndex,
    /// Walking the ground the index doesn't cover, live.
    Walking,
}

/// How a live search's walk ended. Typed, because three of the four leave the
/// result list INCOMPLETE and the copy differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WalkEnding {
    /// The index already covered the whole scope, so nothing had to be walked.
    NothingToWalk,
    /// Every frontier root this run took was covered end to end.
    Completed,
    /// It stopped before covering its frontier: the drive went away mid-walk, a
    /// root couldn't be read, or the volume couldn't be walked at all. Whatever
    /// it did read is in the index; the rest is still frontier and the next
    /// search asks for it again.
    Interrupted,
    /// Somebody stopped it (Escape, or the dialog closing).
    Cancelled,
}

/// What ground a run's answer was drawn from: the index, a live walk, or both.
///
/// Derived from the coverage question rather than from how the walk ended, so it
/// says what the search HAD to do, not how far it got — a cancelled run over
/// half-covered ground is still `Mixed`, with [`WalkEnding`] saying the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CoverageKind {
    /// The index covered the whole scope. Nothing had to be walked.
    Covered,
    /// Nothing in the scope was covered, so every bit of the answer was walked
    /// live. The cold-drive case this effort exists for.
    Live,
    /// Part of the scope came from the index and part from the walk.
    Mixed,
}

/// What a live run could NOT answer for, gathered in one place so a terminal
/// event says it once.
///
/// `Run` in the name because the operation log has a `SearchCoverage` of its own
/// (how much of a copy's source tree a journal search covered), and two types of
/// one name can't both cross specta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchRunCoverage {
    /// How the walk ended. Anything but [`WalkEnding::Completed`] /
    /// [`WalkEnding::NothingToWalk`] means the list is a lower bound.
    pub walk: WalkEnding,
    /// Which ground this run's answer came from. The measure of how often a
    /// search still needs to walk at all, and the one field here that describes
    /// the QUESTION rather than what came back from it.
    pub kind: CoverageKind,
    /// Directories a walk tried to read and was REFUSED, as absolute paths.
    ///
    /// The half a user can act on: on macOS this is usually Full Disk Access, and
    /// granting it heals the mark on the next search (the successful listing
    /// clears it). ❌ Keep it apart from [`declined`](Self::declined) — offering
    /// Full Disk Access over a snapshot folder is advice that does nothing.
    pub permission_denied: Vec<String>,
    /// Directories no walk will read at all, by Cmdr's own choice: a NAS snapshot
    /// tree, whose per-snapshot hardlinked copies the scanner refuses on purpose
    /// (44 TB reported on a 10 TB volume).
    ///
    /// Nothing for the user to fix, so the copy explains rather than offers.
    pub declined: Vec<String>,
    /// Ground another walk on this volume is covering right now, so this run left
    /// it alone. Its rows reach the same index, so this is "these arrive a bit
    /// later", never "these are lost".
    pub still_covering: Vec<String>,
    /// Scope paths that routed to this volume but aren't in its index and
    /// couldn't be walked either. The typed "Cmdr can't speak for this folder"
    /// signal; ❌ never worded as "that folder doesn't exist".
    pub unresolved_scopes: Vec<String>,
    /// Scope paths on a volume no drive index can serve (an SFTP or WebDAV
    /// server), so this run neither read an index for them nor walked them. The
    /// live twin of `SearchResult::uncovered_scopes`, and the only thing on the
    /// wire that says why a server pane's search came back empty. ❌ Never
    /// "fixed" by walking: a walk stands an index up for the volume it walks.
    #[serde(default)]
    pub uncovered_scopes: Vec<String>,
    /// Whether ground was given up on: a directory that stopped responding, one
    /// that failed with an errno the walk can't act on, or a subtree pruned by the
    /// walker's consecutive-failure budget. Covers both what THIS run's walk gave
    /// up on and what an earlier walk recorded as `UnreadableCause::Abandoned` (the
    /// reason the frontier didn't offer it to this run at all).
    ///
    /// TRUE means the list is a lower bound even when [`walk`](Self::walk) is
    /// [`WalkEnding::Completed`] — the third way a run can be short, alongside
    /// cancel and disconnect (Accepted difference 9), and the quiet one: nothing
    /// else on the wire hints at it. ❌ Don't fold it into `walk`: Cmdr retries
    /// this ground on a backoff, which is a different sentence from "the drive went
    /// away".
    pub abandoned_ground: bool,
    /// How many PLACES the walk gave up on, for a note that can say how much of
    /// the drive this is about instead of leaving the reader to imagine it.
    ///
    /// Folders grouped by their parent, ❌ never the raw folder count: a wedged
    /// mount marked 1,497 directories on one real machine, which
    /// `coverage_for_scope` already cuts to 76 shallowest ancestors, and grouping
    /// those lands on the one place the user would recognize. Reporting 1,497
    /// would be true and useless.
    ///
    /// `0` with [`abandoned_ground`](Self::abandoned_ground) true is a real state
    /// and the note handles it: this run's own walk gave up on ground it never
    /// recorded a path for, so something was missed and nothing can say where.
    #[serde(default)]
    pub abandoned_locations: u32,
    /// Folders this run's walk set out to read and couldn't open at all, as
    /// absolute paths, while the drive itself stayed there: the server didn't
    /// have that name, the folder went away since the index was asked, or the
    /// listing failed outright. Nothing inside them was read, so for these the
    /// list says nothing, ❌ never "no matches".
    ///
    /// A scope listed here is NOT also in
    /// [`unresolved_scopes`](Self::unresolved_scopes): that one says "press
    /// Enter and Cmdr will look", and Cmdr just did. ❌ Not a disconnect either,
    /// which is [`WalkEnding::Interrupted`]. The next search tries them again.
    #[serde(default)]
    pub failed_roots: Vec<String>,
    /// Whether the result cap was reached. The walk carries on past it (the count
    /// keeps rising), only the rows stop.
    pub capped: bool,
    /// The ONE volume this run covered, as routing resolved it.
    pub target_volume_id: String,
    /// How many matches an exclusion rule kept out of the count: the
    /// system/build/cache tier (on unless the query turns it off) plus any `!`
    /// excludes in the scope.
    ///
    /// Coverage, not a statistic: the defaults are right for "find my invoice"
    /// and exactly wrong for "where is my disk space going", where the caches ARE
    /// the answer. A caller that can't see this number reads a filtered count as
    /// the whole truth. Counted across BOTH halves — the arena scan and the walk
    /// — so a live run doesn't under-report it.
    #[serde(default)]
    pub hidden_by_excludes: u32,
}

/// Why a run couldn't answer at all. Typed for the branch, with the sentence
/// alongside for display (the bare-message contract search results already have).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SearchRunError {
    /// The query can't be run: an invalid pattern, or one that narrows nothing
    /// while ground still needs walking (a walk over an unknown filesystem can't
    /// afford "show me everything").
    Query,
    /// The volume's index file exists but won't open or read. Distinct from
    /// "never indexed", which is not an error — that volume gets walked.
    IndexUnreadable,
}
