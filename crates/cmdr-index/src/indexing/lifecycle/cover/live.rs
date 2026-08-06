//! One walk per patch of ground.
//!
//! Two searches over the same volume routinely want overlapping frontiers: a
//! refined query re-asks `coverage` while the first query's walk is still
//! running, and Decision 11 keeps that first walk alive. Letting both walk the
//! same directories is a data-safety bug, not a performance one — the two walks
//! allocate different ids for the same names, `insert_entries_v2_batch` is
//! `INSERT OR IGNORE` against `UNIQUE (parent_id, name_folded)`, and whichever
//! row loses takes its whole subtree with it (`scanner/DETAILS.md` § "Three scan
//! roots"). It is the same hazard `lifecycle/state.rs` names for two writers on
//! one database, one level down.
//!
//! So a walk CLAIMS its frontier roots, and a later walk over ground someone
//! already claimed simply doesn't take it. The second search loses nothing
//! durable: the first walk's rows land in the same index, and Decision 12 makes
//! them visible to the very next query, which is exactly how Decision 11 already
//! says a superseded query recovers the ground its predecessor covered — from the
//! index, never from a replay.
//!
//! ❌ Don't reach for a shared-subscriber fan-out instead. It would give the
//! second search live batches for the shared ground, but it needs per-subscriber
//! filtering and per-subscriber completion (root X is done while the walk moves
//! on to Y and Z), and there is no second consumer today to shape either against.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::indexing::paths::path_prefix::is_strict_descendant;
use cmdr_fs::ignore_poison::IgnorePoison;

/// The frontier roots being walked right now, per volume id. A volume with no
/// live walk holds no entry.
static IN_FLIGHT: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();

fn in_flight() -> &'static Mutex<HashMap<String, Vec<String>>> {
    IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One walk's hold on the frontier roots it may cover, released when it ends.
///
/// The walk thread owns it for its whole life, so the roots free up on the
/// completion path, the cancel path, and a panic alike.
pub(super) struct Claim {
    volume_id: String,
    /// The roots this walk took.
    mine: Vec<String>,
    /// The roots it didn't, because another walk on this volume is covering
    /// them.
    deferred: Vec<String>,
}

impl Claim {
    /// Split `frontier` into the roots this walk may take and the roots another
    /// walk already owns.
    ///
    /// A requested root overlaps a claimed one in EITHER direction — a
    /// descendant of a live root is already being walked, and an ancestor of one
    /// would walk straight through it. The second case shouldn't arise from a
    /// coverage answer (a frontier node's ancestors are listed, or they'd be the
    /// frontier instead), which is exactly why it's handled rather than assumed.
    /// The same test deduplicates a frontier that overlaps itself.
    pub(super) fn take(volume_id: &str, frontier: Vec<String>) -> Self {
        let mut live = in_flight().lock_ignore_poison();
        let claimed = live.entry(volume_id.to_string()).or_default();

        let mut mine = Vec::with_capacity(frontier.len());
        let mut deferred = Vec::new();
        for root in frontier {
            if claimed.iter().any(|held| overlaps(held, &root)) {
                deferred.push(root);
            } else {
                claimed.push(root.clone());
                mine.push(root);
            }
        }

        if claimed.is_empty() {
            live.remove(volume_id);
        }
        if !deferred.is_empty() {
            log::debug!(
                "Cover: leaving {} frontier root(s) on '{volume_id}' to the walk already covering them",
                deferred.len()
            );
        }
        Self {
            volume_id: volume_id.to_string(),
            mine,
            deferred,
        }
    }

    /// The roots this walk is covering.
    pub(super) fn mine(&self) -> &[String] {
        &self.mine
    }

    /// The roots it left to another walk.
    pub(super) fn deferred(&self) -> &[String] {
        &self.deferred
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.mine.is_empty() {
            return;
        }
        let mut live = in_flight().lock_ignore_poison();
        if let Some(claimed) = live.get_mut(&self.volume_id) {
            claimed.retain(|held| !self.mine.contains(held));
            if claimed.is_empty() {
                live.remove(&self.volume_id);
            }
        }
    }
}

