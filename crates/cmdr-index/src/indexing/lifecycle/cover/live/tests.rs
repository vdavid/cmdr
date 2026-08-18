//! Unit tests for the claim table: the overlap rule its range queries stand for,
//! the two modes, partial grant, and the one rescan a volume waits for. Pure and
//! synchronous, so exhaustive coverage costs milliseconds.

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
        claims.insert(held.to_string(), Holder::a_background_walk());
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
        claims.insert(root.to_string(), Holder::a_background_walk());
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
    let first = Claim::take(
        "overlap-vol",
        vec!["/a".to_string(), "/b".to_string()],
        Holder::a_background_walk(),
    );
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
        Holder::a_background_walk(),
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

    let held = Claim::take("ask-vol", vec!["/a".to_string()], Holder::a_background_walk());
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
    let _first = Claim::take("volume-one", vec!["/shared".to_string()], Holder::a_background_walk());
    let second = Claim::take("volume-two", vec!["/shared".to_string()], Holder::a_background_walk());

    assert_eq!(second.mine(), ["/shared"], "a different drive, a different folder");
}

/// A frontier that overlaps ITSELF is deduplicated by the same rule, so one
/// walk can't double-write its own ground either.
#[test]
fn a_frontier_that_overlaps_itself_is_deduplicated() {
    let claim = Claim::take(
        "self-overlap-vol",
        vec!["/a".to_string(), "/a/inner".to_string()],
        Holder::a_background_walk(),
    );

    assert_eq!(claim.mine(), ["/a"]);
    assert_eq!(claim.deferred(), ["/a/inner"]);
}

/// The ground frees up when the walk ends, so the next search over it walks
/// rather than deferring forever.
#[test]
fn ground_is_released_when_its_walk_ends() {
    drop(Claim::take(
        "release-vol",
        vec!["/a".to_string()],
        Holder::a_background_walk(),
    ));

    let next = Claim::take("release-vol", vec!["/a".to_string()], Holder::a_background_walk());
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
    let keeper = Claim::take("mixed-vol", vec!["/keep".to_string()], Holder::a_background_walk());
    drop(Claim::take(
        "mixed-vol",
        vec!["/go".to_string()],
        Holder::a_background_walk(),
    ));

    let next = Claim::take(
        "mixed-vol",
        vec!["/keep".to_string(), "/go".to_string()],
        Holder::a_background_walk(),
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
    let _scan = Claim::take("exclusive-vol", vec!["/scan".to_string()], Holder::Rewriting);

    let walk = Claim::take(
        "exclusive-vol",
        vec!["/somewhere/else".to_string()],
        Holder::a_background_walk(),
    );
    assert!(walk.mine().is_empty(), "the whole volume is spoken for");
    assert_eq!(walk.deferred(), ["/somewhere/else"]);
}

/// And an `Exclusive` claim is refused by ground an `Additive` walk holds,
/// however little of the volume that is. This is the truncate-under-a-walk
/// door, from the other side.
#[test]
fn a_walk_anywhere_refuses_an_exclusive_claim() {
    let _walk = Claim::take(
        "exclusive-refused-vol",
        vec!["/corner".to_string()],
        Holder::a_background_walk(),
    );

    let scan = Claim::take("exclusive-refused-vol", vec!["/".to_string()], Holder::Rewriting);
    assert!(scan.mine().is_empty(), "one walk anywhere is enough to refuse it");
    assert_eq!(scan.deferred(), ["/"]);
}

/// Two `Exclusive` claims exclude each other, even on disjoint ground: each
/// one is the whole volume's.
#[test]
fn two_exclusive_claims_exclude_each_other() {
    let _first = Claim::take("two-exclusive-vol", vec!["/one".to_string()], Holder::Rewriting);

    let second = Claim::take("two-exclusive-vol", vec!["/two".to_string()], Holder::Rewriting);
    assert!(second.mine().is_empty());
    assert_eq!(second.deferred(), ["/two"]);
}

/// Two `Additive` walks on disjoint ground both run. This is the mode pair
/// the search walk and the phase machine rely on (Decision 13), and the one
/// an `Exclusive`-everywhere design would have broken.
#[test]
fn two_additive_walks_on_disjoint_ground_both_run() {
    let _first = Claim::take("additive-vol", vec!["/one".to_string()], Holder::a_background_walk());

    let second = Claim::take("additive-vol", vec!["/two".to_string()], Holder::a_background_walk());
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
        Holder::Rewriting,
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
        Holder::Rewriting,
    );

    assert_eq!(scan.mine(), ["/a"]);
    assert_eq!(scan.deferred(), ["/a/inner"]);
}

