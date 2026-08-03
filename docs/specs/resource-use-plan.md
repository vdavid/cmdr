# Cutting Cmdr's idle CPU and RAM

Prod v0.37.0, measured 2026-08-03 on David's machine: **93 minutes of CPU over 7.6 hours** (83% of a core at the moment
of sampling), **1.78 GB physical footprint** (2.8 GB peak), and **141,072 log lines / 28 MB in six hours**. The app was
idle the whole time. Principle 5 says respect the user's resources; this is a long way from that.

Five workstreams. They are independent enough to land separately, and each one is worth landing on its own.

## The evidence, once, so no milestone re-derives it

Method: `sample` on the live prod process, `footprint`/`vmmap` per `docs/tooling/memory-debugging.md`, `lsof`, and the
app's own log at `~/Library/Logs/com.veszelovszki.cmdr/`. A read-only copy of `index-root.db` was queried for row
counts.

- **7,007,762 rows** in the root index. **974,485 of them (14%) under `.claude/worktrees/`.** 26,536 `node_modules`
  directories, 131 `target` directories.
- **3,704 distinct rescan anchors in eight hours; 3,438 (93%) under `.claude/worktrees/*/target`**, across **27,617**
  queue events. The hot ones are `target/debug/incremental/<crate>-<hash>/s-<hash>-<hash>-<hash>`.
- Churn windows that crossed budget today: `108s`, `156s`, `79s`, `72s`, `44s`, `7s` of walking per 15 minutes. Six
  windows. `docs/tooling/logging.md` says silence is the expected state.
- Media live tick, every 60 s, forever: `live tick 'root': 0 enriched, 0 GC'd across 5,492 touched dir(s)`. 586 of 3,425
  samples on one blocking thread inside `run_live_tick_blocking` → `walk_image_entries_in_dirs` → `list_children_on` →
  `sqlite3_step`.
- **132 open SQLite connections** across 71 threads: 60 × `index-root.db`, 28 × `index-smb-…naspi.db`, 24 ×
  `importance-root.db`, 13 × `importance-smb`, 5 × `media-root.db`, 2 × `operation-log.db`.
- Footprint split: Rust heap (mimalloc, shown as `IOAccelerator`) 947 MB dirty + 725 MB reclaimable; system C heap
  `MALLOC_LARGE` 643 MB + `MALLOC_SMALL` 152 MB. WebKit and the compositor under 4 MB, not involved, consistent with
  both earlier investigations.
- Log volume by target over six hours: 32,479 `rescan`, 19,705 `reconciler`, 15,405 `sync_status`, 13,133 `writer`,
  8,590 `smb2::client::tree`, 7,694 `stall_probe`, 6,514 `space_poller`, 5,911 SMB `WARN`.

## The constraint that shapes every design here

**No denylists. No path-shaped exclusions for build output.** David's call, and it's the right one: a user may run any
obscure tool that churns hard, and the app must recognize and throttle that, not carry a list of the ones we happened to
think of. Every mechanism below has to work having never heard of cargo.

`importance/classify.rs`'s `is_denylisted` stays exactly where it is and keeps doing what it does (floor a folder's
*ranking*). It does not grow a second job.

---

## M1: Bound aggregate reconcile cost, not just per-anchor cost

The one that actually moves the CPU number. Do it first; M3 and M4 both shrink once it lands.

### Why the existing throttle doesn't fire

It works. That's the finding. `RescanThrottle` (`reconciler/rescan_throttle.rs`) gives each anchor a cost-proportional
window (`30 × walk_cost`, clamped 60 s–30 min), so one anchor spends at most ~1/30th of the time re-walking itself. The
churn log confirms it: the top anchors in every window show **"1 walk" or "2 walks"**.

The problem is that **every gate keys on the anchor path**, and cargo mints a fresh unique path per build session. With
N distinct anchors, aggregate cost is N/30 and nothing bounds N. The single-flight drain caps it at 100% of one
`Utility`-QoS thread, which is exactly what we measure.

`reconcile/DETAILS.md:483` already writes the general form of this, about an Electron updater: *"The per-subtree
throttle cannot catch this, and no tuning of it would: its signal is REPETITION, and every path is unique."* The answer
chosen then was the 30 s birthtime settle delay, which worked because those directories **vanished** before settling.
Cargo's `s-<hash>` dirs persist, settle, and get walked. Same shape, different lifetime, and the instrument doesn't
reach.

**What's missing is a governor above the anchor.** Every existing bound is per-path; nothing is per-volume.

