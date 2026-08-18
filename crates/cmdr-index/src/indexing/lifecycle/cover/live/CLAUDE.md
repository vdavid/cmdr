# The claim table (who may write this ground right now)

One table answers ground ownership for every holder on a volume: a cover walk, a truncating scan, and a journal replay
all take a `Claim` here, and a scan entry's single-flight question is a claim attempt. `mod.rs` is the whole mechanism;
`bench.rs` measures it (`#[ignore]`d); the walk that consumes it is `../CLAUDE.md`.

## Must-knows

- **A holder CLAIMS the ground it writes, and a later one over claimed ground doesn't take it.** Two writers over one
  directory allocate different ids for the same names, and `INSERT OR IGNORE` makes the loser lose its whole subtree. A
  data-safety rule, ❌ not a performance one. A deferred search loses nothing durable, so ❌ don't reach for a
  shared-subscriber fan-out.
- **`Holder::Walking` (a cover walk, `Additive`) or `Holder::Rewriting`** (the whole volume: a scan, a replay,
  `Exclusive`). The mode falls OUT of the holder, and a walking holder carries the token that stops it, so "a holder
  nobody can ask to stop" is unrepresentable. ❌ Never solve a third wish with holder identity or re-entrancy. A refusal
  reports the blocking holder's MODE, which the scan entries map to their outcomes (`../../DETAILS.md`).
- **A walk somebody is waiting on takes its ground through `preempt`, ❌ never `take`**: background walks over it are
  asked to yield, and what they let go of is handed over INSIDE the leaving holder's critical section. ❌ Never
  "release, then let the waiter take it again" — that gap is the race that made preemption look impossible, and
  `tests::ground_a_yielding_walk_lets_go_of_is_already_the_waiters` races a third claim against it. ⚠️ The wait is
  bounded and the ask is not a promise.
- **Only `WalkFor::TheIndex` holders are asked to yield.** ❌ Never a walk somebody is already waiting on (two would
  take turns stopping and neither would cover its ground), ❌ never a `Rewriting` holder (half a truncate is not a thing
  to hand over, and nobody waits for one either). Both in `tests`, by name.
- **A stopped walk COMMITS before it lets go** (`../mod.rs`'s flush): ground changes hands immediately, and the next
  holder reads the DATABASE to decide what is virgin. ⚠️ No behavioural test can fail on this, so
  `../tests::flushing_a_stopped_walk_cannot_be_tidied_away` pins it in the source.
- **`ground_being_walked` answers for `Additive` holders ONLY**: a scan owns the volume without covering any root of the
  frontier it was asked about, so ❌ never let one answer — it would send a search off to wait for a walk that isn't
  coming.
- **Path-keyed `BTreeMap`, ❌ never a `Vec` scan**: `take` checks each root against the ones it already took, so a
  linear test is quadratic in the frontier's own width (446.77 ms at 2,503 roots, on the search's own thread). Its
  ranges only approximate the component-aware overlap predicate; ❌ don't delete
  `the_range_queries_answer_the_overlap_rule`, the one thing holding them together.
- **The table also holds the one rescan a volume is WAITING for** ("may it start" is one question: owed, and no ground
  held), and whoever is part way through being handed ground. So an entry outlives its claims, and ❌ pruning on
  `roots.is_empty()` alone drops both (`is_idle()`).

The mechanism in depth, the handover's ordering rules, and the costs that shaped the structure: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
