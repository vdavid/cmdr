This folder contains notes that are not specs, ADRs, or docs on the system. This is pretty much a catch-all folder for
docs that feel helpful and important for some time, but don't belong anywhere else. Like specs, this folder gets wiped
periodically once we made sure that all important information like intent behind features and processes is captured
somewhere else (code or docs).

Some notes here are load-bearing rather than historical. Those are grouped below by what makes them worth keeping.

**Before-and-afters for a crate extraction**, including the method each number was taken with. They're what any future
"did this get slower?" re-measurement compares against, and they record what a crate boundary did and didn't buy.

- `index-extraction-baseline.md` — the `cmdr-index` extraction.
- `archive-extraction-baseline.md` — the `cmdr-archive` extraction, the measurement gate
  `docs/specs/backend-crates-plan.md` ends at. **Its numbers are provisional**: the machine was contended and the volume
  near-full throughout, so most readings are withdrawn and the note carries the procedure for re-taking them.

**Load-bearing for a decision that hasn't been made yet:**

- `importance-treadmill-2026-08-04.md` — what the 60-second rescore treadmill really was, why raising
  `SCOPED_WALK_MAX_DIRS` is refuted, and the measurement (99.88% against 0.03%) behind the signals-not-score equality
  key. Keep it until the open batch-width question in it is settled.

**Load-bearing as regression anchors:**

- `coverage-frontier-query-2026-08-05.md` — the search frontier query measured against its 50 ms warm budget on a real
  658 188-folder root index, plus what it actually scales with and when to revisit the "no new index" call.
- `cover-walk-primitive-2026-08-05.md` — parallel walker against serial reconcile over four real trees, the decision it
  settled for search-driven walks, and what the published "the parallel walk gives up ~10% of rows" caveat actually
  turned out to be.
- `search-arena-row-2026-08-06.md` — what shrinking `SearchEntry` from 56 to 40 bytes actually bought (−92 MiB of arena,
  measured two ways), that it cost no scan latency, and the A/B method for comparing two builds on a machine running
  other work.

**Load-bearing as the input to a job that hasn't been done yet:**

- `flake-corpus-2026-08-08.md` — every test seen failing without a defect behind it, ranked over 48 E2E shard-runs and
  six `rust-tests` contention verdicts, with a cause hypothesis and confidence per entry, plus the structural levers a
  de-flaking pass should reach for. ⚠️ Its E2E counts come from `/tmp` logs that age out, so they can't be re-derived.

- `silent-inertness-hunt-2026-08-08.md` — a sweep for mechanisms that look active but aren't reaching their subject
  (inert guards, tests that can't touch their code, unanswerable questions turned into facts). Carries the two
  capability flags a backend answers wrongly today, the shared `volume::conformance` assertions added to fence the
  class, and the leads that turned up nothing so nobody re-runs them. Keep it until its two open recommendations (a
  mount-kind answer for `LocalPosixVolume`, and measuring FSEvents on an `smbfs` mount) are settled.

**A decision record for something we evaluated and rejected**, kept so nobody re-proposes it from the same premises:

- `size-only-subtrees-rejected-2026-08-06.md` — why storing folder totals instead of per-file rows under `CACHEDIR.TAG`
  subtrees was dropped (the CPU case is ~1%, measured in release), the cross-directory hardlink finding any revival has
  to solve first, and the search-arena and APFS-clone leads that came out of it.
