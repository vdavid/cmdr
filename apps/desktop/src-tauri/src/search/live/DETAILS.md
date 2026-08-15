# Live-run reporting details

The event family a live run reports through, and the fold that hands a one-shot caller an answer. Read this before any
non-trivial work here: editing, planning, reorganizing, or advising. How a run is routed, executed, and superseded is
`../CLAUDE.md` and `../DETAILS.md`.

## Decision 10: an agent's search over a one-shot transport

`run_live_collected` is the live run for a caller that can't subscribe to events. Same routing, same coverage question,
same arena, same walk, same matcher; the only difference is `CollectingSink` (`collect.rs`), which folds the event
stream into one `LiveAnswer`. ❌ There is no walk-versus-don't parameter and no agent-specific policy — MCP takes the
same path a person's search does.

What survives the flattening, and why each half matters:

- **The rows are whatever had arrived** when the wait ran out, with the walk's own progress attached, rather than an
  empty list. The fold is bounded by the query's row cap (`ResultStream` emits at most `limit` rows for a whole run),
  so a fold that outlives its reader can't grow.
- **Returning is not cancelling.** `AnswerEnding::StillWalking` says the walk is still going; its rows land in the
  index either way, so the same search run again continues from where it left off. That's Decision 11's reasoning over
  a different transport: walking is coverage work, and coverage work outlives the query that asked for it.

The wait comes from the tool's `maxWaitSeconds` (`AGENT_WAIT_DEFAULT` 20 s, `AGENT_WAIT_MAX` 120 s). It's a transport
knob: it says how much of the walk to wait for, never whether to walk. The MCP reply renders the typed coverage signal
above the results (`mcp/executor/search.rs::coverage_note`), including the two unreadable lists.


## Terminal states

`WalkEnding` is typed because three of the four leave the list incomplete and the copy differs:

- **`NothingToWalk`** — the index covered the scope; no walk ran.
- **`Completed`** — every frontier root this run took was covered.
- **`Interrupted`** — the walk stopped without being asked (an ejected drive, a share that dropped: `CoverOutcome`
  reports `cancelled` and we know we didn't ask), a root couldn't be walked (`roots_covered` short of what it took), or
  the volume couldn't be walked at all. `ScanError::RootUnlistable` is volume-root-only, so a subtree walk needs
  exactly this derivation.
- **`Cancelled`** — Escape, the dialog closing, or the app quitting (`RunEvent::Exit` cancels every run). For an
  agent's run only the last of those applies: handing an MCP caller its answer never cancels the walk behind it.

None of them can leave coverage claiming completeness: a directory is marked listed only once its rows are written
(`scanner/CLAUDE.md` § "Honest-stale, never false-complete"), so a walk cut off anywhere claims only what it read.
App quit needs nothing beyond that — the process dies with the marks unwritten.

**The fourth way short has no ending of its own**: `abandoned_ground` is true alongside any of the four, and
`abandoned_locations` says how much of the drive it is — the given-up-on folders grouped by their parent
(`cmdr_fs::path_locations::location_count`), ❌ never the folder count, since a mount that went to sleep marks every directory a walk
had reached inside it. `0` with the flag true is real (this run's own walk gave up on ground it recorded no path for)
and the note has words for it. Why it stays a count rather than a fourth list of paths: `../DETAILS.md` § The shape.


## What a live result carries, and what it can't

- **No `entry_id`** (`0`): a walked entry has no arena id.
- **No directory size**: `dir_stats` doesn't exist for ground walked a moment ago (Accepted difference 5), and a file's
  size is its own, pre-hardlink-dedup, which is what a listing shows.
- **Arrival order, not rank.** Ranking is a whole-result-set operation; the frontend appends and re-ranks once on
  completion (Decision 8).
- **The cap stops rows, never the walk.** Convergence is the payoff, and a stopped walk would freeze "N so far" at a
  number that never becomes true. `capped` says the rows stopped; the count keeps rising.