### Two corrections to assumptions I carried into this plan

Both matter, and the implementing agent should not inherit my errors:

1. **`rescan_churn.rs` does NOT roll up an ancestor chain.** It is flat per-anchor
   (`record_reconcile(anchor, walk_cost, rows)`), with a 64-entry cap and cheapest-eviction (`MAX_TRACKED_ANCHORS`, and
   the `64+ anchors` in the log line is that cap, not a count). The ancestor rollup I remembered lives in
   `watch/churn_monitor.rs`, a different instrument, gated off by default behind `CMDR_CHURN_SPIKE`, and it rolls up for
   **ranking**, not governing.
2. **`cost_budget.rs` argues explicitly AGAINST charging cost up the whole ancestor chain.** Read
   `local_reconcile/cost_budget.rs:23` before designing:

   > ❌ Don't give every directory its own accumulator by charging each read up its whole ancestor chain. A fraction
   > needs a SAMPLE, and most directories are a handful of reads; per-directory fractions would be noise that the floors
   > would then have to suppress one by one, and the unit refused would become "whichever depth tripped first", which is
   > neither predictable nor explainable.

   Its answer is **one accumulator at a fixed depth** (`ANCHOR_DEPTH = 5` below the volume root), giving a sample worth
   taking a statistic over, a refusal unit that is a property of the subtree alone, and O(1) work per read.

So "charge it up the ancestor chain" is not the house pattern and should not be adopted without arguing past that ❌.

### The design pass (do this before writing code)

This milestone earns a real design pass, written into `reconcile/DETAILS.md` as a Decision/Why. Three candidate shapes,
and the agent picks with reasoning, not preference:

- **(a) Fixed-depth cost accumulator, the `cost_budget` pattern.** Charge each completed walk's cost to its ancestor at
  a fixed depth below the volume root. When that accumulator's cost in a rolling window crosses a budget, the whole
  subtree under it backs off. Predictable and explainable, O(1), and it composes with the existing per-anchor window
  (whichever says "not yet" wins, exactly as settle and window already compose). Open question the agent must answer:
  `ANCHOR_DEPTH = 5` puts the anchor at `~/projects-git/vdavid/cmdr`, which is the whole repo, so editing
  `apps/desktop/src` would be throttled by a cargo build. Is that acceptable (it IS one project churning), or does the
  event path want a different depth or a different anchoring rule than the serial walk?
- **(b) A volume-wide duty-cycle budget.** One accumulator per volume: the drain may spend at most X% of wall clock
  walking. Simplest to reason about and to test, directly expresses "at most 1/30th of the machine", and is immune to
  anchor cardinality by construction. Weakness: it's global, so a genuinely important subtree can be starved by an
  unimportant one; needs a fairness or priority story so an anchor the user is looking at isn't stuck behind churn.
- **(c) Both, layered.** A volume-wide ceiling as the hard backstop, plus a coarser-than-per-anchor accumulator for
  attribution so the back-off lands on the subtree responsible rather than on everyone.

My read, for the agent to accept or reject: **(c), with (b) as the load-bearing half**, because (b) is the only one whose
guarantee doesn't depend on getting an anchoring rule right, and the plan's whole premise is that path-keyed reasoning is
what failed. But (b) alone will starve something eventually, so it needs (a)'s attribution to decide *who* waits.

Whatever is chosen must be argued against the two ❌s above and recorded.

### Constraints the design must honor

- **Composes as a further eligibility gate.** `RescanThrottle::is_eligible` already answers to two independent absolute
  deadlines (settle, cost window) and whichever says "not yet" wins. A third must behave the same way and must not be
  able to starve an anchor permanently: whatever the budget refuses now must become eligible later on its own.
- **The pure/clock-injected discipline holds.** `RescanThrottle` and `RescanChurnWindow` make no filesystem, clock, or
  logging calls; `now` is passed in. Keep that, so every rule stays deterministically unit-testable. Any `stat` lives at
  the call site, as `rescan_settle` already does.
- **The hourglass hold must stay honest** (`rescan_hold.rs`): an anchor holds iff it is walking, or queued AND eligible.
  A budget-refused anchor is *not* eligible, so it must hold nothing, or `~` and `/` show "size updating" indefinitely.
  This is the invariant most likely to break silently; test it explicitly.
- **Depth-split routing is upstream** (`rescan_route.rs`): shallow (`depth ≤ 2`) anchors never reach `pending_rescans`
  at all, so a governor on the drain does not see them. Decide deliberately whether shallow anchors are in or out of the
  budget, and say why.
