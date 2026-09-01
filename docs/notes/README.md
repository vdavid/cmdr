This folder contains notes that are not specs, ADRs, or docs on the system. This is pretty much a catch-all folder for
docs that feel helpful and important for some time, but don't belong anywhere else. Like specs, this folder gets wiped
periodically once we made sure that all important information like intent behind features and processes is captured
somewhere else (code or docs).

Some notes here are load-bearing rather than historical. Those are grouped below by what makes them worth keeping.

**Before-and-afters for a crate extraction**, including the method each number was taken with. They're what any future
"did this get slower?" re-measurement compares against, and they record what a crate boundary did and didn't buy.

- `index-extraction-baseline.md` — the `cmdr-index` extraction.
- `archive-extraction-baseline.md` — the `cmdr-archive` extraction, the pilot the backend-crate boundary was measured
  on. **Its numbers are provisional**: the machine was contended and the volume near-full throughout, so most readings
  are withdrawn and the note carries the procedure for re-taking them.

**Load-bearing for a decision that hasn't been made yet:**

- `self-move-to-applications-2026-08-25.md` — whether Cmdr could move itself to Applications instead of only telling the
  user to, measured on macOS 26.5.2 rather than reasoned. **The FDA worry is answered: nothing in TCC records a path**,
  so a moved bundle keeps its grant, and the note carries the four measurements plus the one 30-second check that closes
  the last gap. It also carries the verified move recipe and the step everyone gets wrong (a copied bundle keeps its
  quarantine xattr and is translocated again **even from `/Applications`** until the xattr is stripped). Read it before
  anyone reopens the auto-move question, or repeats the claim that changing the `.app` inode costs FDA.
- `rust-test-flake-analysis-2026-08-23.md` — what actually makes the Rust lanes go red, measured rather than assumed.
  **What predicts a starvation kill is a test's MARGIN (per-test cap ÷ idle runtime), not its duration**: every test two
  saturated full-suite runs killed sits at the thin end of that ratio, while three of the four causes found have no
  duration signal at all. That is a flake predictor, not the speed bar: the standing standard is two seconds on a
  saturated machine (`docs/testing.md` § "A Rust test gets two seconds on a saturated machine"), and no margin ratchet
  is planned. Read this note before anyone seeds a duration allowlist, and read the first section regardless: it carries
  the two ways `~/cmdr-test-log.csv` and `~/cmdr-check-log.csv` mislead a naive top-offenders query.
- `sftp-crate-evaluation-2026-08-22.md`: which Rust crate the SFTP backend gets built on, with each candidate's source
  read rather than its README. **The recommendation is `russh` + `openssh-sftp-client`**, and the reasoning is written
  out so it can be argued with. Read it before anyone proposes `russh-sftp` (the popular default) or a libssh2 binding:
  it carries the two disqualifiers nothing else records (`russh-sftp` mangles non-UTF-8 filenames through
  `from_utf8_lossy`; `libssh-rs` vendors LGPL C that `cargo deny` cannot see), and the measurement that shapes the whole
  backend: **sequential SFTP reads are 4.2 MB/s at 50 ms RTT, a request window is worth 10×, and the window buys nothing
  until `russh`'s 2 MiB channel window is raised too**.
- `ftp-crate-evaluation-2026-08-22.md` — whether an FTP backend is worth building after SFTP, and what it would rest on.
  **The crate question is settled**: `suppaftp` 10.0.2 is the only living Rust FTP client (everything else died between
  2018 and 2022, or is sync-only), it's good enough on all eight axes, and the note carries the signatures and line
  numbers rather than adjectives. **The protocol question is the open one**, and the note argues no: the recommendation
  is SFTP, then WebDAV or S3, with FTP parked behind a request counter. Read it before anyone writes an FTP spec, and
  read the four gotchas regardless (ASCII is the wire default and corrupts files, a dropped data stream wedges the
  connection unless it goes through `abort()`, FTPS session reuse works on rustls and fails on native-tls, and non-UTF-8
  filenames are unaddressable without patching the crate).
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

- `idle-cpu-attribution-2026-08-03.md` — where an idle Cmdr's 110 minutes of CPU over 9.1 hours actually went, and the
  four answers that were wrong on the way (the reconcile drain, the writer, the sync-status probe, and a search arena
  that was being dropped all along). **Read it before profiling this app again**: three of the four came from one method
  bias — a `sample` leaf-frame list that counts scheduler waits and so scores `stat` and `pwrite` as busy CPU — and two
  20-second windows on the same idle process disagreed about which thread dominated. It also carries the two footprint
  blocks nothing has explained yet (643 MB `MALLOC_LARGE`, 947 MB Rust heap) and the proof the first of them is not
  SQLite page cache.
