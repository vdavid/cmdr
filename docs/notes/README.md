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

**Load-bearing as the evidence behind a decision that would otherwise look arbitrary:**

- `phased-vs-bulk-index-2026-08-14.md` — the measurement gate the phased-indexing plan set for itself, and the running
  record of what the phased shape costs. **The current number is 1.75×** (the shipped machine over a real `/`, against a
  same-evening bulk baseline of 40.5 s), with `home_covered_at` at 42.5–44.1 s, which is parity with the bulk build's
  entire run. It is in the last section; every earlier figure is the state of knowledge when it was written, so quote
  the last section or nothing. The note also **settles what a first scan really costs** (39–41 s over 6.1–6.2M entries,
  confirmed by running the release app, against the 145–193 s two in-repo comments claimed), and carries the cost
  decomposition the gate call rests on, the time-to-value numbers, the depth-1-against-depth-2 answer, what a resume
  costs, and the one open confounder (a reboot-fresh page cache).
- `index-scope-measurement-2026-08-14.md` — what indexing outside `$HOME` actually costs (15.4% of the entries, ~30 s,
  ~115 MB, against `~/Library`'s 27.7% _inside_ home), and why phased indexing reorders the walk instead of narrowing
  it. Read it before anyone proposes a home-only default again; it names the conditions that would change the answer.

**Load-bearing as regression anchors:**

- `coverage-frontier-query-2026-08-05.md` — the search frontier query measured against its 50 ms warm budget on a real
  658 188-folder root index, plus what it actually scales with and when to revisit the "no new index" call.
- `cover-walk-primitive-2026-08-05.md` — parallel walker against serial reconcile over four real trees, the decision it
  settled for search-driven walks, and what the published "the parallel walk gives up ~10% of rows" caveat actually
  turned out to be.
- `cover-no-ground-block-2026-08-15.md` — what a cover walk that got NO ground used to cost (4.5-5.8 s in the app, 100%
  of it parked on the writer queue, 35 s on a cold drive), measured with the writer-wait split the `Cover:` line now
  carries. Read it before putting work back into `cover::start` or `walk_frontier`. The one cost it names without
  settling, `begin_branch_coverage` registering thousands of frontier roots one at a time, is settled next door in
  `branch-set-cost-2026-08-15.md`.
- `branch-set-cost-2026-08-15.md` — what the branch set cost when it was a self-scanning `Vec` and what it costs as a
  path-keyed `BTreeMap` (87× off registering a 2,500-root frontier, up to 1,231× off a single live event), plus the two
  questions whose cost has to stay bounded by the PATH rather than by the set. Read it before touching
  `watch/branches.rs`: three separate efforts named this as a suspect and none of them measured it.
- `claim-table-cost-2026-08-17.md` — the same `Vec`-to-`BTreeMap` story one level down, for the claim table that keeps
  two walks off each other's ground (200× off taking a 2,500-root frontier, 1,651× off re-asking under a live walk).
  Read it before assuming the claim is too cheap to matter: it was 446.77 ms on the thread a search waits on, an order
  of magnitude above what the plan that ordered the measurement had guessed. It names ~450 ms of
  `cover-no-ground-block-2026-08-15.md`'s unattributed 3.0 s and ❌ does not close that question.
- `churn-against-completion-2026-08-15.md` — whether a drive somebody is writing to can stop its first index from ever
  finishing (no: it takes ~200 new folders a second sustained to cost one session's completion marker, and the next
  launch settles the drive in ~2 s). Read it before treating a slow first index on a busy machine as a regression, or
  before proposing a bigger pass budget: it weighs the three ways to close the gap and says why none was taken.
- `search-arena-row-2026-08-06.md` — what shrinking `SearchEntry` from 56 to 40 bytes actually bought (−92 MiB of arena,
  measured two ways), that it cost no scan latency, and the A/B method for comparing two builds on a machine running
  other work.
- `cargo-lane-feature-thrash.md` — what it cost when the cargo check lanes asked cargo different questions about one
  `target/` (20-100 s of rebuild per flip), the before/after of aligning them, and the two measurements that ruled out
  splitting the Rust test lane per package.
- `frontend-lane-cache-partitioning.md` — what the 21 checks on `svelteInputs` cost (59.6 h of a 24-day window), what
  excluding the colocated agent docs bought (41.3% → 35.0% of commits), the isolated Vitest timings behind it, and the
  three independent reasons a per-area split of `svelte-tests` was rejected.
- `transfer-concurrency-window-bench-2026-08-02.md` — the transfer concurrency window swept 1-32 against a real QNAP and
  Docker Samba: that the window was worth ~14% while a serialized per-file destination probe was worth 74%, both fixes'
  after-numbers, and the rule that a Docker SMB number is a correctness signal and never a latency one.
- `transfer-subtree-concurrency-bench-2026-08-13.md` — what putting one operation-wide window inside the folder-merge
  walk bought (1.79× on loopback for a folder copy that could not overlap at all before), the loose-files-vs-one-folder
  A/B the harness gained to measure it, David's real-hardware curve saying the useful width is 4-8, and the open
  question about a default of 10 that nothing has changed.

**Load-bearing as the input to a job that hasn't been done yet:**

- `flake-corpus-2026-08-08.md` — every test seen failing without a defect behind it, ranked over 48 E2E shard-runs and
  six `rust-tests` contention verdicts, with a cause hypothesis and confidence per entry, plus the structural levers a
  de-flaking pass should reach for. ⚠️ Its E2E counts come from `/tmp` logs that age out, so they can't be re-derived.

- `e2e-flake-remeasured-2026-08-14.md` — the run-level companion: what the widely-quoted "the E2E lane fails 60% of the
  time" is worth now that the fixed-MCP-port and shared-`/tmp` bugs are fixed. Read it before quarantining anything —
  it's the evidence that the red rate is the suite's WIDTH rather than a few offenders (14 failures, 14 distinct tests,
  zero repeats), that the concurrency bugs were worth about one point of it, and that the post-fix sample is still too
  small to quote a new number. Carries the queries and the sample size that would settle it.

- `silent-inertness-hunt-2026-08-08.md` — a sweep for mechanisms that look active but aren't reaching their subject
  (inert guards, tests that can't touch their code, unanswerable questions turned into facts). Carries the two
  capability flags a backend answers wrongly today, the shared `volume::conformance` assertions added to fence the
  class, and the leads that turned up nothing so nobody re-runs them. Keep it until its two open recommendations (a
  mount-kind answer for `LocalPosixVolume`, and measuring FSEvents on an `smbfs` mount) are settled.

**A decision record for something we evaluated and rejected**, kept so nobody re-proposes it from the same premises:

- `size-only-subtrees-rejected-2026-08-06.md` — why storing folder totals instead of per-file rows under `CACHEDIR.TAG`
  subtrees was dropped (the CPU case is ~1%, measured in release), the cross-directory hardlink finding any revival has
  to solve first, and the search-arena and APFS-clone leads that came out of it.

**A ledger of known gaps on an unsupported platform:**

- `linux-gaps-2026-08-10.md` — what a Linux user actually hits: the inotify watcher that never starts because one
  unreadable directory aborts the recursive watch, the `Cmd+` menu accelerators that bind to Super rather than Ctrl, and
  the 504 macOS-specific strings in the English catalog. Contributed alongside the Linux `.deb` bundling, and kept
  because Linux isn't advertised, so nothing here has an issue behind it.
