# The claim table, in depth

Who may write a patch of a volume right now, how ground changes hands, and the one rescan a volume remembers. The walk
that consumes all of this is `../DETAILS.md`; the rule that makes it necessary (two writers over one directory) is
stated there under "One writer per database, and one walk per patch of ground".

## The two modes a claim can hold in

`Mode` is the whole arbitration vocabulary, and two values are all of it:

- **`Additive`** — the holder speaks only for the ground it names. Two additive claims compose as long as their
  frontiers stay off each other. Every walk `cover::start` makes is one.
- **`Exclusive`** — the holder speaks for the whole volume, whatever ground anyone else names. What a truncating scan
  needs: it blanks the database and bumps the epoch, so "somewhere else on the same drive" is no protection. Journal
  replay takes one too, for a subtler reason (`../../DETAILS.md`).

**A claim names its `Holder`, and the mode falls out of that** rather than being passed beside it. `Holder::Rewriting`
is the whole volume; `Holder::Walking` carries the token that stops that one walk, plus whether somebody is waiting on
it. The reason it is a type and not two arguments: a walking holder without a yield handle is a holder nobody can ask to
stop, and the enum makes that unrepresentable instead of a rule saying to remember it.

The conflict rule follows from that: an `Exclusive` holder refuses everything on its volume; an `Exclusive` claim is
refused by any holder at all; `Additive` against `Additive` is decided per root by the overlap rule. **The volume-wide
half is read BEFORE the claim takes anything**, so a claim naming several roots never conflicts with its own — asked per
root, an `Exclusive` claim over `/one` and `/two` would refuse `/two` the moment `/one` landed.

**A refusal reports the blocking holder's MODE** (`Claim::refused_by`), which is the whole of what a refused caller is
told. It is read off the table as it stood BEFORE the claim took anything, and reported only when the claim got NO
ground — the first root of a frontier can't be refused by roots the same call took, since it took none yet, so a
self-overlapping frontier reads as "refused by nobody" rather than by itself. What the two scan entries do with it is
`../../DETAILS.md` § "The two single-flight questions a scan has to ask".

Two consequences worth stating, because both are easy to get backwards:

- **A holder that speaks for a volume is not WALKING it.** `ground_being_walked` filters to `Additive` holders, so a
  running scan never answers it (`Index::coverage`'s `being_walked`, and with it
  `StartOutcome::DeferredUntilSearchEnds`, keeps meaning "another WALK has this ground").
- **A whole-volume claim outlives the call that takes it.** `start_scan` and `start_volume_scan` return while their
  walks run, so the claim travels into the task that ends the run. Custody and the release sites: `../../DETAILS.md`.

## Asking a walk for its ground

A refusal that only NAMES the holder leaves a person waiting on a background walk they can't reach. So a walk somebody
is waiting on takes its frontier through `Claim::preempt` instead of `Claim::take`: every background walk over ground it
names is asked to stop, and the roots they let go of come to it. `cover::start` picks the door from `WalkFor`, which
`Index::cover` sets to `TheUser` and the phase machine to `TheIndex`.

Two things had to be true for this to work at all, and they are independent:

1. **The handover happens inside the LEAVING holder's own critical section.** `Claim`'s `Drop` removes the roots and
   hands the freed ones straight to the waiters, under one lock. ❌ Never "release, then let the waiter take it again":
   between those two moments any claim can arrive, and the waiter that asked for the ground would cover nothing while
   reporting success. That race is what made preemption look impossible. Guarded by
   `tests::ground_a_yielding_walk_lets_go_of_is_already_the_waiters`, which races a third claim against the release.
2. **The wait is bounded, because the ask is not a promise.** The walker checks its token between directories, so
   cancel-to-join is a directory's read plus the walker's own drain: 89 ms median over 2,400-directory roots, 151 ms
   median and 214 ms worst over 40,000-directory ones (`docs/notes/preemption-2026-08-18.md`). `YIELD_WAIT` is the
   budget, and a holder that doesn't stop inside it costs the waiter that wait and then the answer a plain `take` would
   have given.

**Who may be asked is the whole of the policy**, and it is `WalkFor`, ❌ never holder identity:

- A `Holder::Rewriting` is never asked, and nobody waits for one either: it is blanking the volume, half a truncate is
  not a thing to hand over, and a scan runs for minutes. A waiter whose only remaining roots sit under one stops hoping
  and reports `refused_by(Exclusive)` immediately.
