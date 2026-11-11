//! The one terminal report: what a run could and couldn't answer for.
//!
//! Every ending builds its [`SearchRunCoverage`] here, from facts the host
//! gathered ([`CoverageFacts`]), and [`ResultStream::finish`](super::ResultStream::finish)
//! is the only caller. ❌ No separate "fully covered" report: a run the index
//! answered whole is the same report with nothing walked
//! ([`WalkEnding::NothingToWalk`], [`CoverageKind::Covered`]) and empty unreadable
//! lists, so a host that never walks (the browser demo) says exactly what a desktop
//! run over a fully indexed drive says.

use super::state::{CoverageKind, SearchRunCoverage, WalkEnding};

/// Directories nothing is going to walk, split by WHOSE refusal it was: the four
/// are different sentences on screen, and only the first is one the user can act
/// on (`crates/cmdr-index`'s `UnreadableCause`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnreadableGround {
    /// A walk tried and macOS's privacy protection refused: Full Disk Access
    /// opens these.
    pub permission_denied: Vec<String>,
    /// A walk tried and the OS refused for good (POSIX permissions, SIP): no
    /// permission the user can grant Cmdr opens these.
    pub off_limits: Vec<String>,
    /// No walk will read it: a NAS snapshot tree.
    pub declined: Vec<String>,
    /// A walk tried and gave up (a wedged mount, a vanished directory). Cmdr comes
    /// back to it on a backoff, so it's neither the user's to fix nor permanent.
    pub abandoned: Vec<String>,
}

impl UnreadableGround {
    /// One order, no duplicates, however many scopes contributed.
    pub fn settle(&mut self) {
        for list in [
            &mut self.permission_denied,
            &mut self.off_limits,
            &mut self.declined,
            &mut self.abandoned,
        ] {
            list.sort_unstable();
            list.dedup();
        }
    }
}

/// A scope path the index couldn't resolve, and whether this run's walk set out
/// to answer for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedScope {
    /// The path as the caller wrote it.
    pub path: String,
    /// The walk took this scope as one of its frontier roots.
    pub walked: bool,
}

/// What a run knows at its end, before it's said as a [`SearchRunCoverage`].
///
/// Start from [`new`](Self::new) and set what the run found; the empty lists are
/// what a run with nothing unreadable reports.
#[derive(Debug, Clone)]
pub struct CoverageFacts {
    /// How the walk ended, as the run saw it. A cancelled run is relabeled on the
    /// way out by the stream, which is the one place that knows.
    pub walk: WalkEnding,
    /// Which ground the answer came from.
    pub kind: CoverageKind,
    /// The ONE volume the run covered.
    pub target_volume_id: String,
    /// What no walk will read, as the index last answered it.
    pub unreadable: UnreadableGround,
    /// Ground another walk is covering right now.
    pub still_covering: Vec<String>,
    /// THIS run's walk gave up on ground it started.
    pub walk_abandoned_ground: bool,
    /// Frontier roots this run's walk couldn't open while the volume stayed there.
    pub failed_roots: Vec<String>,
    /// Scope paths the index couldn't resolve.
    pub unresolved_scopes: Vec<UnresolvedScope>,
    /// Scope paths on a volume no drive index can serve, neither read nor walked.
    pub uncovered_scopes: Vec<String>,
}

impl CoverageFacts {
    /// A run that ended `walk`-wise over `kind` ground with nothing unreadable,
    /// nothing deferred, and every scope resolved.
    pub fn new(walk: WalkEnding, kind: CoverageKind, target_volume_id: String) -> Self {
        Self {
            walk,
            kind,
            target_volume_id,
            unreadable: UnreadableGround::default(),
            still_covering: Vec::new(),
            walk_abandoned_ground: false,
            failed_roots: Vec::new(),
            unresolved_scopes: Vec::new(),
            uncovered_scopes: Vec::new(),
        }
    }

    /// The report. `capped` and `hidden_by_excludes` are the stream's, the one
    /// place that has seen both halves.
    pub(crate) fn report(self, capped: bool, hidden_by_excludes: u32) -> SearchRunCoverage {
        let walk_completed = self.walk == WalkEnding::Completed;
        SearchRunCoverage {
            // Ground a walk gave up on reaches this two ways and BOTH have to
            // count. The flag is what THIS walk gave up on; `unreadable.abandoned`
            // is what any walk gave up on durably, which is precisely why the
            // frontier stopped offering it and this run never went there. Without
            // the second half a search over a wedged mount would report itself
            // exhaustive.
            abandoned_ground: self.walk_abandoned_ground || !self.unreadable.abandoned.is_empty(),
            // How much of the drive that is, in places rather than folders. Only the
            // durable half has paths to count; a walk that gave up on ground it
            // never recorded leaves this 0, and the note says the honest thing over
            // it.
            abandoned_locations: cmdr_fs::path_locations::location_count(&self.unreadable.abandoned),
            failed_roots: self.failed_roots,
            // A scope the INDEX couldn't resolve isn't a gap once the walk has been
            // to it: the walk is the probe, and it just answered. Only a walk that
            // ran to the end proves it, so anything short leaves the signal
            // standing. Without this, a folder too new to be indexed would show
            // "Cmdr doesn't cover this folder" over a complete list of its files.
            unresolved_scopes: self
                .unresolved_scopes
                .into_iter()
                .filter(|scope| !(walk_completed && scope.walked))
                .map(|scope| scope.path)
                .collect(),
            uncovered_scopes: self.uncovered_scopes,
            walk: self.walk,
            kind: self.kind,
            permission_denied: self.unreadable.permission_denied,
            off_limits: self.unreadable.off_limits,
            declined: self.unreadable.declined,
            still_covering: self.still_covering,
            capped,
            target_volume_id: self.target_volume_id,
            hidden_by_excludes,
        }
    }
}