- `idle-malloc-large-clip-towers-2026-08-21.md` — the leading candidate for most of that 643 MB, measured: Core ML
  holding the two CLIP towers costs **307–412 MB of `MALLOC_LARGE` plus 120–176 MB of `MALLOC_SMALL`, from the first
  encode of a session until the process exits**, 80% of it the text tower that enrichment never calls, and all of it
  invisible to `query_mimalloc_heap` because Core ML allocates through the system allocator. The region sizes ARE the
  weight matrices and they sum exactly, which is what turned a size into a name. Carries the method (a per-tag
  region-size histogram is a fingerprint), the 35× compute-unit lever, the clean negative on Vision, the ranked fix
  options none of which were taken, and the one `vmmap` line that confirms or refutes it on a live prod build.
- `listing-row-fetch-quadratic-2026-08-22.md` — why a pane parked at the BOTTOM of a large directory saturates the main
  thread and stops answering IPC, and the proof it is **pre-existing** (`main` reproduces it at the same 3.8× ratio, and
  the whole call path is byte-identical). The MCP pane mirror fetches its visible range one row at a time, each row a
  synchronous `#[tauri::command]` on the main thread doing a linear scan to its index, so one index event costs ~7.4M
  predicate evaluations at the bottom of a 74k listing and nothing at the top. Read it before reading a "URL-scheme
  handler" sample as the asset protocol; on macOS that IS Tauri's IPC transport. Carries the ranked fixes and, at the
  bottom, what shipped on 2026-08-22 with the before/after on a running app.
- `listing-wedge-impact-2026-08-22.md` — the blast-radius companion to the note above: which releases carried each
  escalation (reachable since v0.5.0, driven with no user input since v0.23.0, worst since v0.37.0, still in every
  released build), whether it recovers on its own (it saturates rather than deadlocks, but the user's way out runs
  through the wedged main thread), and what the live feedback loops say. **The answer to "did testers hit it" is that we
  cannot tell and could not have**: a hang is neither an error nor a crash, there is no hang detector, the heartbeat
  keeps beating from a background thread, and one install out of 765 has auto error reporting on. Read it before
  treating a quiet `#error-reports` channel as evidence that a defect did not bite.

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
- `preemption-2026-08-18.md` — what stopping a walk costs (cancel-to-join, the bound the claim table's atomic handoff
  does NOT fix) and what it buys the folder somebody just opened. Read it before widening `YIELD_WAIT`, before assuming
  a stopped walk is free, or before taking preemption to a share: the SMB half of the handover is tested but not timed.
- `wide-dir-scaling-2026-08-18.md` — why a first index over one directory of 60,000 children used to take over an hour
  once something stopped the walk, where covering it uninterrupted takes 3.2 s. It names the stage (the writer's
  ancestor roll-up, `O(width²)` when every child of a wide parent is its own frontier root), proves it three ways
  including an ablation, and carries the before/after curves for the per-burst coalescing that fixed it. It is the only
  place the quadratic, its break-even (~750 children), and the measured linear replacement are written down, and it
  corrects two claims in `preemption-2026-08-18.md`.
- `churn-against-completion-2026-08-15.md` — whether a drive somebody is writing to can stop its first index from ever
  finishing (no: it takes ~200 new folders a second sustained to cost one session's completion marker, and the next
  launch settles the drive in ~2 s). Read it before treating a slow first index on a busy machine as a regression, or
  before proposing a bigger pass budget: it weighs the three ways to close the gap and says why none was taken.
- `live-tick-cost-2026-08-21.md` — what a media live tick costs, split into the coverage gate (45–46 ms per tick at
  90,308 scored folders, from reading `above_threshold` directly once a minute per volume; 2.8 µs from the cache that
  already existed for it) and the scoped walk (~20 µs per touched directory, against 0.03 µs for the filter that now
  replaces it). Read it before assuming a gate is cheap because it guards something expensive: these two were the same
  order, so filtering the walk without fixing the gate would have left the floor where it was. It also records why the
  filtered set has to reach the walk, the GC scope, and the counts patch together, and the two behaviors deliberately
  given up.
- `search-arena-row-2026-08-06.md` — what shrinking `SearchEntry` from 56 to 40 bytes actually bought (−92 MiB of arena,
  measured two ways), that it cost no scan latency, and the A/B method for comparing two builds on a machine running
  other work.
- `cargo-lane-feature-thrash.md` — what it cost when the cargo check lanes asked cargo different questions about one
  `target/` (20-100 s of rebuild per flip), the before/after of aligning them, and the two measurements that ruled out
  splitting the Rust test lane per package.
