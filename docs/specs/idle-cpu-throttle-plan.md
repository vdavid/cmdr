# Throttle live filesystem-event handling to stop excessive idle CPU (#37)

Status: planned (branch `david/idle-cpu-throttle`, worktree `.claude/worktrees/idle-cpu-throttle`). Owner: David +
agent. Issue: [#37](https://github.com/vdavid/cmdr/issues/37).

## Problem

A user reports Cmdr burning CPU **constantly while idle**, right after launch, doing nothing in the background
(`indexingEnabled: true`, app v0.33.0, ~285 MB index). David tagged it a regression.

> **Baseline correction (important, 2026-07-14).** The original profiling below **missed a larger, separate idle-CPU
> consumer**: the importance scheduler was running a full whole-volume recompute of the root index (~750k folders,
> re-score + full-table REPLACE, ~100 s) back-to-back every ~2 min, pegging a core continuously. Root cause: every live
> `dir-changed` batch carries the bare root `/` (the universal ancestor added by `reconciler::collect_ancestor_paths`),
> and the importance incremental treated any `/` in a batch as a full-refresh sentinel → continuous full recomputes.
> **Fixed on `main` in `cc6681c00`** (drop `/` at the incremental boundary + throttle incrementals to ≤1 walk/60 s).
> That loop ran on `spawn_blocking` tasks a quick sample can lump into "background indexing", so it was almost certainly
> a **bigger** idle-CPU cost than the per-event reconcile+write pipeline this plan targets. This plan is
> **complementary** — it throttles the index-**write** layer; `cc6681c00` fixed the importance-**recompute** layer. Both
> were real. **Do NOT credit this throttle for the idle-CPU drop `cc6681c00` already delivered:** capture a fresh
> before/after baseline on top of `cc6681c00` (M0 below). The `stall_probe::sqlite_busy` WARN bursts were a downstream
> symptom of the importance loop starving the WAL checkpoint and are already gone — not a separate pipeline problem.

**Measured root cause** (profiled a live dev instance, 2026-07-14; log + `vmmap`/`footprint`) — the per-event layer,
i.e. what remains for this plan after `cc6681c00`:

- Cmdr live-watches the **entire boot volume** with file-level FSEvents and reconciles the write activity of the _whole
  machine_. On any running Mac, background apps write constantly (browser caches, app-state SQLite DBs, preferences,
  temp files, lock files, build artifacts), so the reconciler + writer never go idle. The dev instance showed thousands
  of live FS events/minute at "idle" and the writer doing continuous upserts.
- By elimination the CPU is the **per-event reconcile + write pipeline**, NOT:
  - the FSEvents watching itself (kernel-side; receiving batches is cheap), nor
  - the search index (it is _dropped_ at idle by the dialog idle/backstop timers and only reloaded on an actual search;
    write-generation bumps don't trigger eager reloads). The 100 MB search-index prealloc is irrelevant at idle.
- The amplifier is the writer's **ancestor-chain delta propagation**: one deep file change writes `dir_stats` up every
  ancestor level to `/` (~8-10 writes per change).

The RSS the reporter saw (~1.4 GB) is a **separate, largely non-indexing** story: on the profiled machine
`phys_footprint` was ~251 MB (peak 665), while raw RSS was inflated by ~662 MB of **IOAccelerator (WebView GPU)
surfaces** plus ~850 MB of shared library pages. So the memory half is mostly RSS-accounting + WebView GPU, not an
indexing leak. **This plan fixes the CPU. Memory/GPU is a separate note (see § Out of scope).**

### The self-write sub-loop (real, but minor)

Cmdr's own data dir (`~/Library/Application Support/com.veszelovszki.cmdr*/`: index DB `-wal`/`-shm`, `logs/cmdr.log`)
lives inside the watched, indexed tree and is not excluded, so writing the index/log itself generates FSEvents → upserts
→ more writes. It's real but the profiling showed it's a small tributary (<1% of sampled churn), not the main river.
**Global throttling subsumes it**: Cmdr's DB is just another rapidly-rewritten file and gets throttled like any other,
so we do **not** special-case it (David's call). A nicer self-specific tooltip is a possible later refinement, not part
of this.

## Two churn patterns → two throttles

The firehose splits into two shapes, and only the first is fixed by a per-file throttle:

1. **Same file rewritten rapidly** — a WAL, a log, an app-state SQLite DB, a plist rewritten in place. The _same entry_
   upserts over and over. → **Per-file throttle** (Milestone 1).
2. **Many short-lived files created/deleted in one directory** — a browser cache dir, `/tmp`, a build `target/`. Each
   individual file is touched once or twice before it's gone, so a per-file throttle never trips; the cost is the sheer
   volume of distinct create/delete events in that dir. → **Per-hot-directory coalescing** (Milestone 3), which
   accumulates child churn in memory and flushes a netted-out roll-up once per window.

David's design stance, which this plan follows: **throttle, don't exclude.** We keep tracking every file's size (nothing
is dropped from the index); hot entities just update less often and are shown as "approximate". No new exclusion
prefixes.

## Design

### Core mechanism: leading + trailing throttle (not debounce)

Per throttled key, a 60 s window:

- **Leading edge:** the first change applies immediately (feels instant for normal edits).
- **Within the window:** further changes are suppressed, but a **trailing flush** is scheduled.
- **Trailing edge (window end):** read the entity's _current_ size once and apply it.

Under continuous change this fires **every 60 s forever** — that's the throttle guarantee. Debounce (wait-for-quiet)
would never fire under sustained change; we explicitly reject it. We only ever need the latest size, so "coalesce" =
"read current size at flush time", never a queue of intermediate values.

**Window:** `THROTTLE_WINDOW = 60 s`, both patterns (David's call).

### Significant-change bypass (2% + 512 KB floor)

A big jump should apply immediately even mid-window. On each change:

```
significant(new, last_applied) =
    (last_applied == 0 && new != 0)                    // empty → nonzero transition only
    OR |new - last_applied| >= max(2% of last_applied, 512 KB)   // FLOOR = 512 KB (David's call)
```

If `significant`, apply now (and reset the window). Else throttle. The **512 KB floor** stops tiny files (a 200-byte
lock file flapping) from bypassing constantly and re-opening a mini-loop; the **2%** lets a genuinely growing file
surface promptly. Applies to both patterns.

- **Predicate gotcha (fixed):** the empty-baseline clause must be `last_applied == 0 && new != 0`, NOT just
  `last_applied == 0`. A 0-byte lock/marker file touched repeatedly has `last_applied == 0` AND `new == 0`; the naive
  clause flags it significant every event → never throttled → exactly the mini-loop the floor targets stays alive.
- **Known cost (accepted, thresholds locked):** for a file growing steadily by >2% (or >512 KB under ~25 MB) per ~1 s
  batch — a VM disk image, a Docker layer, a torrent, a build artifact — the bypass fires every batch, so that file pays
  full ancestor-propagation cost and isn't throttled. This is the "idle to the user, busy on disk" case that can still
  burn CPU. Acceptable for now; noted so it isn't mistaken for pure upside.

### What's cheaply available (verified)

`handle_creation_or_modification` already calls `symlink_metadata` and extracts `logical_size`/`physical_size` before
the upsert, so **new size is free** at event time. The **old size is NOT read** in the live path (the upsert is blind;
the writer computes the delta). So we do **not** add a DB read; the throttle map holds the **last-applied size** as the
baseline — it _is_ what we last wrote, the correct comparison point. This keeps the "no extra DB reads during a storm"
invariant.

**Never re-stat at flush time (data-safety + dead-mount safety).** The trailing flush must apply the size from the
already-stat'd _suppressed_ event, not stat again on the timer. Re-statting on the `select!` sweep would (a) reintroduce
a bare filesystem syscall on a timer that could block the live loop indefinitely on a dead/dataless mount (the exact
hazard the sibling guarded-scan plan exists to fix — a `~/Library/CloudStorage` dataless file), and (b) add a "file
deleted between last event and sweep" phantom-apply case. Carrying the last-seen size makes the trailing edge pure,
FS-free, testable, and strictly more correct.

### Throttle state model (per key)

The key is the **normalized path `String`** (the live path has the path in hand; `entry_id` would need a resolve). Each
entry holds:

- `last_applied_at: Instant` — when we last wrote this key's size.
- `last_applied_size: u64` — the baseline for the significance test (= what's in the DB).
- `pending: Option<u64>` — `Some(last_seen_size)` if a change was **suppressed** since the last apply and still needs a
  trailing flush; `None` if nothing is outstanding.

`(last_applied_at, last_applied_size)` alone is insufficient: the sweep can't tell "leading edge applied, nothing
suppressed" from "a change is suppressed, must flush." The `pending` field carries both the dirty bit and the size to
flush.

**Data-safety invariant (red test):** a key with `pending == Some(_)` is **never evictable**. Cold-eviction may only
drop keys with `pending == None` that are past N quiet windows. Otherwise: file changes to final size B at t=30 s
(suppressed → `pending = Some(B)`), goes silent; if eviction drops the key before the window's trailing flush, B is
**never written** and the index shows a permanently-wrong size with no correction. Pin this with a test:
suppress-then-evict must still flush B.

Map is bounded by the count of actively-churning keys (path-sized, not 8 bytes); evict cold `pending == None` keys and
`log()` if a hard cap forces eviction of anything (no silent cap).

### Where the state and the timer live

- **State home:** `EventReconciler` (`reconciler.rs`), which already owns live state (pending MustScanSubDirs, live vs
  replay mode). Add the throttle map here.
- **Insertion point is a signature change across a SHARED path, not a one-liner (S2).** The live upsert flows
  `process_live_event` (method) → `process_fs_event` (free fn) → `handle_creation_or_modification` (free fn) →
  `writer.send(UpsertEntryV2)`. But `process_fs_event`/`handle_creation_or_modification` are **also** the replay path
  (`run_replay_event_loop`), which must stay unthrottled, and they don't receive the `EventReconciler` (`&mut self`)
  that owns the map. So thread a **live-only** throttle handle (`Option<&mut Throttle>`, `None` in replay) down through
  `process_fs_event` → `handle_creation_or_modification`. The throttle decision happens **after** the stat (needs the
  new size for the significance test), immediately before the `writer.send`.
- **Trailing-flush timer:** the live loop(s) already run a `tokio::select!` with a `flush_interval` tick. Add a
  throttle-sweep (a dedicated ~1 s tick that flushes any key whose 60 s window elapsed, or fold into the existing
  cadence). **No new thread.**
- **Two live loops — a duplication hazard, NOT a double-apply risk (N2).** `event_loop.rs` has two `select!` live loops:
  `run_live_event_loop` (post-scan path, from `scan_completion.rs`) and the inline post-replay tail (its own
  `EventReconciler::new()`, from `manager.rs`). They're on **mutually exclusive** startup paths — a volume goes through
  one or the other, never both — so no double-apply. The cost is implementing the sweep twice; **unify the two loops
  first** (cleaner), or wire the sweep in both. Don't half-wire.
- **Replay stays unthrottled** (`None` handle): journal catch-up at launch must converge fully and fast; the throttle is
  a live-steady-state concern only.

### Surfacing "approximate" — the `~` marker is mostly MOOT for M1 (B1, verified)

**Key finding from review, verified in code:** a listed **file's** displayed size does NOT come from the index. Index
enrichment (`enrichment.rs::enrich_entries_with_index_on_volume`) overlays recursive size ONLY onto **directory**
entries (`is_directory && !is_symlink`); a file's `size` in a pane comes from the **live `lstat`** during
`list_directory`, and the `file_system` watcher re-stats it live on change. So:

- **A throttled file in a pane still shows its true, current size.** The throttle changes only how often we write the
  _index_, which for files feeds search + directory aggregation — not the pane's file size. So no `~` is needed or even
  sourced there.
- **Directory recursive sizes** (which the index DOES feed) stay `Exact` under our "don't propagate reason up" rule — so
  the aggregate the user sees is never marked either.

Therefore the `~` "approximate size" marker, the `SizeFreshness` reason enum, the per-entry `size_reason` column, and
the `SCHEMA_VERSION` bump **buy almost nothing in M1** and would force a full disposable-cache rebuild (whole-disk
re-scan) on every beta tester (`SCHEMA_VERSION` is already at 14). **Recommendation: M1 is a pure backend
write-reduction with NO schema change, NO reason column, NO frontend marker** — invisible to the user, zero UX downside
(pane sizes stay live and exact), all upside (CPU).

The "approximate" honesty concern only bites where the index size is _actually shown_, which is:

1. **Search results** — `search/index.rs` loads `logical_size` from `entries`, so a throttled file's search-result size
   can lag ≤60 s. A marker here would be honest. Small, optional surface — treat as a **follow-up**, decide with David.
2. **Hot-directory recursive sizes (M3)** — a coalesced hot dir's aggregate genuinely lags, so the marker is meaningful
   there. **The reason enum / marker lands in M3, not M1**, where it decorates something real.

**Decision needed from David** (see report): confirm dropping the `~` marker from M1 (backend-only), and whether the
search-result marker is worth a small follow-up or skipped. This is the one open product call the review surfaced.

### `~/Downloads` allowlist

`~/Downloads` stays **Exact** (never throttled) — users watch active downloads there and want a live size (David's
call). Implement as an allowlist check in the throttle decision: if the path is under the user's Downloads, skip
throttling entirely. Resolve Downloads via the OS dir lookup, not a hardcoded string. Note the FDA/TCC caveat: reading
metadata under `~/Downloads` is already happening via the index; the allowlist is purely a "don't throttle" flag, no new
TCC surface.

### Search-index prealloc right-size (independent cleanup)

`search/index.rs::load_search_index` hardcodes `String::with_capacity(100_000_000)` + `Vec::with_capacity(5_000_000)` (a
worst-case ~5M-entry guess), paid on every load. Replace with a count-driven size: `SELECT COUNT(*) FROM entries` once,
then `Vec::with_capacity(count)` and `String::with_capacity(count.saturating_mul(AVG_NAME_BYTES))` (AVG ~20), clamped to
a sane ceiling so a bad count can't request gigabytes. Secondary to the throttle (reloads are rare once the firehose is
cut), but a correct, cheap win. Independent of the throttle work — can land in either order.

## Milestones

Each milestone ends green: scoped `pnpm check --fast`, then a milestone `pnpm check`. TDD (red→green) is called out per
item; this is risky live-indexing + data-shape logic, so lean test-first.

### M0 — Honest idle-CPU baseline on top of `cc6681c00` [do FIRST, before any code]

The earlier numbers predate the importance fix, so re-measure. On a rebased build (current `main`), capture idle
attribution over a comparable window: live event rate (`live_heartbeat total_events` delta/min), writer msgs/s, and a
`sample`/`vmmap` snapshot of the running instance while idle. Record it in `docs/notes/`. Purpose: (a) know what the
per-event reconcile+write pipeline actually costs NOW, so M1's before/after is honest and not conflated with
`cc6681c00`; (b) sanity-check that the per-event layer is still a _meaningful_ consumer worth throttling. **If the fresh
baseline shows the per-event pipeline is now negligible, pause and report to David before building M1** — the throttle
still helps disk-write thrash and the self-loop, but its priority would change.

### L1 — Importance folded-key: kill the per-comparison NFD normalization [THE idle-CPU win, do first after M0]

**Confirmed sink** (8 s `sample`, `importance-writer` thread pegged a core): `apply_incremental`'s subtree-clear
`DELETE FROM weights WHERE path = ?1 OR path LIKE ?2 || '/%'`. `weights.path` is a `WITHOUT ROWID` PK with a **custom
`platform_case` collation** (case + NFD fold), so the LIKE-prefix optimization can't apply and the planner **full-scans
~166k rows**, invoking `platform_case_compare` → `unicode_normalization` NFD on **both operands per row**, per changed
prefix. `cc6681c00` throttled this to ≤1/60 s but each firing still multi-second-pegs a core.

**Fix (mechanical, provably equivalent):** add a precomputed `path_folded TEXT` column =
`normalize_for_comparison(path)` (the same function the collation ran internally), make it the **BINARY** PK, keep
verbatim `path` as a plain column for return values. Rewrite the DELETE as an explicit half-open range on the BINARY key
(`path_folded = ?1 OR (path_folded >= ?2 AND path_folded < ?3)`, `?2 = folded||"/"`, `?3 = folded||"0"`) so it's
index-served — removes both the per-compare NFD AND the full scan. Bump importance `SCHEMA_VERSION` 2→3 (disposable
cache: wipes only `importance-*.db`, one recompute regenerates; **no drive-index rebuild, no tester re-scan**).

**Ranking correctness (preserved):** the score is pure Rust (never touches SQL collation); `path_folded` is
byte-identical to what the collation computed, so row identity/collision is exactly preserved; `ORDER BY … path ASC` is
a determinism tiebreak only; the search ranker doesn't use the collation at all (exact `HashMap` lookup on verbatim
path). Consumers `weight_for`/`lookup`/`read_*` bind `normalize_for_comparison(path)` against `path_folded`.

**Files:** `importance/store/mod.rs` (schema + `SCHEMA_VERSION` + `read_weight`/`read_visit`), `importance/writer.rs`
(`WeightRow` folded key, `insert_rows`, `apply_incremental` DELETE, `apply_visit`), `importance/read/mod.rs`
(`read_scored_weight`, `read_ordered`), `importance/scheduler/recompute.rs` (build folded in `WeightRow`). Reuse
`indexing/store::normalize_for_comparison` — single-source, don't reimplement.

**TDD (red first):** (1) a ~150k-row synthetic `importance.db` micro-bench timing one incremental write + asserting
`EXPLAIN QUERY PLAN` of the DELETE shows index `SEARCH`, not `SCAN weights` (the crisp red→green); (2) keep the
case/NFD-fold lookup test + full-pass + incremental transition tests green; add a case-variant `weight_for`-after-
incremental test. **Docs:** fix `importance/DETAILS.md`'s stale "the walk, not the write, dominates" line (the profile
disproves it); document the folded-key decision + the case-sensitive-volume collision note (unchanged from today).
**Drive types:** APFS local (main target), SMB (NFD display paths — same fix), case-sensitive volumes (same collision as
today, no regression), MTP excluded (never scored). Checks: `pnpm check rust` then milestone `pnpm check`. **Measure
before/after** (bench + `EXPLAIN QUERY PLAN` + a scoped walk-vs-write timing log).

### L2 — Importance targeted subtree walk [after L1; gated on re-measurement]

The incremental walks **all** ~166k dirs before rescoping to the touched subset (`walk_index_folders`), on a
`spawn_blocking` worker (so the `sample` didn't catch it — but it's real O(dirs) work, capped to 1/60 s by the
throttle). **Re-measure after L1 first:** if L1 brings idle CPU under target and the walk isn't co-dominant, L2's
cross-boundary correctness risk may not be worth it yet — report the numbers and decide. If doing it: add
`walk_index_subtree(conn, home, changed_paths)` computing `IndexFolder` for only the touched ancestor-chains ∪ changed
subtrees, with `has_marker_below` from a bounded downward DFS and `under_floored_ancestor` from the capped upward walk,
reusing the **shared** `classify` predicates so it can't drift. **Guard with an oracle test:** targeted-walk output must
be identical to filtering the full `walk_index_folders` output to the same set, across the hard cases (`.git` deep
inside; `node_modules` ancestor above the subtree; rename in/out of floored; deletion). Once targeted, the 60 s throttle
can relax. Primitives all exist (`list_child_dir_ids_and_names`, `get_parent_id`, `reconstruct_path`,
`resolve_component`). Keep all `incremental_tests`/`recompute_tests` green. This is the one genuinely tricky part —
oracle test is mandatory.

### M1 — Per-file throttle (pattern 1), backend-only [after the importance levers; disk-thrash + self-loop win, not the idle-CPU headline]

Orthogonal to the adjacent guarded-local-scan refactor (see § Coordination). Touches `reconciler.rs` (live path) +
`event_loop.rs` (sweep tick) only. **No schema change, no reason column, no frontend** (per B1: pane file sizes are
live; the throttle is an invisible backend write-reduction). Pending David's confirmation on dropping the marker.

1. **Throttle engine + significant-change bypass — TDD red first.** A pure, unit-testable throttle keyed by normalized
   path, holding `(last_applied_at, last_applied_size, pending: Option<u64>)` (see § Throttle state model), with
   `THROTTLE_WINDOW = 60 s`, `FLOOR = 512 KB`, `2%`. Tests (red→green): leading edge applies immediately; second change
   within window is suppressed and sets `pending`; trailing flush after the window applies the **last-seen** size (not a
   re-stat) and clears `pending`; sustained change fires once per window (throttle, not debounce); significant bypass
   (≥max(2%, 512 KB)) applies mid-window; sub-floor flap stays throttled; empty→nonzero is significant but
   `new==0 && last==0` is NOT (the lock-file case); **a key with `pending == Some(_)` is not evictable** (data-safety).
   Pure logic, no FS/DB — inject the clock (pass `now: Instant` in) so tests are deterministic.
2. **Thread a live-only throttle handle through the shared path (S2).** `Option<&mut Throttle>` down
   `process_live_event` → `process_fs_event` → `handle_creation_or_modification`, `None` on the replay path. Decision
   happens after the stat, before `writer.send(UpsertEntryV2)`: on suppress, store `pending = Some(last_seen_size)` and
   skip the send; on apply, send and update `last_applied`. Downloads allowlist short-circuits to always-apply.
3. **Trailing-flush sweep** on the live loop timer(s): a ~1 s tick flushes any key past its 60 s window using its
   `pending` size, then clears `pending`. Evict only cold `pending == None` keys past N quiet windows; `log()` a
   hard-cap eviction. ⚠️ **Unify the two live loops first, or wire the sweep in both** (N2).
4. **Throttle-boundary note (S4).** The throttle covers the dominant pattern-1 path (in-place rewrite → `item_modified`
   → `process_live_event`). Live upserts via `diff_dir_against_db` (MustScanSubDirs / `reconcile_subtree`, on a
   `spawn_blocking` task with no `EventReconciler`) stay **unthrottled** — acceptable because in-place rewrites don't
   trigger MustScanSubDirs. State this boundary in the code + docs so it isn't a silent hole; a dir that both churns
   children and triggers rescans is M3's job.

Docs: update `indexing/CLAUDE.md` must-knows (live throttle: 60 s, leading+trailing, 2%/512 KB bypass, Downloads exempt,
replay + `diff_dir_against_db` unthrottled, pane file sizes are live so the throttle is invisible for files) and
`indexing/DETAILS.md` (mechanism, why throttle-not-debounce, why no marker in M1, the self-loop-subsumed note). Add a
`docs/notes/` entry with the profiling evidence (event rates, vmmap breakdown) that motivated this.

Tests: unit (throttle engine per above). Integration over a temp index driving synthetic rapid events through the live
reconciler asserting ≤1 write/window/file + correct final size — this exercises the real `tokio::time::interval` sweep,
so use `tokio::time::pause()`/`advance()` or a test-configurable `THROTTLE_WINDOW` (N4), not only the threaded clock.
Checks: `pnpm check` (rust + docs groups; no svelte change in M1).

### M2 — Search-index prealloc right-size [independent; any order]

Count-driven capacity in `load_search_index`. Test: load over a small synthetic index doesn't over-allocate (assert via
the count path / a capacity assertion); load correctness unchanged (existing search tests stay green). Check:
`pnpm check rust`.

