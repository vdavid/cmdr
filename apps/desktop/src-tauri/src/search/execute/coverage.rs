//! What the index can't answer for, and the arena that answer may be honored
//! against.
//!
//! A live run asks this question first and builds everything else on the answer:
//! the frontier is the ground the walk reads, the covered half is what's left,
//! and the tokens decide whether the arena is allowed to serve it (Decision 12).
//! The model lives apart from the run that consumes it (`live_run.rs`) because
//! it's the half that's decided before anything is emitted, and it answers on
//! its own terms: `coverage_kind` and `every_frontier_root_is_another_walks` are
//! pure, and the two index reads say nothing about how a run reports.

use crate::index_host::index;
use cmdr_index::{CoverageDimension, CoverageToken};

use super::Target;
use crate::search::live::CoverageKind;
use crate::search::query;
use crate::search::volumes::{self, VolumeLoad};

/// Directories nothing is going to walk, split by WHOSE refusal it was: the three
/// are different sentences on screen, and only the first is one the user can act
/// on (`crates/cmdr-index`'s `UnreadableCause`).
#[derive(Default)]
pub(super) struct UnreadableGround {
    /// A walk tried and the OS refused.
    pub(super) permission_denied: Vec<String>,
    /// No walk will read it: a NAS snapshot tree.
    pub(super) declined: Vec<String>,
    /// A walk tried and gave up (a wedged mount, a vanished directory). Cmdr comes
    /// back to it on a backoff, so it's neither the user's to fix nor permanent.
    pub(super) abandoned: Vec<String>,
}

impl UnreadableGround {
    /// Fold one scope's answer in.
    fn extend(&mut self, map: &cmdr_index::CoverageMap) {
        self.permission_denied.extend(map.permission_denied.iter().cloned());
        self.declined.extend(map.declined.iter().cloned());
        self.abandoned.extend(map.abandoned.iter().cloned());
    }

    /// One order, no duplicates, however many scopes contributed.
    fn settle(&mut self) {
        for list in [
            &mut self.permission_denied,
            &mut self.declined,
            &mut self.abandoned,
        ] {
            list.sort_unstable();
            list.dedup();
        }
    }
}

/// A coverage answer over a query's scopes, merged.
pub(super) struct CoverageQuestion {
    /// Every frontier root, across every scope path.
    pub(super) frontier: Vec<String>,
    /// Every directory nothing will walk, across every scope path.
    pub(super) unreadable: UnreadableGround,
    /// The token each answer carried. All of them have to match the arena's for
    /// the covered half to be trustworthy (Decision 12).
    pub(super) tokens: Vec<CoverageToken>,
    /// The frontier roots another walk is covering as this was read. This run
    /// can't have them: one walk per patch of ground, or the two orphan each
    /// other's subtrees.
    being_walked: Vec<String>,
}

/// Ask the index what it can't answer for, over every scope path in turn.
pub(super) fn coverage_of(volume_id: &str, scopes: &[String]) -> CoverageQuestion {
    let mut question = CoverageQuestion {
        frontier: Vec::new(),
        unreadable: UnreadableGround::default(),
        tokens: Vec::new(),
        being_walked: Vec::new(),
    };
    for scope in scopes {
        match index().coverage(volume_id, scope, CoverageDimension::Listing) {
            Ok(map) => {
                question.unreadable.extend(&map);
                question.frontier.extend(map.frontier);
                question.being_walked.extend(map.being_walked);
                question.tokens.push(map.token);
            }
            Err(e) => {
                // An index that can't say what it covers can't be trusted to have
                // covered anything, so the scope goes to the walk whole — the same
                // conservative answer the coverage query gives itself when the
                // exclusion policy stamp doesn't match.
                log::warn!("Live search: no coverage answer for '{scope}': {e}");
                question.frontier.push(scope.clone());
            }
        }
    }
    question.frontier.sort_unstable();
    question.frontier.dedup();
    question.being_walked.sort_unstable();
    question.being_walked.dedup();
    question.unreadable.settle();
    question
}