- `rust-lane-input-narrowing-2026-08-23.md` — what splitting the one `rustInputs` set into per-lane, per-member blocks
  bought (a `tools/` edit went from 44 lanes to 12, a crate edit from 46 to 39), and the re-measurement that closed the
  per-crate `-p` lane question for good: the app depends on every crate, so `--workspace` is already minimal, and a `-p`
  lane compiles `cmdr-fs` without the `testing` feature the workspace build unifies in. Read it before proposing
  per-package cargo lanes or dependency-aware invalidation. The win it pointed at next (`scripts/check/**` in
  `GlobalInputs`, 205 commits that re-ran everything for nothing) has since landed: `scripts/check/DETAILS.md` § "The
  runner's own source".
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

**Design research behind a roadmapped feature nobody has started:**

- `totalcmd-plugin-analysis.md` — the only design artifact for "Add plugins". Covers all four Total Commander plugin
  types, categorizing every plugin in the catalogs A-F, and the bottom third is the actionable part the doc's own
  reading guide points at: which abstraction should own each job, the patterns worth inheriting against the historical
  accidents, and 10 questions that shape a plugin API more than format support does. **Its recommendations are
  subprocess plus JSON-RPC as primary with WASM as a fast lane, one capability manifest instead of four plugin types,
  and a Column-first vertical slice as the first build**, with MCP-shaped-against-bespoke and manifest-against-types
  flagged as the two calls expensive to get wrong. ⚠️ 84 KB, most of it the survey tables backing the priority stats.

- `disk-cleanup-advice-process.md` — how to give disk-cleanup advice without losing the user's trust, from a session
  where an agent got it wrong three times. **The heuristic is to delete only what is BOTH filesystem-idle by mtime and
  process-idle by `pgrep`, and to present candidates with their signals rather than a "safe to delete" bucket**, which
  doubles as the judgment model for the roadmap's disk-space visualizer. Also carries the Cmdr-against-`du` numbers
  (~25-30× on a directory level, mtimes included).

**A decision record for something we evaluated and rejected**, kept so nobody re-proposes it from the same premises:

- `manager-custody-spike-2026-08-18.md` — why `IndexPhase::Running` stays a `Box<IndexManager>` and the
  extract-work-reinsert dance stays: `Arc<Mutex<IndexManager>>` can't give the exclusion up (the closure needs the guard
  for its whole body), so it keeps the same exclusion and pays a lock for it across the blocking scan-start prelude,
  measured at 3-10 ms typical and unbounded on a wedged mount. It also retires none of the three stranding hazards it
  was credited with. Read it before proposing shareable manager custody again, and read § 4 regardless: it carries two
  red tests proving a **fourth** hazard nobody had listed, that a teardown landing in the extraction window
  (`stop_indexing`, `clear_index`, and `fail_index` alike) reports success and does nothing.
- `size-only-subtrees-rejected-2026-08-06.md` — why storing folder totals instead of per-file rows under `CACHEDIR.TAG`
  subtrees was dropped (the CPU case is ~1%, measured in release), the cross-directory hardlink finding any revival has
  to solve first, and the search-arena and APFS-clone leads that came out of it.

**A ledger of known gaps on an unsupported platform:**

- `linux-gaps-2026-08-10.md` — what a Linux user actually hits: the inotify watcher that never starts because one
  unreadable directory aborts the recursive watch, the `Cmd+` menu accelerators that bind to Super rather than Ctrl, and
  the 504 macOS-specific strings in the English catalog. Contributed alongside the Linux `.deb` bundling, and kept
  because Linux isn't advertised, so nothing here has an issue behind it.

**An incident diagnosis kept for the lever it rules out:**

- `smb-credit-stall-2026-09-01.md` — why a 300 GB SMB-to-SMB copy stopped moving bytes 30 seconds into its copy phase.
  **The compound fast-path charged SMB credits for `max_read` (8 MB, 130 credits) rather than for the 4 MB file it was
  reading**, which capped the connection at three concurrent reads against a 512-credit window while the transfer
  launched 10, so seven tasks parked on credits that couldn't arrive. Read it before anyone raises `CREDIT_TARGET`,
  widens the fast-path threshold, or adds adaptive concurrency backoff: the arithmetic says the charge was the lever and
  the other three aren't. Carries the stall dump that confirmed the three-of-10 prediction exactly, the truncation guard
  the fix had to move, and the two things it does NOT explain (the destination's independently slow send side, and a
  Pi-class source ceiling of 7.4 MB/s).
