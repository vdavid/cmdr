# The cover walk (filling in what a search can't answer for)

`read/coverage.rs` says what a scope still needs walked; this walks it, writing every row through the volume's normal
writer so the next search over the same ground walks less.

`mod.rs` the walk handle, the frontier loop, and `CoverOutcome`; `ground.rs` the one per-kind branch (`Ground`) and
which primitive takes a root; `bootstrap.rs` what a walk needs before it can start; `live/CLAUDE.md` the claim table
every holder arbitrates through. The registry and phase machine are `../CLAUDE.md`.

## Must-knows

- **A walk reuses the RUNNING writer or stands one up** (`Activation::WriterOnly`, ❌ no scan or watcher), and EVICTS an
  index this build's coverage rules refuse. ⚠️ A volume mid-SCAN isn't walked.
- **A walk stops through a CHILD of the caller's token and flushes before reporting**, cancel included. The child lets
  one walk be stopped without stopping the volume; ⚠️ a stopped walk flushes whatever `FlushOnFinish` its caller chose,
  because its ground changes hands the moment it lets go and the next holder reads the DATABASE to decide what is
  virgin. **`CoverOutcome::abandoned_ground` is independent of every other field**: ❌ any caller
  reporting completeness must consult it.
- **A typed disconnect ends the WHOLE frontier, not one root** (`RootOutcome::VolumeGone`): the roots behind it share
  one session, so re-asking buys a dead round trip each. ⚠️ Skipped is not condemned — nothing walks them,
  so nothing marks them and they stay frontier. ❌ Never widen the trigger past `is_terminal_disconnect`.
- **A walk RELEASES its branch whatever the registry phase** (`finish_branch_coverage` reaches the set directly), ❌
  never behind `with_running_manager`: a walk ending in a `Detached`/`ShuttingDown` window would hold that ground forever.
- **Bootstrap creates the rows a walk needs to START, each at `listed_epoch = 0`; ❌ nothing here claims coverage** —
  the walk earns it, or an ancestor marks a whole tree covered off one walked folder.
- **A missing `entries` row is NOT only a cold-drive case**: a folder created since its parent was listed has none on a
  drive indexed yesterday. ❌ Don't gate bootstrap on "never indexed".
- **Every primitive REPORTS the rows it CREATED and PULSES per directory**, the repair included (`LiveWalk` + a
  `ScanSummary`, ❌ never `(None, Covered)`): a search reads the rest off an arena predating the walk, and reads
  `foldersFound` off the pulse, so a silent one answers short and calls itself complete. ⚠️ Created rows only;
  re-sending a held row doubles it.
- **A holder CLAIMS the ground it writes, and a later one over claimed ground doesn't take it** (`live/CLAUDE.md`, and
  read it before touching arbitration). Two writers over one directory allocate different ids for the same names, and
  `INSERT OR IGNORE` makes the loser lose its whole subtree. A data-safety rule, ❌ not a performance one.
- **`WalkFor` decides two things**: a walk somebody waits on ASKS the background walks holding its ground to hand it
  over, and a background walk hands its own over when asked. ⚠️ `Index::cover` is `TheUser`, the phase machine
  `TheIndex`; ❌ never the waiting form for background work, which stops converging the moment somebody keeps searching.
- **A claim that takes NO ground spawns no walk at all** (`CoverWalk::took_no_ground`), so ❌ never put work a no-ground
  request still owes into `start` or `walk_frontier`: a walk's tail commits the writer, parking behind every queued
  batch — seconds behind a first index, spent to commit nothing (`docs/notes/cover-no-ground-block-2026-08-15.md`).

The walk's stages, the claim mechanism, and everything bootstrap has to stand up: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
