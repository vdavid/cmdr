# Cover-walk internals

What a coverage walk needs before it can start (`bootstrap.rs`) and how two walks stay off the same ground (`live.rs`).
The walk itself, its `CoverOutcome`, and the activation it goes through are `../CLAUDE.md`.

## Must-knows

- **Bootstrap creates the rows a walk needs to START, each at `listed_epoch = 0`; ❌ nothing here claims coverage.** The
  walk earns it. An ancestor that claimed a listing it never did would mark a whole tree covered off one walked folder.
- **A missing `entries` row is NOT only a cold-drive case**: a folder created since its parent was listed has no row on
  a drive indexed yesterday either. ❌ Don't gate bootstrap on "never indexed".
- **A walk CLAIMS its frontier roots, and a later walk over claimed ground doesn't take it** (`live.rs`). Two walks over
  one directory allocate different ids for the same names, and `INSERT OR IGNORE` against
  `UNIQUE (parent_id, name_folded)` makes the loser lose its whole subtree. This is a data-safety rule, ❌ not a
  performance one.
- **The second search loses nothing durable**: the first walk's rows land in the same index and Decision 12 shows them
  to the very next query. ❌ Don't reach for a shared-subscriber fan-out — it needs per-subscriber filtering and
  completion, with no second consumer to shape either against.
- **A claim that takes NO ground spawns no walk at all** (`CoverWalk::took_no_ground`), so ❌ never put work a no-ground
  request still owes into `start` or `walk_frontier`. A walk's tail commits the writer, which parks behind every batch
  already queued: seconds behind a first index, spent to commit nothing, while the search that asked sat silent.
  `docs/notes/cover-no-ground-block-2026-08-15.md`.

The claim mechanism and everything bootstrap has to stand up: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