/// Which of `frontier`'s roots a walk on this volume is covering RIGHT NOW.
///
/// The same overlap rule [`Claim::take`] would apply, asked without taking
/// anything — so a caller can find out that the ground it wants is spoken for
/// before it commits to a walk that would take none of it. ❌ Not a reservation
/// and not a promise: the answer can go stale the moment it's read, which is why
/// `Claim::take` stays the authority and reports what it left behind.
pub(in crate::indexing) fn ground_being_walked(volume_id: &str, frontier: &[String]) -> Vec<String> {
    let live = in_flight().lock_ignore_poison();
    let Some(claimed) = live.get(volume_id) else {
        return Vec::new();
    };
    frontier
        .iter()
        .filter(|root| claimed.iter().any(|held| overlaps(held, root)))
        .cloned()
        .collect()
}

/// Whether walking one of these two roots would cover any of the other's ground.
fn overlaps(a: &str, b: &str) -> bool {
    a == b || is_strict_descendant(a, b) || is_strict_descendant(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case Decision 11 creates: a refined query asks for ground the first
    /// query's walk is still covering. The second walk takes none of it, and says
    /// which roots it left behind.
    #[test]
    fn a_root_another_walk_is_covering_is_left_to_it() {
        let first = Claim::take("overlap-vol", vec!["/a".to_string(), "/b".to_string()]);
        assert_eq!(first.mine(), ["/a", "/b"]);
        assert!(first.deferred().is_empty());

        let second = Claim::take(
            "overlap-vol",
            vec![
                "/a".to_string(),      // the same root
                "/b/deep".to_string(), // inside a claimed root
                "/c".to_string(),      // nobody's
                "/".to_string(),       // an ancestor of both claimed roots
                "/bc".to_string(),     // NOT inside `/b`, component-aware
            ],
        );
        assert_eq!(second.mine(), ["/c", "/bc"]);
        assert_eq!(second.deferred(), ["/a", "/b/deep", "/"]);
    }

    /// Asking who holds ground answers by the same overlap rule, and takes
    /// nothing — which is what lets a search find out that walking would get it
    /// nothing BEFORE it commits to a walk.
    #[test]
    fn ground_a_walk_holds_can_be_asked_about_without_taking_it() {
        assert!(
            ground_being_walked("ask-vol", &["/a".to_string()]).is_empty(),
            "nobody is walking a volume with no walk on it"
        );

        let held = Claim::take("ask-vol", vec!["/a".to_string()]);
        assert_eq!(
            ground_being_walked("ask-vol", &["/a/inner".to_string(), "/b".to_string()]),
            ["/a/inner"],
            "a descendant of a claimed root is being walked; a sibling isn't"
        );

        drop(held);
        assert!(
            ground_being_walked("ask-vol", &["/a".to_string()]).is_empty(),
            "and the answer follows the walk out"
        );
    }

    /// Claims are per volume: the same path on two drives is two different
    /// places.
    #[test]
    fn two_volumes_claim_independently() {
        let _first = Claim::take("volume-one", vec!["/shared".to_string()]);
        let second = Claim::take("volume-two", vec!["/shared".to_string()]);

        assert_eq!(second.mine(), ["/shared"], "a different drive, a different folder");
    }

    /// A frontier that overlaps ITSELF is deduplicated by the same rule, so one
    /// walk can't double-write its own ground either.
    #[test]
    fn a_frontier_that_overlaps_itself_is_deduplicated() {
        let claim = Claim::take("self-overlap-vol", vec!["/a".to_string(), "/a/inner".to_string()]);

        assert_eq!(claim.mine(), ["/a"]);
        assert_eq!(claim.deferred(), ["/a/inner"]);
    }

    /// The ground frees up when the walk ends, so the next search over it walks
    /// rather than deferring forever.
    #[test]
    fn ground_is_released_when_its_walk_ends() {
        drop(Claim::take("release-vol", vec!["/a".to_string()]));

        let next = Claim::take("release-vol", vec!["/a".to_string()]);
        assert_eq!(next.mine(), ["/a"]);
        drop(next);

        assert!(
            !in_flight().lock_ignore_poison().contains_key("release-vol"),
            "and the volume's entry goes with it, rather than growing a map forever"
        );
    }

    /// Releasing one walk's roots leaves another walk's alone, even where they
    /// were taken in the same order.
    #[test]
    fn releasing_one_walk_leaves_the_others_claims_standing() {
        let keeper = Claim::take("mixed-vol", vec!["/keep".to_string()]);
        drop(Claim::take("mixed-vol", vec!["/go".to_string()]));

        let next = Claim::take("mixed-vol", vec!["/keep".to_string(), "/go".to_string()]);
        assert_eq!(next.mine(), ["/go"], "only the released root is free");
        assert_eq!(next.deferred(), ["/keep"]);
        drop(keeper);
    }
}