- **`gc` measures each record against its OWN window.** A global window frees a backed-off anchor early. Any new state
  needs the same discipline and the same bounding.

### Tests

TDD, red first, and see it fail for the right reason. This is risky logic in a hot path.

- **Unit, test-first**: the governor engine, pure and clock-injected, in its own module's `tests`. The characteristic
  case: **N distinct anchors, each walked exactly once, aggregate cost over budget**. This is the case the current code
  cannot express, so write it against the current code first and watch it pass wrongly (or fail to compile), then make
  it meaningful. Plus: budget refusal expires on its own; a refused anchor holds no hourglass; `gc` bounds the new map;
  an anchor the user navigates to isn't starved.
- **Integration**: extend `reconcile/reconciler/tests/live_events.rs` with a many-unique-anchors storm and assert the
  drain's aggregate walk time stays under budget across the window. There is an existing storm/stress fixture
  (`disable_rescan_throttle_for_test`, `set_settle_delay`) to build on.
- **Regression anchor**: a test named for this bug's shape, so a future tuning pass can't quietly reintroduce
  cardinality blindness.

### Docs

- `reconcile/DETAILS.md`: the design pass, the rejected alternatives, and why the ancestor-chain shape was or wasn't
  adopted (must engage `cost_budget.rs:23`).
- `reconcile/CLAUDE.md`: one guardrail line, in the style of the existing throttle bullets.
- Correct any doc that implies `rescan_churn` rolls up ancestors.

### Checks

`pnpm check rust`, then `pnpm check -q`, then `pnpm check --include-slow` before wrapping.

---

## M2: Make the SQLite page-cache bound real

Independent of everything else. The only milestone that touches memory.

### The gap

`SHARED_PAGE_CACHE_BYTES` (`crates/cmdr-fs/src/sqlite_util.rs:39`) is a 64 MiB slab handed to SQLite via
`SQLITE_CONFIG_PAGECACHE`, and its docstring claims:

> total page memory is THIS number no matter how many connections exist

That isn't what SQLite does. `pcache1` falls back to plain `sqlite3Malloc` when the slab is exhausted, counted as
`SQLITE_STATUS_PAGECACHE_OVERFLOW`, which the crate's own test comment at `sqlite_util/tests.rs:61` already names.
Nothing bounds the overflow, and there is no `sqlite3_soft_heap_limit64` or `SQLITE_CONFIG_HEAP` call anywhere in the
tree. So the real ceiling is back to `connections × cache_size` = 132 × 8 MiB ≈ 1 GB, against ~800 MB observed.

The fix works here specifically because the bundled build defines `SQLITE_ENABLE_MEMORY_MANAGEMENT` (verified:
`libsqlite3-sys-0.37.0/build.rs:135`, and `sqlite3_soft_heap_limit` / `sqlite3_release_memory` are both present in the
shipped binary). That define puts all caches in one `PGroup`, which is what lets a soft limit reclaim across every
connection rather than only the one that tripped it. The existing `READ_PAGE_CACHE_KIB` docstring already relies on that
same one-`PGroup` property, so this is consistent with the design, not a new assumption.

### Sequencing: measure, then size

**Do M2a before M2b.** Sizing the limit by guessing repeats the mistake that produced the current false claim.

- **M2a, the readout**: Expose `sqlite3_status64` for `SQLITE_STATUS_PAGECACHE_USED`, `PAGECACHE_OVERFLOW`, and
  `MEMORY_USED` on a dev-only diagnostic surface. Run a dev build under real load and record actual overflow. This also
  attributes the **unexplained ~650 MB**: `MALLOC_LARGE` regions are 9 MB and 2.25 MB each, far larger than a 4 KB page
  allocation, so some of that is NOT page cache and I could not name it from outside the process. Do not assume it's
  SQLite. If the readout shows overflow is small, that is a finding, and M2b's sizing changes accordingly; say so
  rather than proceeding as if the hypothesis held.
- **M2b, the limit**: Add `sqlite3_soft_heap_limit64` beside `install_shared_page_cache`, ordered like the slab (before
  SQLite initializes, from the same connection factories, so "installed first" stays true by construction and
  `desktop-rust-sqlite-open-direct` keeps it that way). Size it from M2a's numbers. **Rewrite the false docstring** at
  `sqlite_util.rs:26` and `:39`; leave a doc line that says what the slab bounds and what the limit bounds, because the
  two are different and the current text conflates them.

### Where the numbers should live