/// The volume opens back up when the exclusive holder leaves, and the counter
/// that tracks it comes back down with it. Without that, one finished scan
/// would wedge its volume for the rest of the session.
#[test]
fn a_volume_reopens_when_its_exclusive_holder_leaves() {
    let scan = Claim::take("exclusive-release-vol", vec!["/".to_string()], Holder::Rewriting);
    drop(scan);

    let walk = Claim::take(
        "exclusive-release-vol",
        vec!["/anywhere".to_string()],
        Holder::a_background_walk(),
    );
    assert_eq!(walk.mine(), ["/anywhere"], "the volume is free again");
}

/// A refused claim says what KIND of holder is in the way, which is the whole
/// of what the two scan entries need: an `Exclusive` one is another whole-volume
/// run, so the walk the caller asked for is already happening.
#[test]
fn a_claim_refused_by_a_whole_volume_holder_says_so() {
    let _scan = Claim::take("refused-by-scan-vol", vec!["/".to_string()], Holder::Rewriting);

    let second = Claim::take("refused-by-scan-vol", vec!["/".to_string()], Holder::Rewriting);
    assert!(second.mine().is_empty());
    assert_eq!(second.refused_by(), Some(Mode::Exclusive));
}

/// And an `Additive` one is a walk holding ground it will let go of, which is
/// what a caller can wait for rather than being told its scan already ran.
#[test]
fn a_claim_refused_by_a_walk_says_so() {
    let _walk = Claim::take(
        "refused-by-walk-vol",
        vec!["/corner".to_string()],
        Holder::a_background_walk(),
    );

    let scan = Claim::take("refused-by-walk-vol", vec!["/".to_string()], Holder::Rewriting);
    assert!(scan.mine().is_empty());
    assert_eq!(scan.refused_by(), Some(Mode::Additive));
}

/// A walk turned away by a whole-volume holder is told the same thing from
/// the other side: what's in the way owns the drive, not a patch of it.
#[test]
fn a_walk_refused_by_a_whole_volume_holder_says_so() {
    let _scan = Claim::take("refused-rank-vol", vec!["/one".to_string()], Holder::Rewriting);

    let refused = Claim::take(
        "refused-rank-vol",
        vec!["/two".to_string()],
        Holder::a_background_walk(),
    );
    assert!(refused.mine().is_empty());
    assert_eq!(refused.refused_by(), Some(Mode::Exclusive));
}

/// Ground in hand is not a refusal, however much of the frontier was left
/// behind. A partial grant's caller walks; it has nobody to wait for.
#[test]
fn a_claim_that_got_ground_reports_no_refusal() {
    let _held = Claim::take(
        "refused-partial-vol",
        vec!["/taken".to_string()],
        Holder::a_background_walk(),
    );

    let mixed = Claim::take(
        "refused-partial-vol",
        vec!["/taken".to_string(), "/free".to_string()],
        Holder::a_background_walk(),
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
        Holder::a_background_walk(),
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
    let _scan = Claim::take("walked-filter-vol", vec!["/".to_string()], Holder::Rewriting);

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
        Holder::a_background_walk(),
    );

    let mixed = Claim::take(
        "partial-vol",
        vec![
            "/taken".to_string(),
            "/free".to_string(),
            "/also-taken/inner".to_string(),
            "/free-too".to_string(),
        ],
        Holder::a_background_walk(),
    );
    assert_eq!(mixed.mine(), ["/free", "/free-too"]);
    assert_eq!(mixed.deferred(), ["/taken", "/also-taken/inner"]);
}

/// A claim at the volume root covers everything under it, which is what lets
/// a scan entry ask about the whole volume by naming just the root.
#[test]
fn a_claim_at_the_volume_root_covers_every_subtree() {
    let _whole = Claim::take("whole-vol", vec!["/".to_string()], Holder::a_background_walk());

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
    let _subtree = Claim::take(
        "subtree-vol",
        vec!["/deep/inside".to_string()],
        Holder::a_background_walk(),
    );

    assert_eq!(
        ground_being_walked("subtree-vol", &["/".to_string()]),
        ["/"],
        "asking about the root finds a walk anywhere under it"
    );
}

// ── The walk a volume is waiting for ─────────────────────────────

/// One request per volume, and taking it is what spends it.
#[test]
fn a_volume_waits_for_at_most_one_scan() {
    remember_rescan("owed-one-vol");
    remember_rescan("owed-one-vol");
    assert!(take_rescan("owed-one-vol"), "the request is there");
    assert!(
        !take_rescan("owed-one-vol"),
        "and a second click didn't queue a second scan"
    );
}

