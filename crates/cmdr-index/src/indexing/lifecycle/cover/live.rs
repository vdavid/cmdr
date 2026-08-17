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
//! So a holder CLAIMS the ground it is about to write, and a later one over
//! ground someone already claimed simply doesn't take it. A truncating scan and a
//! journal replay claim the volume WHOLE (they blank it, or write anywhere on
//! it); a cover walk claims the frontier roots it names. That one table is the
//! single-flight answer both scan entries read: a refusal names the KIND of
//! holder in the way, which is the whole difference between "the walk you asked
//! for is already running" and "wait for the walk holding this ground".
//!
//! A deferred search loses nothing durable: the first walk's rows land in the
//! same index, and Decision 12 makes them visible to the very next query, which
//! is exactly how Decision 11 already says a superseded query recovers the ground
//! its predecessor covered — from the index, never from a replay.
//!
//! ❌ Don't reach for a shared-subscriber fan-out instead. It would give the
//! second search live batches for the shared ground, but it needs per-subscriber
//! filtering and per-subscriber completion (root X is done while the walk moves
//! on to Y and Z), and there is no second consumer today to shape either against.
//!
//! ## What the table is, and why
//!
//! Claims are held path-keyed in a `BTreeMap` per volume, so "does anyone hold
//! ground overlapping this root" is two range questions rather than a scan of
//! everything held: the ancestor chain is a handful of lookups whatever the table
//! holds, and the descendants come out of one sorted range that costs what it
//! yields. ❌ Never a `Vec` scan — a frontier is checked root by root against the
//! roots already taken, so a linear membership test makes ONE `take` quadratic in
//! its own width, and a cold-drive search really does arrive with thousands of
//! roots (`docs/notes/claim-table-cost-2026-08-17.md`).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

use crate::indexing::paths::path_prefix::{descendant_range_prefix, self_and_ancestors};
use cmdr_fs::ignore_poison::IgnorePoison;

/// The ground held right now, per volume id. A volume nobody is writing holds no
/// entry.
static IN_FLIGHT: OnceLock<Mutex<HashMap<String, VolumeClaims>>> = OnceLock::new();

fn in_flight() -> &'static Mutex<HashMap<String, VolumeClaims>> {
    IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How much of a volume a claim speaks for.
///
/// ⚠️ The two modes are the whole arbitration vocabulary, and deliberately so:
/// ❌ never re-entrancy or holder identity. A refusal says what KIND of holder is
/// in the way, which is all a caller needs to decide what to tell the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum Mode {
    /// The whole volume, whatever ground anyone else names. A truncating scan
    /// blanks the database and bumps the epoch, so every concurrent walk is
    /// writing into a table about to disappear — overlapping ground or not.
    Exclusive,
    /// Only the ground it names. Two of these compose as long as their frontiers
    /// stay off each other, which is what lets a search-driven walk run beside
    /// the phase machine covering the same volume in pieces (Decision 13).
    Additive,
}

/// The claims held on one volume, keyed by path.
#[derive(Default)]
struct VolumeClaims {
    /// Every root somebody holds, and how much of the volume that holder speaks
    /// for. Keys are unique across holders: a root is granted only when nothing
    /// overlapping it is held, and a path overlaps itself.
    roots: BTreeMap<String, Mode>,
    /// How many of `roots` are [`Mode::Exclusive`], so "is this whole volume
    /// spoken for" is a counter read rather than a scan.
    exclusive: usize,
}

/// Which holders an overlap question is asked about. Every mode passes unless a
/// caller narrows it.
fn any_holder(_: Mode) -> bool {
    true
}

/// Only the holders that are WALKING the ground they name. What a caller asking
/// "is somebody covering this" means, and what an `Exclusive` holder — which
/// speaks for a whole volume it may not be walking a step of — must not answer.
fn walking_holder(mode: Mode) -> bool {
    mode == Mode::Additive
}

impl VolumeClaims {
    /// Whether any held root would cover `root`'s ground, in either direction.
    ///
    /// A requested root overlaps a held one in EITHER direction — a descendant of
    /// a live root is already being walked, and an ancestor of one would walk
    /// straight through it. The second case shouldn't arise from a coverage
    /// answer (a frontier node's ancestors are listed, or they'd be the frontier
    /// instead), which is exactly why it's handled rather than assumed.
    fn overlapping(&self, root: &str) -> bool {
        self.overlapping_holder(root, any_holder)
    }