`docs/notes/idle-memory-profile-2026-07-28.md` is the canonical prior investigation and it is the natural home for the
follow-up. Add the M2a measurement there (or a dated sibling note) rather than inventing a third place, and link it from
`indexing/store/DETAILS.md` § "SQLite page memory is one process-wide slab", which currently states the bound this
milestone is correcting.

### Tests

- **Test-first**: a test that opens many read connections, drives real queries, and asserts total SQLite page memory
  stays under the limit. Against current code it fails (that's the red), because nothing bounds overflow. The existing
  `cached_pages_come_from_the_shared_slab` test is the pattern to copy, and `sqlite_status` is already available in that
  test module.
- Assert the limit is installed before the first connection, mirroring
  `the_shared_page_cache_is_installed_before_any_connection_opens`.
- Do **not** assert an exact byte number a future tuning pass would have to chase; assert the bound holds.

### Checks

`pnpm check rust`. Watch for the `file-length` and `claude-md-length` allowlists: do not add or raise entries
(`.claude/rules/file-length-allowlist.md`), surface a warn to David instead.

---

## M3: Stop the media live tick doing SQL for nothing

Small. Partly evaporates once M1 lands, because the touched-dir set collapses. Worth doing anyway: the ordering is
wrong on its own merits, and M1 reduces the input without fixing it.

`run_live_tick_blocking` (`media_index/scheduler/live.rs:124`) walks first and gates second:

- line 138: `.with_conn(|conn| walk_image_entries_in_dirs(conn, touched_dirs))`, one `resolve_path` plus one
  `list_children_on` **per touched dir**, thousands per tick
- lines 151–152: `gate::importance_threshold()` and `folder_scores(...)`, the coverage gates that decide the dirs were
  never eligible

Move the gate ahead of the walk so an ineligible dir costs no SQL. `walk_image_entries_in_dirs`
(`scheduler/enrich.rs:226`) takes `&HashSet<String>`, so the natural shape is to filter that set before the call rather
than to change the walk.

**Watch for**: the full pass and the live tick share these gates deliberately (`live.rs:147` says "the SAME coverage
gates as the full pass"). Keep them the same; if the ordering change makes the two paths diverge, that's a bug, not a
saving. Also confirm whether `folder_scores` itself is cheap enough to run per tick, or whether the gate needs its own
cheap pre-filter. Measure, do not assume.

**Tests**: test-first unit test asserting a tick over N ineligible dirs issues zero `list_children_on` calls (the
`CountingOpener` idiom in `sqlite_util/tests.rs` shows how the repo counts calls rather than timing them). Plus a test
that an eligible dir still enriches, so the gate can't be made trivially "correct" by gating everything out.

**Docs**: `media_index/scheduler/DETAILS.md` (or nearest): a Decision/Why on gate-before-walk.

---

## M4: Cut log volume to ~0.1% of current

Small and mechanical, but the file sink is unconditionally DEBUG and 141k lines in six hours is a real cost in
formatting and IO. Do this **after** M1, because M1 removes most of the lines and the remaining shape will be clearer.

- `reconciler.rs:1193` and `:1265`: `reconcile: can't read {path}: {e}`, 19,705 lines. This is the expected race with a
  compiler deleting files mid-walk, not a diagnosis. **Count them per walk and fold the count into the existing
  reconcile summary; individual lines to TRACE.**
- `rescan.rs`: `MustScanSubDirs for {path} queued (rescan already active)`, 32,479 lines. It already dedups
  (`[+N identical suppressed]`) but **per exact path**, which 3,704 unique paths defeat. **Replace with a per-window
  counter on the churn line** the code already emits: "queued 27,617 signals across 3,704 anchors". This makes the churn
  line strictly more informative while removing the volume.
- `file_system/sync_status/service.rs:175`: 15,405 lines, one per resolve. To TRACE.
- `space_poller.rs` per-tick lines: 6,514. To TRACE. (M5 may remove most of them anyway.)

**Do not** silence anything that would hide a real regression. The `held_back` counter on the churn line exists
precisely so that "a window that churns hard while that reads zero" is detectable
(`docs/tooling/logging.md`); the new anchor-cardinality counter should serve the same purpose for M1's governor.

**Docs**: update `docs/tooling/logging.md` § "The reconcile churn line" for the new fields, since it documents the line's
exact shape and how to read it.

**Verification**: run the app for a while post-change and count lines per hour. State the actual before/after number;
"should be quieter" is not a result.

---

## M5: Space info: subscribe if we can, back off honestly if we can't

`space_poller.rs` polls `get_volume_space()` per watched volume: local every 2 s, network/MTP every 5 s, driven by
`Volume::space_poll_interval()`. Against the NAS that's an `fs_info` round trip every five seconds forever (6,514 log
lines), plus a permanent boot-volume watcher for the low-disk-space warning. `AGENTS.md` says subscribe, don't poll.

**Investigate first, and report the finding even if it's negative:**

- Local volumes: is there a real notification (DiskArbitration, an FSEvents-derived signal, `statfs` on a volume-change
  event)? Free space changes when files change, and we already watch files.
- SMB: SMB2 has no `fs_info` subscription. Confirm rather than assume, but expect a negative.

**If no subscription exists (the likely outcome), back the poll off honestly rather than pretending:**

- Adaptive interval: poll fast while the value is moving, decay toward a long interval while it's stable. A NAS whose
  free space hasn't changed in an hour does not need a five-second question.
- Don't poll a volume no pane is showing, and consider not polling while the window is hidden or unfocused. The
  low-disk-space watcher is the deliberate exception; keep it, and give it its own (slow) cadence rather than the pane
  cadence.
- Document why polling remains, at the module doc, with the evidence-anchor format
  (`.claude/rules/docs.md`): a claim about OS/protocol behavior needs `(verified on <version>, <method>, <date>)`.

**Careful**: the low-disk-space hysteresis detector and the `volume-space-changed` stream that feeds the live numbers
behind the toast both ride this loop (`space_poller.rs:10-19`). Backing off must not make the toast's numbers stale
while it's on screen. That's a user-visible behavior, so it needs a test, and David reviews anything user-facing.

**Tests**: unit tests on the interval-decay policy (pure, clock-injected, same discipline as the throttles). A test that
a volume under active change keeps the fast cadence.

---

## Out of scope, tracked elsewhere

The SMB `ChangeNotify` long-poll liveness bound and the 5,911 sweeper `WARN`s are being handled by a dedicated agent in
`~/projects-git/vdavid/smb2`, serving that library's interests first. Cmdr will consume the resulting release
afterwards. Leave the seam: don't work around the warning volume on Cmdr's side, and don't pin `smb2` in a way that
makes picking up the fix awkward.

Diagnosis for reference: `is_long_poll(ChangeNotify)` (`connection.rs:341`) correctly exempts notifies from the request
deadline, and the docstring at `:335` says they're "bounded by the connection instead of by themselves". Every liveness
verdict is connection-level (`liveness_is_proven`, `unresponsive_for`, `LIVENESS_WINDOW_PROBES = 3`,
`ALIVE_DEADLINE_FACTOR = 6`). When the connection is healthy and only the long-poll is dead, that means bounded by
nothing. Measured: `fs_info` round-tripping in 4 ms and Echo `msg_id` climbing steadily while two `ChangeNotify`s sat
unanswered for 6,186 seconds.

---

## Sequencing and parallelism

**Sequential by default.** We're not in a hurry, and M1 changes the input to M3 and M4.

1. **M2** first if you want an early independent win: it touches only `cmdr-fs` and shares no files with anything else.
   M2a (measure) then M2b (limit).
2. **M1**: the big one. Design pass, then TDD.
3. **M3**: after M1, so the ordering fix is measured against the reduced input.
4. **M4**: after M1, so the remaining log shape is clear.
5. **M5**: independent of all of the above; can go any time.

**Safe to parallelize**: M2 and M5 touch disjoint files (`crates/cmdr-fs/` and `apps/desktop/src-tauri/space_poller.rs`)
and can run alongside M1. **Do not** parallelize M3 or M4 with M1; both read state M1 is changing.

## Definition of done

Not "the code compiles". The whole point is a number:

- Re-measure CPU over a comparable idle period (`ps -o time`) and footprint (`footprint -p`), same method as the
  evidence section. **State the before/after.**
- Re-count log lines per hour.
- `pnpm check --include-slow` green.
- Colocated `CLAUDE.md` / `DETAILS.md` updated per `.claude/rules/docs.md`, with Decision/Why entries where a design
  choice was made.
- No new `file-length` or `claude-md-length` allowlist entries without David's explicit consent.

A caveat worth carrying: the evidence above was gathered on a machine running six Cmdr worktrees with active cargo
builds. That's a heavy case, not an unrepresentative one (it's a real user's real machine), but the fixes should be
sanity-checked against a quiet machine too, so we don't tune for one workload.