/// The scope paths to ask about: the query's own include paths, canonicalized the
/// same way the index-side resolution canonicalizes them (a symlinked `/tmp` and
/// the index's `/private/tmp` have to be the same folder), or the whole volume
/// when the query has no scope.
pub(super) fn coverage_scopes(target: &Target) -> Vec<String> {
    if target.include_paths.is_empty() {
        return vec![volumes::registry_mount_root(&target.volume_id).unwrap_or_else(|| "/".to_string())];
    }
    target
        .include_paths
        .iter()
        .map(|path| query::canonicalize_scope_path(path))
        .collect()
}

/// Which ground a run's answer is drawn from, decided by the coverage question
/// and nothing downstream of it.
///
/// A scope root that is itself a frontier root was covered by NOTHING, so a run
/// where that holds for every scope answers entirely off the walk. One where it
/// holds for none of them still had ground to walk somewhere below, which is the
/// mixed case. Pure, because it's the measure of how often a search still needs
/// to walk at all and that number is worth being able to test.
pub(super) fn coverage_kind(frontier: &[String], scopes: &[String]) -> CoverageKind {
    if frontier.is_empty() {
        return CoverageKind::Covered;
    }
    if scopes.iter().all(|scope| frontier.contains(scope)) {
        return CoverageKind::Live;
    }
    CoverageKind::Mixed
}

/// Whether there IS uncovered ground and every bit of it belongs to a walk
/// already running — the cheap question the wait loop re-asks.
pub(super) fn every_frontier_root_is_another_walks(question: &CoverageQuestion) -> bool {
    !question.frontier.is_empty()
        && question
            .frontier
            .iter()
            .all(|root| question.being_walked.contains(root))
}

/// Whether this groundwork is being redone after watching somebody else's walk
/// end, which is its own reason to trust nothing the arena holds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AfterAnotherWalk {
    No,
    Yes,
}

/// The arena a coverage answer may be honored against (Decision 12).
///
/// A coverage answer that calls a subtree covered is a promise the arena holds
/// its rows. A walk that wrote rows behind the arena breaks that promise, and the
/// symptom is silent: the same query, run again, prunes the ground it just walked
/// and returns FEWER results than the first time.
///
/// So: reload when the tokens disagree AND a walk is what put them out of step.
/// Both halves earn their keep. Without the token, every query after any walk
/// would pay a full arena rebuild. Without the walk mark, a boot disk — whose
/// background indexer moves the token several times a second — would rebuild in
/// front of nearly every search, which is the regression `volumes::get_loaded`
/// documents removing once already. What's left uncovered is ordinary index lag,
/// which search has always had.
pub(super) fn arena_for_coverage(volume_id: &str, tokens: &[CoverageToken], after: AfterAnotherWalk) -> VolumeLoad {
    let load = volumes::ensure_volume(volume_id);
    let VolumeLoad::Loaded(ref loaded) = load else {
        return load;
    };
    if tokens.iter().all(|token| *token == loaded.coverage_token) {
        // Exactly the rows the answer was computed against.
        volumes::take_walked_behind(volume_id);
        return load;
    }
    // A run that WATCHED another walk end doesn't need the mark to know a walk
    // wrote rows: it waited for that walk, and its own reason for waiting was
    // that the rows would be there afterwards. The mark is a global one-shot, so
    // whoever else consumed it must not cost this run the reload.
    if after == AfterAnotherWalk::No && !volumes::take_walked_behind(volume_id) {
        return load;
    }
    // Loaded strictly after the coverage answer was taken, so it holds every row
    // that answer calls covered, whatever else landed meanwhile.
    log::debug!("Live search: reloading '{volume_id}'s arena, a walk wrote rows behind it");
    volumes::reload_volume(volume_id)
}