/// A volume that stopped indexing is waiting for nothing.
#[test]
fn a_torn_down_volume_keeps_no_request() {
    remember_rescan("owed-teardown-vol");
    forget_rescan("owed-teardown-vol");
    assert!(!take_rescan("owed-teardown-vol"));
}

/// Requests are per volume: one drive's click doesn't rescan another.
#[test]
fn two_volumes_wait_independently() {
    remember_rescan("owed-vol-one");
    assert!(!take_rescan("owed-vol-two"));
    assert!(take_rescan("owed-vol-one"));
}

/// The whole point of keeping the waiter beside the claims: a rescan can start
/// only once the ground is free, and that is ONE look at the table.
#[test]
fn a_waiting_rescan_can_start_only_when_the_ground_is_free() {
    assert!(
        !a_rescan_can_start("owed-ready-vol"),
        "a volume nobody asked about is waiting for nothing"
    );

    let walking = Claim::take(
        "owed-ready-vol",
        vec!["/scope".to_string()],
        Holder::a_background_walk(),
    );
    remember_rescan("owed-ready-vol");
    assert!(
        !a_rescan_can_start("owed-ready-vol"),
        "a walk still holds ground, so a scan that started now would truncate under it"
    );

    drop(walking);
    assert!(a_rescan_can_start("owed-ready-vol"), "and the last holder out frees it");
    assert!(take_rescan("owed-ready-vol"));
}

/// A volume holding nothing but a waiting request keeps its entry, and loses it
/// when the request is spent. Recording the request BEFORE the scan tries to
/// start means this shape is the normal one, not an edge case.
#[test]
fn a_waiting_request_outlives_an_empty_claim_table() {
    remember_rescan("owed-empty-vol");
    assert!(
        in_flight().lock_ignore_poison().contains_key("owed-empty-vol"),
        "the request survives having no ground held beside it"
    );

    assert!(take_rescan("owed-empty-vol"));
    assert!(
        !in_flight().lock_ignore_poison().contains_key("owed-empty-vol"),
        "and the entry goes with it, rather than growing a map forever"
    );
}

// ── Asking for ground somebody else has ──────────────────────────────

/// How long a test that expects the ask to fail gives it. Long enough that a
/// loaded machine doesn't flake, short enough that a stuck test is a fast one.
const A_SHORT_WAIT: Duration = Duration::from_millis(50);

/// A walk somebody is waiting on, with a token nothing else holds.
fn a_walk_someone_waits_on() -> Holder {
    Holder::Walking {
        yield_to: CancellationToken::new(),
        for_whom: WalkFor::TheUser,
    }
}

/// The yield channel itself: a refusal that only NAMED the holder left the
/// person waiting on a walk they couldn't reach, so the claim carries the token
/// that stops it and asking for the ground pulls it.
#[test]
fn a_background_walk_holding_ground_somebody_wants_is_asked_to_stop() {
    let stop = CancellationToken::new();
    let _background = Claim::take(
        "asked-vol",
        vec!["/a".to_string()],
        Holder::Walking {
            yield_to: stop.clone(),
            for_whom: WalkFor::TheIndex,
        },
    );
    assert!(!stop.is_cancelled(), "nobody has asked it for anything yet");

    let waiting = Claim::preempt(
        "asked-vol",
        vec!["/a".to_string()],
        a_walk_someone_waits_on(),
        A_SHORT_WAIT,
    );

    assert!(stop.is_cancelled(), "the walk holding the ground was asked to stop");
    assert!(
        waiting.mine().is_empty(),
        "and the ground stays its own until it actually lets go"
    );
    assert_eq!(waiting.deferred(), ["/a"]);
    assert_eq!(waiting.refused_by(), Some(Mode::Additive));
}

