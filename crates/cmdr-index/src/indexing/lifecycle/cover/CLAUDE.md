# The cover walk (filling in what a search can't answer for)

`read/coverage.rs` says what a scope still needs walked; this walks it, writing every row through the volume's normal
writer so the next search over the same ground walks less.

`mod.rs` the walk, its one per-kind branch (`Ground`: local guarded walker vs the `Volume` trait), and its
`CoverOutcome`; `bootstrap.rs` what a walk needs before it can start; `live.rs` how holders stay off each other's
ground. The registry and phase machine are `../CLAUDE.md`.

## Must-knows

- **A walk reuses the RUNNING writer or stands one up** (`Activation::WriterOnly`, ❌ no scan or watcher), and EVICTS an
  index this build's coverage rules refuse. ⚠️ A volume mid-SCAN isn't walked.
- **A walk stops through the CALLER's token and flushes before reporting**, cancel included, unless the caller took the
  drain. **`CoverOutcome::abandoned_ground` is independent of every other field**: ❌ any caller reporting completeness
  must consult it.
- **A walk RELEASES its branch whatever the registry phase** (`finish_branch_coverage` reaches the set directly), ❌
  never behind `with_running_manager` — a walk ending in a `ShuttingDown` window would hold that ground forever.
- **Bootstrap creates the rows a walk needs to START, each at `listed_epoch = 0`; ❌ nothing here claims coverage** —
  the walk earns it, and an ancestor claiming a listing it never did would mark a whole tree covered off one walked
  folder.
- **A missing `entries` row is NOT only a cold-drive case**: a folder created since its parent was listed has none on a
  drive indexed yesterday. ❌ Don't gate bootstrap on "never indexed".
- **A holder CLAIMS the ground it writes, and a later one over claimed ground doesn't take it** (`live.rs`). Two writers
  over one directory allocate different ids for the same names, and `INSERT OR IGNORE` makes the loser lose its whole
  subtree. A data-safety rule, ❌ not a performance one. A deferred search loses nothing durable, so ❌ don't reach for
  a shared-subscriber fan-out. The table also holds the one rescan a volume is WAITING for ("may it start" is one
  question: owed, and no ground held), so an entry outlives its claims and ❌ pruning on `roots.is_empty()` alone drops
  the request (`is_idle()`).
- **A claim holds `Additive`** (a cover walk: the ground it names, composing with the phase machine) **or `Exclusive`**
  (the whole volume: a scan, a journal replay). ❌ Never solve a third wish with holder identity or re-entrancy. A
  refusal reports the blocking holder's MODE, which is what the scan entries map to their outcomes (`../DETAILS.md`).
- **`ground_being_walked` answers for `Additive` holders ONLY**: a scan owns the volume without covering any root of the
  frontier it was asked about, so ❌ never let one answer: it would send a search off to wait for a walk that isn't
  coming.
- **The claim table is a path-keyed `BTreeMap`, ❌ never a `Vec` scan**: `take` checks each root against the ones it
  already took, so a linear test is quadratic in the frontier's own width (446.77 ms at 2,503 roots, on the search's own
  thread). Its ranges only approximate the component-aware overlap predicate; ❌ don't delete
  `the_range_queries_answer_the_overlap_rule`, the one thing holding them together.
- **A claim that takes NO ground spawns no walk at all** (`CoverWalk::took_no_ground`), so ❌ never put work a no-ground
  request still owes into `start` or `walk_frontier`: a walk's tail commits the writer, parking behind every queued
  batch — seconds behind a first index, spent to commit nothing (`docs/notes/cover-no-ground-block-2026-08-15.md`).

The walk's stages, the claim mechanism, and everything bootstrap has to stand up: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
