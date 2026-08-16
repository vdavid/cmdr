# Can somebody writing to a drive keep its first index from ever finishing?

**Short answer: no, and the shape of the "no" is what matters.** Sustained creation of new folders under ground the
index already covers can cost a first index its completion marker for that session, but only at a couple of hundred new
folders a second, and the very next launch settles the drive in about two seconds. There is no state a drive gets into
where it stops converging.

The question came out of `docs/notes/phased-vs-bulk-index-2026-08-14.md` § "`~/Library` re-measured, and one run that
didn't finish", which recorded a `home_bench` run that gave up at its 10-minute patience and offered churn as an
explicit hypothesis rather than a diagnosis. Answered here.

## The mechanism, which is real

Completion is derived: the volume is done when the frontier under its root is empty
(`crates/cmdr-index/src/indexing/lifecycle/phases/completion.rs`). Two things make that a race rather than a
monotonically approaching state:

- **A phase gets two passes and no more** (`MAX_PASSES_PER_PHASE`). Pass one walks what the frontier named; pass two
  picks up what pass one exposed. Past that, whatever is left is this session's loss and the next launch asks again.
  That is the pass budget working as designed, ❌ not a completion rule.
- **A new folder on covered ground becomes frontier.** The branch watcher is live from the first walk
  (`begin_branch_coverage` calls `ensure_branch_watch`), and the live reconciler writes a row for a created directory
  without reading it — so its `listed_epoch` is 0, which the coverage descent classifies as frontier. That is correct:
  nothing has listed it, and a search over it does have to walk it.

Put together: the volume is marked done if some stock-take finds the frontier empty, and whether one does depends on
whether the watcher's next batch of new folders lands before or after the machine's last pass. That is a race, so a
single run reports a coin toss as a law, which is why the bench below repeats every rate.

⚠️ Under a chunk of ground being written to but never new folders — a build rewriting object files in place, a log
growing — nothing here fires at all. **Only new DIRECTORIES matter.** A file under a listed directory is reconciled in
place and never becomes frontier.

## What it takes, measured

`tests::churn_bench::churn_against_completion`, release, over a 300,002-directory / 599,857-entry synthetic tree in
`/private/tmp` (~15 s a launch), 2026-08-15. Each arm writes into one folder that sorts first, so the walk covers and
watches it in the first group and everything after that lands on ground the index already claims — a build directory on
a real disk. Each rate indexes the same tree from nothing at least three times.

How many first indexes marked the drive done, per rate:

- nobody writing: 3 of 3.
- 20 new folders a second (a compile): 3 of 3.
- 60 a second: 3 of 3.
- 200 a second (a package manager unpacking): 0 of 1 on this run, 1 of 4 across every run of the bench at that rate.
- 2,000 folders at once 12 s in, then quiet (a build kicking off): 3 of 3.

- **Twenty and sixty folders a second never cost a completion**, over six trials, even though every one of those runs
  ended with new folders on the frontier (18–114 of them). The marker had already landed at an earlier stock-take; the
  ground that arrived afterwards is the live path's business.
- **Two hundred a second cost it.** The machine gave up after 15.9 s with 181 frontier roots left, all of them new, and
  nothing marking the drive done.
- **A finite burst is absorbed, however big.** 2,000 folders at once, timed to land while the machine is finishing,
  completed 3 of 3. The mop-up pass walks the burst; only ground still ARRIVING during that pass survives it. This is
  the reason a real compile is safe: builds are bursty, and a burst is exactly the case the second pass exists for.
- **The next launch settles it in ~2 s.** With the writing still going at 200 folders a second, the relaunch after the
  failed first index covered end to end in 2.2 s (and 2.7 s on the earlier run of the bench). A resume's frontier is a
  few hundred tiny roots rather than a whole drive, so the machine retires them inside one pass and a stock-take lands
  in a gap between watcher batches. ❌ Nothing here ever needed the writing to stop.