    /// The same question asked of some of the holders: whichever `keep` accepts.
    fn overlapping_holder(&self, root: &str, keep: fn(Mode) -> bool) -> bool {
        self_and_ancestors(root).any(|candidate| self.roots.get(candidate).is_some_and(|mode| keep(*mode)))
            || self.holds_under(root, keep)
    }

    /// Whether anything strictly under `root` is held by a holder `keep` accepts.
    /// One sorted range, stopped at the first key outside the prefix.
    fn holds_under(&self, root: &str, keep: fn(Mode) -> bool) -> bool {
        let prefix = descendant_range_prefix(root);
        // ⚠️ The root is its OWN range prefix, so without the length test a claim
        // at the volume root reads as holding ground under itself. Every strict
        // descendant's key is longer than the path it sits under.
        let self_len = root.len();
        self.roots
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .any(|(key, mode)| key.len() > self_len && keep(*mode))
    }

    fn insert(&mut self, root: String, mode: Mode) {
        if mode == Mode::Exclusive {
            self.exclusive += 1;
        }
        self.roots.insert(root, mode);
    }

    fn remove(&mut self, root: &str) {
        if self.roots.remove(root) == Some(Mode::Exclusive) {
            self.exclusive -= 1;
        }
    }
}

/// One holder's grip on the ground it may write, released when it ends.
///
/// Whoever holds it owns it for the whole of their work, so the roots free up on
/// the completion path, the cancel path, and a panic alike. A cover walk's lives
/// on its own thread; a scan's and a replay's travel into the task that ends
/// them, since both outlive the call that started them.
pub(in crate::indexing) struct Claim {
    volume_id: String,
    /// The roots this holder took.
    mine: Vec<String>,
    /// The roots it didn't, because another holder on this volume has them.
    deferred: Vec<String>,
    /// What kind of holder was in the way, for a claim that took NOTHING. See
    /// [`Claim::refused_by`].
    refused_by: Option<Mode>,
}

impl Claim {
    /// Split `frontier` into the roots this walk may take and the roots another
    /// holder already owns.
    ///
    /// The overlap rule is [`VolumeClaims::overlapping`], and it deduplicates a
    /// frontier that overlaps ITSELF by the same test: each root is checked
    /// against the ones this call has already taken, so one walk can't
    /// double-write its own ground either.
    pub(in crate::indexing) fn take(volume_id: &str, frontier: Vec<String>, mode: Mode) -> Self {
        let mut live = in_flight().lock_ignore_poison();
        let claimed = live.entry(volume_id.to_string()).or_default();

        // ⚠️ Read BEFORE this claim takes anything, so the volume-wide rule is
        // about OTHER holders. Asked per root instead, an `Exclusive` claim over
        // several roots would refuse its own second root the moment its first
        // landed.
        let volume_is_spoken_for = !claimed.roots.is_empty() && (mode == Mode::Exclusive || claimed.exclusive > 0);

        // And WHAT KIND those other holders are, read at the same moment and for
        // the same reason: a property of the table BEFORE this call, which is what
        // keeps a claim's own roots out of the answer. An `Exclusive` holder can
        // only ever be on the volume alone (any holder refuses one, and it refuses
        // everyone), so this reads a table that is all one kind or the other.
        let in_the_way = if claimed.exclusive > 0 {
            Some(Mode::Exclusive)
        } else if claimed.roots.is_empty() {
            None
        } else {
            Some(Mode::Additive)
        };

        let mut mine = Vec::with_capacity(frontier.len());
        let mut deferred = Vec::new();
        for root in frontier {
            if volume_is_spoken_for || claimed.overlapping(&root) {
                deferred.push(root);
            } else {
                claimed.insert(root.clone(), mode);
                mine.push(root);
            }
        }

        if claimed.roots.is_empty() {
            live.remove(volume_id);
        }
        if !deferred.is_empty() {
            log::debug!(
                "Cover: leaving {} frontier root(s) on '{volume_id}' to the holder already covering them",
                deferred.len()
            );
        }
        // A claim that took nothing was turned away by whoever was already here:
        // the FIRST root of a frontier can't be refused by roots this same call
        // took, since it took none yet. That's what makes this exact rather than
        // "somebody else is on the volume".
        let refused_by = (mine.is_empty() && !deferred.is_empty())
            .then_some(in_the_way)
            .flatten();
        Self {
            volume_id: volume_id.to_string(),
            mine,
            deferred,
            refused_by,
        }
    }

