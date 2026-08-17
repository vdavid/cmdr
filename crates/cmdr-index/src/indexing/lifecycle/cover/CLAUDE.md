# The cover walk (filling in what a search can't answer for)

`read/coverage.rs` says what a scope still needs walked; this walks it, writing every row through the volume's normal
writer so the next search over the same ground walks less.

`mod.rs` the walk, its one per-kind branch (`Ground`: local guarded walker vs the `Volume` trait), and its
`CoverOutcome`; `bootstrap.rs` what a walk needs standing up before it can start; `live.rs` how two walks stay off the
same ground. The registry and the phase machine it runs against are `../CLAUDE.md`.

## Must-knows

- **A walk reuses the RUNNING writer or stands one up** (`Activation::WriterOnly`, ❌ no scan or watcher), and EVICTS an
  index whose coverage this build refuses. ⚠️ A volume mid-SCAN isn't walked.
- **A walk stops through the CALLER's token and flushes before reporting**, cancel included, unless the caller took the
  drain. **`CoverOutcome::abandoned_ground` is independent of every other field**: ❌ any caller reporting completeness
  must consult it.
- **A walk RELEASES its branch whatever the registry phase** (`finish_branch_coverage` reaches the set directly), ❌
  never behind `with_running_manager` — a walk ending in a `ShuttingDown` window would hold that ground forever.
- **Bootstrap creates the rows a walk needs to START, each at `listed_epoch = 0`; ❌ nothing here claims coverage.** The
  walk earns it. An ancestor that claimed a listing it never did would mark a whole tree covered off one walked folder.
- **A missing `entries` row is NOT only a cold-drive case**: a folder created since its parent was listed has no row on
  a drive indexed yesterday either. ❌ Don't gate bootstrap on "never indexed".
- **A walk CLAIMS its frontier roots, and a later walk over claimed ground doesn't take it** (`live.rs`). Two walks over
  one directory allocate different ids for the same names, and `INSERT OR IGNORE` makes the loser lose its whole
  subtree. A data-safety rule, ❌ not a performance one. The deferred search loses nothing durable, so ❌ don't reach
  for a shared-subscriber fan-out.
- **A claim holds `Additive`** (every cover walk: the ground it names, composes with the phase machine) **or
  `Exclusive`** (the whole volume, for a holder that blanks the database). ❌ Never solve a third wish with holder
  identity or re-entrancy.
- **The claim table is a path-keyed `BTreeMap`, ❌ never a `Vec` scan**: `take` checks each root against the ones it
  already took, so a linear test is quadratic in the frontier's own width (446.77 ms at 2,503 roots, on the thread the
  search waits on). Its range queries only approximate the component-aware overlap predicate; ❌ don't delete
  `the_range_queries_answer_the_overlap_rule`, the one thing holding them together.
- **A claim that takes NO ground spawns no walk at all** (`CoverWalk::took_no_ground`), so ❌ never put work a no-ground
  request still owes into `start` or `walk_frontier`. A walk's tail commits the writer, which parks behind every batch
  already queued: seconds behind a first index, spent to commit nothing, while the search that asked sat silent.
  `docs/notes/cover-no-ground-block-2026-08-15.md`.

The walk's stages, the claim mechanism, and everything bootstrap has to stand up: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