## The run that started this, and what it was

**Not diagnosed, and now diagnosable.** Two things can be said honestly:

- **"A slower run over a bigger tree hitting a fixed patience" does not fit.** That run's early home signal landed at
  38.4 s against the finishing run's 37.5 s, so the machine was not slowed down; a 7.8× slowdown that starts only after
  the early signal doesn't add up.
- **The churn hypothesis is plausible but unproven at that rate.** 267k entries appearing during a run is a cold
  `target/` being written, which is in the right order of magnitude for the 200-folders-a-second boundary above, but
  nobody measured the actual rate and it can't be recovered.

What made it unanswerable is the third thing, and that is fixed: **`home_bench` watched only the marker.** The
stock-take that stamps runs before the machine reports itself idle, so a machine that stops without the marker will
never write one — and the bench could not tell that apart from a slow run, so it sat out its whole patience and filed a
run that probably ended in ~90 s as one that took ten minutes. It now watches the machine and prints the frontier it
stopped with, and `Machine::finish` logs the same fact, so the next occurrence says which of the two it was in one line.

Re-measured the same evening for comparison, with other agents building in sibling worktrees throughout: real `$HOME`
covered end to end in **73.0 s over 5,153,947 entries**, early signal at **34.4 s**. Unchanged in substance from the
76.6 s / 37.5 s reading it sits beside.

## So is it a bug?

**It is a documented limit, and arguably correct behavior.** The frontier genuinely isn't empty: ground exists that no
walk has listed, and the design's own order of preference puts "honest about what's covered" above "reports done
quickly". A rule that declared completion with ground unwalked would be the worse failure.

What the limit costs, when it fires: the drive doesn't reach `Fresh` until something covers the rest, so the badge
doesn't settle and the `scan_completed_at`-gated work (the calibration, the `dir_stats` ledger heal, the shallow-sweep
ledger, rescan routing) waits. ⚠️ Coverage itself is unaffected — everything walked stays walked, sizes and search answer
for it — and the early home signal has already fired, so photo search and folder importance are not waiting on this.

Three options were weighed, and the second was built:

- **A bigger or time-boxed pass budget.** Moves the threshold and doesn't remove the case, at the cost of more walking
  on exactly the machine that is already busy. ❌ Not taken.
- **A backoff retry**: on stopping with a non-empty frontier, restart the phases after 1 / 5 / 15 minutes, the way
  abandoned ground already gets a persisted per-volume backoff. Each attempt is the ~2 s resume measured above, and it
  converges the moment the writing pauses. ✅ **Built** (`crates/cmdr-index/src/indexing/lifecycle/completion_retry.rs`),
  because a drive staying unmarked through a week-long session was judged unacceptable while the same drive settles in
  two seconds at the next launch. In memory rather than persisted, since a relaunch already resumes; the cost is a map
  entry per never-completed volume and the status surface saying "indexing" for two seconds per attempt.
- **Have the live reconciler LIST a directory it creates**, so new ground on a watched branch never becomes frontier at
  all. It would remove the case entirely and it is honest (we really did read it), but it trades a self-healing delay
  for a silent-miss risk on the highest-traffic path in the system: a directory that reads as covered and empty because
  its fill events were coalesced away is a search result nobody ever gets. ❌ Not worth it.

## Re-asking

```sh
CMDR_PHASES_TEST_TREE_DIR=/private/tmp \
  cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
  indexing::lifecycle::phases::tests::churn_bench::churn_against_completion
```

⚠️ `CMDR_CHURN_BENCH_DIRS` is the one parameter that can silently invalidate the whole thing. The tree has to take tens
of seconds to cover: FSEvents coalesces on its own latency, so over a tree small enough to cover in a tenth of a second
not one churn event is delivered before the machine stops, every arm completes, and the bench cheerfully reports that
churn is harmless. Two smoke runs at 2,000 and 5,000 directories did exactly that before the default was raised to
300,000.
