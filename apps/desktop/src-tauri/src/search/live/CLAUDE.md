# Live-run reporting

What a live search reports while it runs (`events.rs`: the event family plus the sink trait, so the run never touches
Tauri) and how a caller that can't subscribe takes the same run as one answer (`collect.rs`). The run registry,
`ResultStream`, and routing are `../CLAUDE.md`.

## Must-knows

- **Every event carries its run id, and the run id is ❌ NOT a cancellation** (Decision 11). A superseded run's WALK
  keeps going: walking is coverage work, matching is query work, and the frontend just drops events for a query it has
  moved on from.
- **MCP takes the SAME run, the same walk, and the same events, folded by `CollectingSink`** (Decision 10). ❌ No
  walk-versus-don't parameter: the wait is a transport budget saying how much of the walk to wait for, never whether to
  walk.
- **Handing back an answer is not a cancel** (`AnswerEnding::StillWalking`): the rows already land in the index, and the
  same search run again picks up where this one left off. Only the app quitting stops it.
- **The fold returns whatever HAD arrived, ❌ never "nothing until it's done"**, with the walk's own progress attached.
  Memory stays bounded by the query's row cap.
- **Four phases, ❌ not one spinner**: resolving coverage can be a multi-second arena load, waiting for another walk is
  somebody else's clock, the index read is fast, and the walk is unbounded. Keep them distinguishable in the event.
- **The phase is what the run IS doing, ❌ never inferred from how it ended.** A walk holding no ground
  (`walk_phase(0)`) and a run parked in `wait_for_the_other_walk` are `WaitingForAnotherWalk`; the terminal event
  repeats the run's LAST phase. Say `Walking` for a run that never walked and the dialog reads "0 folders scanned" under
  a sentence about walking, for however long the other walk takes. DETAILS § What a run says it's doing.

The one-shot fold, the terminal events, and what a live row can't carry: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