### M3 — Per-hot-directory coalescing (pattern 2) [DEFERRED — future work, not this effort]

**Deferred:** the guarded walker landed so it's technically unblocked, but M3 is the most complex + data-safety-heavy
milestone (in-memory child-churn coalescing across rescan boundaries) and, post-baseline, is NOT the idle-CPU sink
(that's L1). Not worth landing unsupervised. Left here as the next-priority future work once L1/L2/M1 land and prove
out. Original design below.

**Dependency resolved (2026-07-14):** the guarded-local-scan refactor **landed on `main`** (the spec was retired; see
`afe5a0df4`, `e42a933fc`, `db4aa1364`), so the `scan_subtree` rewrite this rode is done. Re-read the current
`scanner/` + `reconciler.rs::reconcile_subtree` on the landed walker before building — the mechanism M3 hooks changed
shape, so validate the coupling against the new code (was N6). Do M3 **after M1 ships and its baseline is confirmed**,
so the two throttles' effects don't get conflated.

Design (to refine against the landed walker):

- Detect a **hot directory**: a dir receiving ≥3 child create/delete/modify events within 10 s (David's threshold)
  enters throttled state; demote after ~2 windows quiet. Bounded LRU of hot dirs; `log()` if capacity forces eviction
  (no silent cap).
- **In-memory child accumulator per hot dir:** net out create+delete of the same child within the window (born-and-gone
  → zero DB writes), keep only the latest size per surviving child. After 60 s, flush **one** coalesced batch: net
  inserts/deletes + a single size delta propagated up the ancestor chain once. Collapses N events → ~1 flush/dir/window.
- **The `~` marker + `SizeFreshness` reason enum land HERE, not M1 (B1, S5).** A hot dir's _recursive_ size genuinely
  lags, so marking it approximate is honest and decorates something the user actually sees (dir sizes come from
  `dir_stats`). Add the typed reason (`no-string-matching`), stamp it on the hot dir's `dir_stats` row (NOT on plain
  file entries — a file's own throttled metadata does not make its parent's recursive aggregate approximate), cross IPC
  as a typed field, render `~` prefix + i18n tooltip (copy reviewed by David). Bump `SCHEMA_VERSION` here (one rebuild,
  in the milestone that needs it). Keep "don't propagate reason up the ancestor chain."
- **Data-safety:** the accumulator must never lose a real deletion or mis-net across a rescan boundary; a
  disconnect/rescan flushes or discards cleanly. TDD hard here (born-and-gone nets zero; born-and-survives inserts once;
  changed-twice keeps last; delete-of-preexisting propagates; crash/rescan mid-window stays honest).

Docs + tests as M1, plus reconcile-boundary tests and the FE marker. Checks: full `pnpm check --include-slow` (touches
scan paths + svelte).

## Coordination with guarded-local-scan (adjacent worktree)

Verified overlap (2026-07-14):

- Their territory: `scanner/*`, `local_reconcile.rs`, `store/` `dir_ids` map, `Cargo.toml` (jwalk). Our territory:
  `reconciler.rs` (live), `event_loop.rs`, frontend, `search/index.rs`.
- The live path resolves parents via `store::resolve_path` (DB), **not** the scan-time `dir_ids` map they delete → M1/M2
  are independent of their biggest change.
- **One seam:** `event_loop.rs` calls `scanner::scan_subtree` (which they rewrite). M1/M2 don't touch it; **M3 does** →
  M3 sequences onto their walker.
- **Verify before M3 (N6):** the sibling plan lists `scanner/*` + `local_reconcile.rs` as what the walker replaces, but
  does NOT explicitly name `reconciler.rs::reconcile_subtree` (the small-scope MustScanSubDirs path M3 rides). Confirm
  whether `reconcile_subtree` is in the walker's scope before asserting M3 "cleanly sequences onto it" — it may be an
  unlisted overlap, or M3's coupling may be looser than stated.
- `WriteMessage` is shared but neither side changes its variants (their writer work is a separate follow-up; we send
  fewer of the same messages). M1 adds NO reason field (B1), so no `WriteMessage` change; M3's reason lands on
  `dir_stats`, coordinate then.

**Update (2026-07-14): their work landed on `main` and this branch is rebased onto it** (includes `cc6681c00` + guarded
walker). So all three milestones now sit on top of the finished refactor; no cross-effort coordination remains. M1 + M2
land first; M3 follows once M1's baseline is confirmed.

## Out of scope (separate notes)

- **Memory / GPU RSS.** The ~1.4 GB is mostly WebView IOAccelerator surfaces + shared pages, not indexing. Worth a
  separate investigation: does the WebView release GPU surfaces when backgrounded? Capture the reporter's
  `phys_footprint` (not raw RSS) to confirm. Do NOT promise a memory fix from this throttle.
- **Whole-boot-volume, file-level live watching** as an architecture. The throttle _mitigates_ the firehose; it doesn't
  remove the root design question (coarser dir-level FSEvents, or keeping only the navigated subtree live). Candidate
  for a later spec.
- Self-specific "Cmdr manages this file" tooltip (global throttle subsumes the loop; nicer copy is a refinement).

## Open questions / risks

- **David's product call (M1 blocker):** confirm dropping the `~` marker from M1 (backend-only throttle, per B1), and
  whether the search-result marker is worth a small follow-up or skipped.
- **Two live loops in `event_loop.rs`** — recommend unifying them before wiring the sweep (they're mutually exclusive,
  so it's a duplication cleanup, not a correctness fork). Execution decides; must not half-wire.
- **Downloads resolution** — OS dir API; handle the (rare) no-Downloads case gracefully. Purely a "don't throttle" flag,
  no new TCC surface.
- **Throttle map bound** — pick a cap and eviction; only cold `pending == None` keys are evictable; `log()` on forced
  eviction.
- **E2E interaction** — the throttle changes live-update timing; check the Playwright/live-index E2E specs don't assume
  instant _directory-size_ index updates on rapidly-changing fixtures (file sizes in panes stay live, so those are
  unaffected). May need a non-hot fixture or a wait.
- **M3 reason storage** — `dir_stats` column for the hot-dir reason; confirm cleanest shape when M3 starts.