    /// The roots this holder is covering.
    pub(in crate::indexing) fn mine(&self) -> &[String] {
        &self.mine
    }

    /// The roots it left to another holder.
    pub(in crate::indexing) fn deferred(&self) -> &[String] {
        &self.deferred
    }

    /// What kind of holder turned this claim away, when it got NO ground at all.
    ///
    /// The whole of what a refused caller is told, and deliberately so: a mode
    /// says whether the ground is being blanked ([`Mode::Exclusive`]) or walked
    /// ([`Mode::Additive`]), which is the difference between "the walk you asked
    /// for is already happening" and "wait for the walk that's holding it". ❌ Not
    /// WHICH holder — identity is re-entrancy's vocabulary, and this table
    /// deliberately doesn't speak it.
    ///
    /// `None` whenever the claim took something, a partial grant included: ground
    /// in hand is not a refusal.
    pub(in crate::indexing) fn refused_by(&self) -> Option<Mode> {
        self.refused_by
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.mine.is_empty() {
            return;
        }
        let mut live = in_flight().lock_ignore_poison();
        if let Some(claimed) = live.get_mut(&self.volume_id) {
            for root in &self.mine {
                claimed.remove(root);
            }
            if claimed.roots.is_empty() {
                live.remove(&self.volume_id);
            }
        }
    }
}