/// The handoff, and the reason it lives in the release's own critical section: a
/// third claim arriving the instant the holder let go must find the ground
/// already in the waiter's name. Released and re-taken instead, the walk that
/// asked for the ground would cover nothing and report success — which is
/// exactly why preemption was ruled out before.
#[test]
fn ground_a_yielding_walk_lets_go_of_is_already_the_waiters() {
    let stop = CancellationToken::new();
    let background = Claim::take(
        "handover-vol",
        vec!["/a".to_string()],
        Holder::Walking {
            yield_to: stop.clone(),
            for_whom: WalkFor::TheIndex,
        },
    );

    let waiter = std::thread::spawn(|| {
        Claim::preempt(
            "handover-vol",
            vec!["/a".to_string()],
            a_walk_someone_waits_on(),
            Duration::from_secs(10),
        )
    });
    // The ask reaching the holder is also how we know the waiter is registered.
    cmdr_fs::testing::wait_until(Duration::from_secs(5), "the holder is asked to yield", || {
        stop.is_cancelled()
    });

    drop(background);

    let racer = Claim::take("handover-vol", vec!["/a".to_string()], Holder::a_background_walk());
    assert!(
        racer.mine().is_empty(),
        "the ground never passed through nobody's hands, so a racing claim finds it taken"
    );
    assert_eq!(
        waiter.join().expect("the waiting claim").mine(),
        ["/a"],
        "and the walk that asked for it got it"
    );
}

/// Two walks somebody is waiting on leave each other alone. Asking would make
/// them take turns stopping each other, and neither would ever cover its ground.
#[test]
fn a_walk_somebody_is_waiting_on_is_never_asked_to_yield() {
    let stop = CancellationToken::new();
    let _first = Claim::take(
        "no-ping-pong-vol",
        vec!["/a".to_string()],
        Holder::Walking {
            yield_to: stop.clone(),
            for_whom: WalkFor::TheUser,
        },
    );

    let second = Claim::preempt(
        "no-ping-pong-vol",
        vec!["/a".to_string()],
        a_walk_someone_waits_on(),
        A_SHORT_WAIT,
    );

    assert!(
        !stop.is_cancelled(),
        "a walk with somebody waiting on it outranks nothing"
    );
    assert_eq!(second.deferred(), ["/a"]);
}

/// A truncating scan is never asked, and nobody waits for one either: half a
/// rewrite is not a thing to hand over, and a scan runs for minutes.
#[test]
fn a_rewriting_holder_is_neither_asked_nor_waited_for() {
    let _scan = Claim::take("no-ask-scan-vol", vec!["/".to_string()], Holder::Rewriting);

    let started = Instant::now();
    let waiting = Claim::preempt(
        "no-ask-scan-vol",
        vec!["/somewhere".to_string()],
        a_walk_someone_waits_on(),
        Duration::from_secs(30),
    );

    assert_eq!(waiting.refused_by(), Some(Mode::Exclusive));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it answered rather than sitting out a scan that will never hand anything over"
    );
}

/// Ground nobody holds is this claim's immediately, and the roots it does have to
/// ask for are still reported separately: partial grant survives the ask exactly
/// as it survives a plain take.
#[test]
fn asking_for_ground_keeps_the_partial_grant() {
    let _background = Claim::take(
        "ask-partial-vol",
        vec!["/held".to_string()],
        Holder::a_background_walk(),
    );

    let waiting = Claim::preempt(
        "ask-partial-vol",
        vec!["/free".to_string(), "/held".to_string(), "/free-too".to_string()],
        a_walk_someone_waits_on(),
        A_SHORT_WAIT,
    );

    assert_eq!(waiting.mine(), ["/free", "/free-too"]);
    assert_eq!(waiting.deferred(), ["/held"]);
    assert_eq!(waiting.refused_by(), None, "it took ground, so nobody refused it");
}

/// And a frontier that overlaps ITSELF is settled before anyone is asked for
/// anything: no holder can hand over ground this same walk is about to cover.
#[test]
fn asking_for_a_self_overlapping_frontier_deduplicates_it_first() {
    let waiting = Claim::preempt(
        "ask-self-overlap-vol",
        vec!["/a".to_string(), "/a/inner".to_string()],
        a_walk_someone_waits_on(),
        A_SHORT_WAIT,
    );

    assert_eq!(waiting.mine(), ["/a"]);
    assert_eq!(waiting.deferred(), ["/a/inner"]);
    assert_eq!(waiting.refused_by(), None);
}

/// The waiter is not a holder: a claim that asked and got nothing leaves the
/// volume's entry exactly as it found it, rather than pinning a map row for the
/// life of the process.
#[test]
fn a_waiter_that_got_nothing_leaves_no_trace() {
    let background = Claim::take("ask-trace-vol", vec!["/a".to_string()], Holder::a_background_walk());
    drop(Claim::preempt(
        "ask-trace-vol",
        vec!["/a".to_string()],
        a_walk_someone_waits_on(),
        A_SHORT_WAIT,
    ));
    drop(background);

    assert!(
        !in_flight().lock_ignore_poison().contains_key("ask-trace-vol"),
        "nothing is held, nobody is owed a rescan, and nobody is part way through a handover"
    );
}
