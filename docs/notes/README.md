This folder contains notes that are not specs, ADRs, or docs on the system. This is pretty much a catch-all folder for
docs that feel helpful and important for some time, but don't belong anywhere else. Like specs, this folder gets wiped
periodically once we made sure that all important information like intent behind features and processes is captured
somewhere else (code or docs).

Two notes are load-bearing rather than historical, and both are before-and-afters for a crate extraction, including the
method each number was taken with. Keep them: they're what any future "did this get slower?" re-measurement compares
against, and they record what a crate boundary did and didn't buy.

A third is load-bearing for a decision that hasn't been made yet:

- `importance-treadmill-2026-08-04.md` — what the 60-second rescore treadmill really was, why raising
  `SCOPED_WALK_MAX_DIRS` is refuted, and the measurement (99.88% against 0.03%) behind the signals-not-score equality
  key. Keep it until the open batch-width question in it is settled.

A fourth is load-bearing as a regression anchor:

- `coverage-frontier-query-2026-08-05.md` — the search frontier query measured against its 50 ms warm budget on a real
  658 188-folder root index, plus what it actually scales with and when to revisit the "no new index" call.

- `index-extraction-baseline.md` — the `cmdr-index` extraction.
- `archive-extraction-baseline.md` — the `cmdr-archive` extraction, the measurement gate
  `docs/specs/backend-crates-plan.md` ends at. **Its numbers are provisional**: the machine was contended and the volume
  near-full throughout, so most readings are withdrawn and the note carries the procedure for re-taking them.