/// Which of `frontier`'s roots a WALK on this volume is covering RIGHT NOW.
///
/// The same overlap rule [`Claim::take`] would apply, asked without taking
/// anything — so a caller can find out that the ground it wants is spoken for
/// before it commits to a walk that would take none of it. ❌ Not a reservation
/// and not a promise: the answer can go stale the moment it's read, which is why
/// `Claim::take` stays the authority and reports what it left behind.
///
/// ⚠️ Only [`Mode::Additive`] holders answer, and that's what makes the question
/// honest rather than a proxy for "is anything held". A scan takes the volume
/// root `Exclusive`ly, so an unfiltered answer would name every root of every
/// frontier for as long as one runs — telling a search to wait for a walk that
/// will never cover its ground.
pub(in crate::indexing) fn ground_being_walked(volume_id: &str, frontier: &[String]) -> Vec<String> {
    let live = in_flight().lock_ignore_poison();
    let Some(claimed) = live.get(volume_id) else {
        return Vec::new();
    };
    frontier
        .iter()
        .filter(|root| claimed.overlapping_holder(root, walking_holder))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::paths::path_prefix::is_strict_descendant;

    /// The overlap rule, written as the predicate it is. The table answers it with
    /// range queries instead, and [`the_range_queries_answer_the_overlap_rule`]
    /// holds the two to each other.
    fn overlaps(a: &str, b: &str) -> bool {
        a == b || is_strict_descendant(a, b) || is_strict_descendant(b, a)
    }

    /// The refactor's one real risk: the `BTreeMap` range queries are an
    /// OPTIMIZATION of the overlap predicate, and nothing else makes them agree
    /// with it. A prefix test that lost its component-awareness would let a walk
    /// take ground another walk is writing, which is the data-safety bug this
    /// whole module exists to prevent, and it would do it silently.
    #[test]
    fn the_range_queries_answer_the_overlap_rule() {
        let paths = [
            "/", "/a", "/a/b", "/a/b/c", "/a/bc", "/a/bc/d", "/ab", "/ab/c", "/b", "/a/b/c/d",
        ];
        for held in paths {
            let mut claims = VolumeClaims::default();
            claims.insert(held.to_string(), Mode::Additive);
            for asked in paths {
                assert_eq!(
                    claims.overlapping(asked),
                    overlaps(held, asked),
                    "holding {held}, asked about {asked}"
                );
            }
        }
    }

    /// The same agreement with the table holding MANY roots at once, which is the
    /// shape a real frontier has and the one where a range that stops too early
    /// (or runs past its prefix) shows up.
    #[test]
    fn the_range_queries_agree_with_a_table_full_of_roots() {
        let held = ["/a/b", "/a/bc", "/ab", "/x/y/z", "/x/y/zz"];
        let mut claims = VolumeClaims::default();
        for root in held {
            claims.insert(root.to_string(), Mode::Additive);
        }
        for asked in [
            "/", "/a", "/a/b", "/a/b/c", "/a/bc", "/a/bcd", "/ab/c", "/x", "/x/y", "/x/y/z/w", "/q",
        ] {
            assert_eq!(
                claims.overlapping(asked),
                held.iter().any(|h| overlaps(h, asked)),
                "asked about {asked}"
            );
        }
    }

    /// The case Decision 11 creates: a refined query asks for ground the first
    /// query's walk is still covering. The second walk takes none of it, and says
    /// which roots it left behind.
    #[test]
    fn a_root_another_walk_is_covering_is_left_to_it() {
        let first = Claim::take("overlap-vol", vec!["/a".to_string(), "/b".to_string()], Mode::Additive);
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
            Mode::Additive,
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

        let held = Claim::take("ask-vol", vec!["/a".to_string()], Mode::Additive);
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
        let _first = Claim::take("volume-one", vec!["/shared".to_string()], Mode::Additive);
        let second = Claim::take("volume-two", vec!["/shared".to_string()], Mode::Additive);

        assert_eq!(second.mine(), ["/shared"], "a different drive, a different folder");
    }

    /// A frontier that overlaps ITSELF is deduplicated by the same rule, so one
    /// walk can't double-write its own ground either.
    #[test]
    fn a_frontier_that_overlaps_itself_is_deduplicated() {
        let claim = Claim::take(
            "self-overlap-vol",
            vec!["/a".to_string(), "/a/inner".to_string()],
            Mode::Additive,
        );

        assert_eq!(claim.mine(), ["/a"]);
        assert_eq!(claim.deferred(), ["/a/inner"]);
    }

    /// The ground frees up when the walk ends, so the next search over it walks
    /// rather than deferring forever.
    #[test]
    fn ground_is_released_when_its_walk_ends() {
        drop(Claim::take("release-vol", vec!["/a".to_string()], Mode::Additive));

        let next = Claim::take("release-vol", vec!["/a".to_string()], Mode::Additive);
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
        let keeper = Claim::take("mixed-vol", vec!["/keep".to_string()], Mode::Additive);
        drop(Claim::take("mixed-vol", vec!["/go".to_string()], Mode::Additive));

        let next = Claim::take(
            "mixed-vol",
            vec!["/keep".to_string(), "/go".to_string()],
            Mode::Additive,
        );
        assert_eq!(next.mine(), ["/go"], "only the released root is free");
        assert_eq!(next.deferred(), ["/keep"]);
        drop(keeper);
    }

    // ── Modes ────────────────────────────────────────────────────────────

    /// An `Exclusive` holder speaks for the whole volume, so a walk over ground
    /// nowhere near it still defers. A truncating scan blanks the database, and
    /// "somewhere else on the same drive" is no protection from that.
    #[test]
    fn an_exclusive_holder_refuses_ground_it_does_not_overlap() {
        let _scan = Claim::take("exclusive-vol", vec!["/scan".to_string()], Mode::Exclusive);

        let walk = Claim::take("exclusive-vol", vec!["/somewhere/else".to_string()], Mode::Additive);
        assert!(walk.mine().is_empty(), "the whole volume is spoken for");
        assert_eq!(walk.deferred(), ["/somewhere/else"]);
    }

    /// And an `Exclusive` claim is refused by ground an `Additive` walk holds,
    /// however little of the volume that is. This is the truncate-under-a-walk
    /// door, from the other side.
    #[test]
    fn a_walk_anywhere_refuses_an_exclusive_claim() {
        let _walk = Claim::take("exclusive-refused-vol", vec!["/corner".to_string()], Mode::Additive);

        let scan = Claim::take("exclusive-refused-vol", vec!["/".to_string()], Mode::Exclusive);
        assert!(scan.mine().is_empty(), "one walk anywhere is enough to refuse it");
        assert_eq!(scan.deferred(), ["/"]);
    }

    /// Two `Exclusive` claims exclude each other, even on disjoint ground: each
    /// one is the whole volume's.
    #[test]
    fn two_exclusive_claims_exclude_each_other() {
        let _first = Claim::take("two-exclusive-vol", vec!["/one".to_string()], Mode::Exclusive);

        let second = Claim::take("two-exclusive-vol", vec!["/two".to_string()], Mode::Exclusive);
        assert!(second.mine().is_empty());
        assert_eq!(second.deferred(), ["/two"]);
    }

    /// Two `Additive` walks on disjoint ground both run. This is the mode pair
    /// the search walk and the phase machine rely on (Decision 13), and the one
    /// an `Exclusive`-everywhere design would have broken.
    #[test]
    fn two_additive_walks_on_disjoint_ground_both_run() {
        let _first = Claim::take("additive-vol", vec!["/one".to_string()], Mode::Additive);

        let second = Claim::take("additive-vol", vec!["/two".to_string()], Mode::Additive);
        assert_eq!(second.mine(), ["/two"], "different ground, both walk");
        assert!(second.deferred().is_empty());
    }

    /// An `Exclusive` claim over several roots takes them ALL: the volume-wide
    /// rule is about other holders, so its own first root can't refuse its
    /// second.
    #[test]
    fn an_exclusive_claim_does_not_refuse_its_own_roots() {
        let scan = Claim::take(
            "exclusive-self-vol",
            vec!["/one".to_string(), "/two".to_string()],
            Mode::Exclusive,
        );

        assert_eq!(scan.mine(), ["/one", "/two"]);
        assert!(scan.deferred().is_empty());
    }

    /// It still deduplicates ground it named twice, by the same overlap rule
    /// every other claim uses.
    #[test]
    fn an_exclusive_claim_still_deduplicates_its_own_frontier() {
        let scan = Claim::take(
            "exclusive-dedup-vol",
            vec!["/a".to_string(), "/a/inner".to_string()],
            Mode::Exclusive,
        );

        assert_eq!(scan.mine(), ["/a"]);
        assert_eq!(scan.deferred(), ["/a/inner"]);
    }

    /// The volume opens back up when the exclusive holder leaves, and the counter
    /// that tracks it comes back down with it. Without that, one finished scan
    /// would wedge its volume for the rest of the session.
    #[test]
    fn a_volume_reopens_when_its_exclusive_holder_leaves() {
        let scan = Claim::take("exclusive-release-vol", vec!["/".to_string()], Mode::Exclusive);
        drop(scan);

        let walk = Claim::take("exclusive-release-vol", vec!["/anywhere".to_string()], Mode::Additive);
        assert_eq!(walk.mine(), ["/anywhere"], "the volume is free again");
    }

    /// A refused claim says what KIND of holder is in the way, which is the whole
    /// of what the two scan entries need: an `Exclusive` one is another whole-volume
    /// run, so the walk the caller asked for is already happening.
    #[test]
    fn a_claim_refused_by_a_whole_volume_holder_says_so() {
        let _scan = Claim::take("refused-by-scan-vol", vec!["/".to_string()], Mode::Exclusive);

        let second = Claim::take("refused-by-scan-vol", vec!["/".to_string()], Mode::Exclusive);
        assert!(second.mine().is_empty());
        assert_eq!(second.refused_by(), Some(Mode::Exclusive));
    }

    /// And an `Additive` one is a walk holding ground it will let go of, which is
    /// what a caller can wait for rather than being told its scan already ran.
    #[test]
    fn a_claim_refused_by_a_walk_says_so() {
        let _walk = Claim::take("refused-by-walk-vol", vec!["/corner".to_string()], Mode::Additive);

        let scan = Claim::take("refused-by-walk-vol", vec!["/".to_string()], Mode::Exclusive);
        assert!(scan.mine().is_empty());
        assert_eq!(scan.refused_by(), Some(Mode::Additive));
    }

    /// A walk turned away by a whole-volume holder is told the same thing from
    /// the other side: what's in the way owns the drive, not a patch of it.
    #[test]
    fn a_walk_refused_by_a_whole_volume_holder_says_so() {
        let _scan = Claim::take("refused-rank-vol", vec!["/one".to_string()], Mode::Exclusive);

        let refused = Claim::take("refused-rank-vol", vec!["/two".to_string()], Mode::Additive);
        assert!(refused.mine().is_empty());
        assert_eq!(refused.refused_by(), Some(Mode::Exclusive));
    }

    /// Ground in hand is not a refusal, however much of the frontier was left
    /// behind. A partial grant's caller walks; it has nobody to wait for.
    #[test]
    fn a_claim_that_got_ground_reports_no_refusal() {
        let _held = Claim::take("refused-partial-vol", vec!["/taken".to_string()], Mode::Additive);

        let mixed = Claim::take(
            "refused-partial-vol",
            vec!["/taken".to_string(), "/free".to_string()],
            Mode::Additive,
        );
        assert_eq!(mixed.mine(), ["/free"]);
        assert_eq!(mixed.refused_by(), None, "it took ground, so nobody refused it");
    }

    /// A frontier that overlaps only ITSELF was refused by nobody: the volume was
    /// free, and the second root lost to the first root of this same claim.
    #[test]
    fn deferring_to_ones_own_root_is_not_a_refusal() {
        let claim = Claim::take(
            "refused-self-vol",
            vec!["/a".to_string(), "/a/inner".to_string()],
            Mode::Additive,
        );

        assert_eq!(claim.deferred(), ["/a/inner"]);
        assert_eq!(claim.refused_by(), None);
    }

    /// Asking who is WALKING ground skips a holder that only speaks for the
    /// volume. A scan holds its root exclusively without covering a step of the
    /// frontier a search asked about, so naming those roots would send the search
    /// off to wait for a walk that is never coming.
    #[test]
    fn a_whole_volume_holder_is_not_walking_the_ground_it_speaks_for() {
        let _scan = Claim::take("walked-filter-vol", vec!["/".to_string()], Mode::Exclusive);

        assert!(
            ground_being_walked("walked-filter-vol", &["/deep/inside".to_string()]).is_empty(),
            "a scan owns the volume, but it is not the walk covering this ground"
        );
    }

    /// A partial grant survives every mode: the walk takes the roots it can and
    /// reports the rest, rather than the all-or-nothing answer that would make a
    /// wide frontier an all-or-nothing bet.
    #[test]
    fn a_partial_grant_takes_what_it_can_and_reports_the_rest() {
        let _held = Claim::take(
            "partial-vol",
            vec!["/taken".to_string(), "/also-taken".to_string()],
            Mode::Additive,
        );

        let mixed = Claim::take(
            "partial-vol",
            vec![
                "/taken".to_string(),
                "/free".to_string(),
                "/also-taken/inner".to_string(),
                "/free-too".to_string(),
            ],
            Mode::Additive,
        );
        assert_eq!(mixed.mine(), ["/free", "/free-too"]);
        assert_eq!(mixed.deferred(), ["/taken", "/also-taken/inner"]);
    }

    /// A claim at the volume root covers everything under it, which is what lets
    /// a scan entry ask about the whole volume by naming just the root.
    #[test]
    fn a_claim_at_the_volume_root_covers_every_subtree() {
        let _whole = Claim::take("whole-vol", vec!["/".to_string()], Mode::Additive);

        assert_eq!(
            ground_being_walked("whole-vol", &["/deep/inside/here".to_string()]),
            ["/deep/inside/here"],
            "the root holds every subtree under it"
        );
    }

    /// And the reverse: a subtree claim answers a whole-volume question, which is
    /// how a scan entry probing with the volume root finds a walk anywhere.
    #[test]
    fn a_subtree_claim_answers_a_whole_volume_question() {
        let _subtree = Claim::take("subtree-vol", vec!["/deep/inside".to_string()], Mode::Additive);

        assert_eq!(
            ground_being_walked("subtree-vol", &["/".to_string()]),
            ["/"],
            "asking about the root finds a walk anywhere under it"
        );
    }
}