- A walk somebody is already waiting on is never asked. Two of them asking each other would take turns stopping, and
  neither would cover its ground.
- ⚠️ The phase machine ❌ never takes the waiting door. A background walker that queued behind user walks would stop
  converging the moment somebody kept searching (the ownership plan's constraint 4).

**The token is a CHILD of the caller's, always** (`cover::start`). Stopping one walk so its ground can change hands must
not stop the volume: a caller that handed its own token straight in would have every yield cancel everything else
hanging off it. The caller's token still stops this walk, because that is what a parent does.

**A stopped walk COMMITS what it wrote before it lets go**, whatever its caller promised about the drain. Ground changes
hands the instant it releases, and the holder taking it decides what is virgin ground by reading the DATABASE — rows
still in the writer's queue read as directories nobody has written, so the next holder allocates fresh ids for names
this walk already named, and that is the `INSERT OR IGNORE` collision this whole table exists to prevent. ⚠️ **No test
can make that fail**: the window opens only behind a writer backlog (a first index, a share), and a unit test's writer
keeps up — a behavioural version stayed green with the flush removed, against 2,000 directories, a 20,000-message
backlog, and a read taken the instant the ground moved. So it is pinned in the source instead, by `../tests.rs`'s
`flushing_a_stopped_walk_cannot_be_tidied_away`.

**A waiter is not a holder.** It sits in the volume's entry until it is served or gives up, which is why `is_idle()`
counts handovers as well as roots and the rescan bit: an entry pruned while somebody was part way through being handed
ground would drop the grant on the floor.

## And the one walk a volume is waiting for

The table also carries one bit per volume: whether a manual "Rescan now" was turned away and is waiting for the ground.
It lives HERE rather than in a set of its own because "may that rescan start" is one question about this table — is
anything owed, and is the ground free — and two structures answering half each can disagree in the window between them,
with a truncating scan riding on the answer. `remember_rescan` / `take_rescan` / `forget_rescan` / `a_rescan_can_start`
are the whole surface; what the request MEANS, who runs it, and when is `../../rescan_request.rs` and `../../DETAILS.md`
§ "The one walk a volume remembers".

⚠️ A volume's entry therefore outlives its claims: the request is recorded BEFORE its scan tries to start, which is
routinely a moment when nobody holds anything. ❌ Never prune an entry on `roots.is_empty()` alone — `is_idle()` is the
test, and `tests::a_waiting_request_outlives_an_empty_claim_table` is what catches losing it.

⚠️ **Two modes deliberately do not express every holder's wish.** A holder wanting "block truncating scans and search
walks, but not phase walks" has no mode: `Exclusive` would refuse the phase machine's own per-group walks, and
`Additive` at the volume root conflicts with every subtree claim because an ancestor counts as overlapping. The
resolution is that the phase machine takes no volume-wide claim at all and `phases_have_work` stays a separate question
(`../../DETAILS.md` § "The two single-flight questions a scan has to ask"). ❌ Don't solve it with holder identity or
re-entrancy: that is the broker design, and it was rejected.

## What the claim table is, and the cost that shaped it

Claims are held per volume in a path-keyed `BTreeMap`, so an overlap question is two range queries — the ancestor chain
(a handful of lookups whatever the table holds) plus one sorted descendant range that costs what it yields. ❌ Never a
`Vec` scan: `take` checks each root against the roots it has already taken, so a linear membership test makes ONE call
quadratic in its own width, and a cold-drive search really does arrive with thousands of roots. Measured at 2,503 roots:
446.77 ms before, 2.23 ms after, on the caller's thread before any directory is listed
(`docs/notes/claim-table-cost-2026-08-17.md`).

⚠️ **The range queries are an OPTIMIZATION of a predicate, and nothing but a test makes them agree with it.** The
predicate is `a == b || is_strict_descendant(a, b) || is_strict_descendant(b, a)`; it lives in `tests.rs` as the
reference implementation, and `the_range_queries_answer_the_overlap_rule` holds the table to it over a grid of sibling
traps. A prefix test that quietly lost its component-awareness would let a walk take ground another walk is writing —
the exact data-safety bug this module exists to prevent — and every other test here would still pass. ❌ Don't delete
that test as redundant.
