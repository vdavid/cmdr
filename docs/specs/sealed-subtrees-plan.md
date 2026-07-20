# Sealed subtrees: bounding the cost of high-churn directories

Status: plan for implementation. Branch: `david/sealed-subtrees`. Follows
[`indexing-churn-resilience.md`](indexing-churn-resilience.md) and the per-subtree rescan throttle
(`indexing/DETAILS.md` § "Per-subtree rescan throttle").

## The incident that motivated this

Measured on David's machine, dev run of 2026-07-19 22:44:45 (`CMDR_LOG_RAM_USE=1`):

- Cold-start replay found an 18,314-event journal gap → 288 affected dirs.
- `run_background_verification` then ran for **7 min 6 s** (`425937ms`), reporting
  `25 stale, 1082101 new files, 0 new dirs`.
- The writer channel sat pegged at its 20,000 cap (`queue_depth=20001`) for the whole window, processing ~14k msgs/5 s
  at ~100% busy. Backend `phys_footprint` peaked at **1.01 GB**; the floor between bursts was ~353 MB, dropping to ~245
  MB once verification finished.

One directory accounts for essentially all of it:

```
~/Library/Containers/com.google.drivefs.fpext/Data/tmp/domain-temp-gdrive-<id>/fetch_temp
  children = 1,138,220    logical = 0 bytes    physical = 0 bytes    largest file = 0 bytes
```

1.14M **empty** files in one flat directory, still growing. It is also ~16% of the 6.95M rows the search index loads
into RAM.

`0 new dirs` matters: there was no recursive amplification. This was a single directory's one-level diff.

**Why the existing throttles don't help.** They work. Post-verification steady state is 50–100 msgs/5 s with an empty
queue, and DriveFS's ~30 renames/min arrive as one batched burst per minute. But both throttles live in `reconciler`
(`throttle.rs` per-file, `rescan_throttle.rs` per-subtree) and `event_loop/verification.rs` consults neither, by design:
verification is the correctness catch-up after a cold start.

More fundamentally, **throttling cannot fix this class**. Re-syncing a directory costs O(children), not O(events),
because the per-child events were dropped and all you can do is readdir and diff. Throttling a 1.14M-entry directory
converts continuous background trickle into a 7-minute stall once per window.

## Why not just exclude

Exclusion breaks size truth for every ancestor up to `~`. For `fetch_temp` the cost is zero (it is 0 bytes), but the
same reflex applied to `target/` would silently remove 50+ GB. Today's exclusions are all absolute prefixes outside `~`
— `/Library/Caches/` does **not** match `~/Library/Caches` (`scanner/exclusions.rs:39-53`) — and the only home-dir
exclusions are four junk basenames. Everything inside the home folder is currently truthful. That invariant is worth
defending.

## The idea

**Seal a subtree: keep its aggregate, drop the per-file tail.** A sealed subtree keeps its `dir_stats` aggregate (so
size truth propagates to `~`) but stores per-file rows only for the files carrying the bytes.

Sealing is a **mode, not a deletion**: we keep consuming FSEvents (to measure churn and to credit the aggregate between
re-anchors), we just stop writing per-file rows and stop per-child diffing. Sustained quiet unseals it.

### Load-bearing consequence 1: a sealed dir is a recompute boundary

The ledger's whole-effort invariant is `dir_stats ≡ recompute-from-entries` (`indexing/DETAILS.md` § "The whole-effort
invariant"). Sealing breaks it by construction: `repair_dir_stats_upward` recomputes each level and **overwrites stored
when it differs** (`writer/repair.rs:47-91`). A sealed dir whose rows were dropped recomputes to ~zero, so the primitive
that keeps the ledger honest would wipe every sealed aggregate on the next ancestor walk.

The invariant becomes: **`dir_stats ≡ recompute-from-entries`, with sealed dirs as opaque leaf constants.**

The good news is the surface is smaller than it looks. All six aggregator entry points funnel through one kernel,
`compute_bottom_up` (`aggregator/mod.rs:251`, called at `:188`, `:387`, `:498`, `:745`). So there are **two
implementation points**, not five:

1. **`aggregator/mod.rs::compute_bottom_up`** — covers `compute_all_aggregates`, `compute_all_aggregates_with_maps`,
   `compute_partial_aggregates`, `compute_partial_aggregates_sql`, `compute_subtree_map`, and
   `backfill_missing_dir_stats` (`:701`).
2. **`writer/repair.rs::recompute_dir_stats_from_children` (`:98`)** — and critically, its **two sub-recomputes**, which
   a naive fix misses: `recompute_recursive_has_symlinks` (`:137`) and `IndexStore::recompute_min_subtree_epoch`
   (`store/dir_stats.rs:183`). Without them a sealed dir silently loses a true symlink flag and gets its coverage epoch
   recomputed from the surviving rows.

Plus **`writer/delta.rs::propagate_min_subtree_epoch` (`:215`)**, an independent up-walker that also calls
`recompute_min_subtree_epoch`, and the stress oracle **`stress_test_helpers::check_db_consistency`**, which will
otherwise fail on any sealed tree.

**A third site in a different category: delta computation, not aggregate recomputation.** `handle_delete_subtree_by_id`
(`writer/entries.rs:697`) computes its negative delta from `IndexStore::get_subtree_totals_by_id`, which is a
**recursive CTE summing `entries` rows directly** (`store/dir_stats.rs:137-149`), not the stored aggregate. Delete a
sealed directory — or any ancestor of one — and only the surviving rows' bytes are credited back; the collapsed bytes
stay added to every ancestor up to `~` **permanently**, with no drift mechanism to reclaim them. It fires from stale
detection (`verification.rs:276`), the per-navigation verifier, `MoveEntryV2`'s conflict-overwrite (`entries.rs:523`),
and any live directory removal.

Note the asymmetry, which is why this is easy to miss: `handle_move_entry_v2` (`entries.rs:561`) already reads
`get_dir_stats_by_id` — the stored aggregate — so the **move** path is sealed-safe while the **delete** path is not.
Deletes must use the same source for a sealed subtree root.

_(An earlier draft named `partial_agg.rs` and `writer/aggregation.rs`. Both are wrong: `partial_agg.rs` holds only
scheduling helpers (`should_send_partial_agg`, `collect_hot_paths`) and `writer/aggregation.rs` holds delegation
wrappers. Neither contains aggregate math.)_

**Thread the sealed set as a required parameter, not an `Option` with a default.** `compute_bottom_up` already takes
`listed_epochs` for exactly this shape, and `DETAILS.md` warns that all four callers must supply it ("a missing one
would re-break coverage"). Let the compiler enforce the same for the sealed set.

**Pass the sealed values, not just the ids.** `compute_bottom_up` takes only maps — **no `&Connection`**
(`aggregator/mod.rs:251-258`). So knowing "id 4711 is sealed" tells it to skip the recompute but not what to write
instead; the opaque leaf would have no constant. Its only existing escape hatch, `existing_stats`, is `None` at three of
the four call sites (`:188`, `:387`, `:498`; only backfill at `:745` passes it). So thread a
**`HashMap<i64, DirStatsById>`**, built by reading the sealed dirs' stored `dir_stats` rows before the pass.

This is exactly where the `AggSource::Maps` path bites: a first full scan with a pre-seeded `target/` has the walker
write the sealed aggregate, then `ComputeAllAggregates { Maps }` runs `compute_bottom_up` over maps that never saw the
collapsed files, with `existing_stats: None`, and overwrites it with ~0. Funnelling through `compute_bottom_up` is
necessary but **not sufficient** — the sealed values have to be threaded in.

**Key the seal row by `entry_id`, with path as a rebuild fallback — and fail loud.** Pure path keying fails _open_ on
rename: renaming the seal root, or **any ancestor**, orphans the row, so the id never enters the sealed map,
`compute_bottom_up` recomputes from surviving rows, and the aggregate is wiped by the app's most ordinary operation.
`MoveEntryV2` already rewrites rows in place preserving `entry_id` and `dir_stats` (`writer/entries.rs:534-536`) — which
is exactly why this codebase is id-keyed everywhere (`DETAILS.md:860`: keyed by entry id "so a rename or delete landing
between send and process can't make the aggregate silently no-op"). So: `entry_id` is the resolution key (renames then
need no seal-table work at all), path is the fallback for rebuilding after `clear_index`, and an unresolvable seal row
must be **loud and fail-safe — never fall through to "recompute this dir from children"**. Do not inherit
`compute_partial_aggregates_sql`'s silent-skip idiom: there a skipped hot path is harmless, here it is destructive.

**State which path space the fallback path is in** — the LOCAL pipeline is mount-relative via `IndexPathSpace` and
strips the mount root only at `resolve_abs`, so "path-keyed" is ambiguous today. The resolution primitive is
`resolve_path_under(conn, ROOT_ID, path)` (`aggregator/mod.rs:603`), not `resolve_path`.

### Load-bearing consequence 2: the walkers will resurrect the rows

Seal state must be consulted by every path that walks and upserts. The complete set:

- **`local_reconcile.rs`** (`run_local_reconcile:299`, `build_live_children:232`) — **the most likely way sealing gets
  undone.** Per `DETAILS.md` § "Non-destructive rescan", a rescan of a populated, previously-completed index takes
  _this_ path. A shallow `MustScanSubDirs` → `start_scan` → `local_reconcile` re-walks and re-upserts the sealed subtree
  wholesale.
- **`reconciler::reconcile_subtree`** and **`scanner::scan_subtree`**.
- **The guarded walker.** Not just "skip": on a full rescan it must **compute and write the sealed aggregate** (a
  streaming sum), or the sealed dir ends up with no `dir_stats` row. The failure is visible rather than silent —
  `recompute_dir_stats_from_children`'s LEFT JOIN absorbs `min_subtree_epoch` to 0 (`repair.rs:36-41`), so ancestors
  read _incomplete_ rather than under-sized — but it is still a bug.
- **`verifier.rs`** (per-navigation diff) — reads DB children and upserts everything on disk that's missing. Navigating
  to a sealed dir before unseal completes re-inserts 1.14M rows.
- **`event_loop/verification.rs::verify_affected_dirs`** — post-seal the dir has ~0 DB rows, so M1's DB-side count probe
  can never fire for it. Only the streaming disk-side cap stands between a sealed dir and full re-insertion.
- **`reconciler::process_fs_event` / `handle_creation_or_modification`** — the live create path writes a _row_
  (`UpsertEntryV2` is what auto-propagates the delta). Sealed mode needs a **row-less credit path**, and
  `PropagateDeltaById` keys off an `entry_id` that no longer exists for a collapsed file or its parent dir. The delta
  must be re-anchored at the seal-root id, which needs path→seal-root resolution on the live event path. **This is a new
  message shape**, not a reuse.

**Where seal state lives.** A dedicated table, keyed by path. `create_tables` runs `CREATE TABLE IF NOT EXISTS` on
**every** open and does so _before_ the schema-version check (`store/connection.rs:78-84`), so a new table materializes
on existing DBs at **zero rebuild cost and no `SCHEMA_VERSION` bump**. Only _altering_ an existing table forces a bump.
Use a real table (path PK, seal epoch, reason, counters, index), not a `meta` blob.

_(The reason is not "truncate wipes it". `TruncateData` fires only for a sentinel-only DB or a never-completed partial
(`DETAILS.md:533-545`); a populated completed index reconciles and preserves ids. The real reasons are that ids churn on
delete+recreate, and `clear_index` deletes the DB file outright — which `meta` would not survive either.)_

### The size cut

Measured across the full root index (6,681,172 files, 2,373 GB logical; hardlinks/clones double-counted):

| threshold | rows kept | % rows | bytes kept | % bytes |
| --------- | --------- | ------ | ---------- | ------- |
| 4 KB      | 2,177,149 | 32.59% | 2370.1 GB  | 99.87%  |
| 64 KB     | 472,435   | 7.07%  | 2344.1 GB  | 98.78%  |
| 256 KB    | 259,732   | 3.89%  | 2316.1 GB  | 97.60%  |
| 1 MB      | 124,681   | 1.87%  | 2242.1 GB  | 94.48%  |
| 10 MB     | 11,194    | 0.17%  | 1840.7 GB  | 77.56%  |

1 MB drops 5.5% of bytes — 130 GB of visible untruth. 64 KB keeps 99% of bytes for 7% of rows. This is a **global**
measurement justifying the constant, not any per-subtree rule.

**The rule: keep files ≥ 64 KB, capped at the largest 10,000 per sealed subtree.**

**Be honest about which knob binds: the cap, not the threshold.** A 1M-file subtree of 100 KB files keeps 10,000 rows
because of the cap; 64 KB does nothing there. So the "rows survive for the big `.rlib`s that dominate the 50 GB" framing
holds only when the cap doesn't bind — and it binds exactly on the pathological subtrees this targets. **This re-opens
Phase C's drift on the majority of bytes**: most of a large sealed subtree's bytes can sit in _collapsed_ files, so
delete/modification drift applies to the bulk of the aggregate, not a tail. Size the re-anchor cadence against that.

**Two different mechanisms, not one:**

- **Sealing already-indexed rows**: no heap needed.
  `DELETE … WHERE parent_id IN subtree AND id NOT IN (SELECT id … ORDER BY logical_size DESC LIMIT 10000)`. SQLite does
  it.
- **Scanning an already-sealed subtree**: a bounded min-heap. But note it is not truly "streaming": you cannot emit a
  row until you know it survives the top-10,000, so writes buffer to the end of the walk (or need retraction with
  compensating deltas). And a subtree-global top-k across a _parallel_ guarded walk means a shared mutex'd heap (the
  hardlink-inode-set pattern), i.e. contention on the hot path.

**Open: what happens to directory rows?** The plan's own worked example (`something/cache/{hex}/{hex}/{hex}.tmp`) is
dir-heavy — ~65k+ directories, few files each. Keeping dir rows makes the row reduction far smaller than the "1.14M → 1"
headline and still walks them in the rollup. Dropping them breaks `resolve_path` for anything under the seal root, which
breaks `reconcile_subtree` re-entry, `PropagateDeltaById` targeting, `recursive_dir_count`, and unseal scoping. **See
Decision 2 — this must be settled before Phase A.**

### Where to seal (the depth question)

For `something/cache/{hex}/{hex}/{hex}.tmp`, the seal root is `something/cache`, **not** `something`. `something` also
holds the source and config the user cares about.

The rule: **the highest node whose subtree is uniformly churny.** Roll churn up the ancestor chain, climb while the
subtree stays ~100% churny by descendant count and by bytes, stop at the first ancestor where the ratio drops. That drop
_is_ the signal you have reached a directory holding something worth keeping. Per-directory decisions can't work: no
individual hex leaf is remarkable; only the roll-up at `cache/` is enormous.

**This is a genuinely new mechanism.** `pick_and_collapse_rescan` (`reconciler/rescan.rs:221-239`) picks the shallowest
member of a pending set and drops queued descendants; it is not an ancestor walk and aggregates nothing.
`rescan_throttle.rs` is only `is_eligible` / `record_completion` / `gc`. Borrow their _testability_ shape (pure,
clock-injected), not their logic.

**Hard stops:** never seal `~`, `~/Documents`, `~/Desktop`, `~/Downloads`, `~/Pictures`, or a volume root. The list is
belt-and-braces only — churn is what actually keys the decision — and it needs a Linux counterpart (Cmdr has a Linux
lane), or an explicit note that it's a macOS-only safety net.

### Reversible beats correct

The classifier needs a cheap undo, not perfection. Seal lazily, unseal automatically on sustained quiet. A false
positive then costs one subtree scan we would have done anyway.

**Churn is episodic** — `target/` is a firehose during a build and quiet for days after. Permanent-until-navigated
sealing would let one `cargo build` degrade a folder forever. Automatic unseal is not optional.

## Milestones

### M1 — Bound the blast radius (ships alone)

Independent of all sealing machinery. No schema change, no seal state.

**Two teeth.** `verify_affected_dirs` materialises the whole DB child list _before_ any per-child work
(`verification.rs:222-250`). Guarding only the upsert loop leaves most of the cost in place.

1. **Before** `list_children_on`: a `LIMIT threshold+1` probe (not a full `COUNT(*)`). Over threshold → skip the path,
   don't snapshot it.
2. **Inside** the Phase-2 `read_dir` loop: a cap on **iterations, not upserts**. The loop `continue`s past DB-known
   children before doing any work (`verification.rs:290-298`), so for a `fetch_temp` already fully in the DB the upsert
   count is near zero while the iteration count is 1.14M — and the iteration plus the `db_child_names` `HashSet`
   (`:259-262`) is where the time and memory actually go. An upsert cap would be a no-op on the measured incident. This
   tooth also covers the directory that is small in the DB but huge on disk, which passes any DB-side count.

**What a declined directory must NOT do: write `listed_epoch = 0`.** An earlier draft proposed this as "honest-stale".
It is the opposite. Affected dirs carry a _positive_ epoch from the scan, and `absorbing_min_epoch`
(`aggregator/mod.rs:272-286`) propagates a zero up the whole chain, so `min_subtree_epoch` → 0 for every ancestor to `~`
and `/`. The read side derives `recursive_size_complete = min_subtree_epoch > 0`, so declining one temp dir would render
the entire home folder incomplete and make `expected_totals` return `None` for every copy of `~`.

The 32-failed-reads walker precedent does not apply: those dirs were _never_ listed, so they stay at 0. Nothing is
downgraded. Same word, opposite operation. **Leave `listed_epoch` untouched.**

**The honest cost of M1 (this is a trade, not a free win).** Say it in the docs rather than claiming no downside:

- Tooth 1 skips before the snapshot, so the stale-detection loop (`verification.rs:272-282`) never runs: deletions from
  the journal gap are not reaped and the ancestor chain stays inflated until another path corrects it. That is the same
  delete-drift class sealing later has to solve.
- Tooth 2 leaves a _partially_ diffed directory.
- The declined dir still reports `recursive_size_complete = true`. We knowingly decline to enumerate and still claim
  exact. Radical transparency (`design-principles.md:11-13`) says own that debt.
- **Scope of the fix:** it fixes the _stall_. It does not reclaim the 16% search RAM (rows stay in the DB), and it
  guards only `verify_affected_dirs` — a shallow `MustScanSubDirs` still routes to `start_scan` (`rescan.rs:43-72`) and
  re-walks, and `reconcile_subtree` still diffs on a deep anchor.
- The "1.14M `EntryRow`s are a large share of the 1.01 GB peak" claim is **unmeasured**. Back-of-envelope it is ~130–160
  MB plus a comparable transient for the per-parent name `HashSet`. Measure before advertising a RAM win.

**The instrument for the n=1 question.** Not an in-memory list from verification: `verify_affected_dirs` runs only after
a journal-gap replay, so on a machine that never gaps it stays empty regardless of how many pathological directories
exist.

The guarded walker alone is not enough either: a populated, previously-completed index **never runs it** — that rescan
takes `local_reconcile.rs` (`build_live_children:232`, per `DETAILS.md:533-545`), and the walker only runs on a first
scan or after `clear_index`. So on exactly the established machines whose directories we want to count, a walker-only
counter stays zero. **Hook both the guarded walker's `visit_dir` and `local_reconcile`'s per-dir listing.**

Make it an **atomic counter**, not `DEBUG_STATS`'s mutex'd `record_must_scan` ring: the walker is multi-threaded and a
per-directory lock on the scan hot path is not acceptable. Expose it through the debug surface and the `indexing` MCP
tool.

**Tests.** Extract the decision as a **pure function** (the `rescan_route::classify` shape) with an injectable
threshold, so it runs in the ms-scale unit tier per `docs/testing.md`.

1. _(TDD, red first)_ Pure-function tests for the probe/cap decision at the boundary (under, at, over).
2. _(TDD, red first)_ Integration: a batch with one over-threshold dir and one normal dir — the oversized one produces
   zero per-child upserts **and** the normal one is still fully diffed. The second half is not optional: without it the
   test passes if the whole function is replaced with `return`, the no-op-fixture anti-pattern `docs/testing.md` names.
   Use the existing `verifier.rs::tests` pattern (`:420`, `:514`, …), which already installs a root `ReadPool` under
   `READ_POOL_TEST_MUTEX` with tiny fixtures — the caveat is about _scale_, not reachability.
3. _(Regression guard, not TDD — it cannot go red, the code never wrote the epoch.)_ A declined directory leaves
   `listed_epoch` and every ancestor's `min_subtree_epoch` unchanged.

**Docs.** `indexing/DETAILS.md` § verification gets the guard, the rationale, and the honest costs above.
`indexing/CLAUDE.md` needs a guardrail line — but it is already **782 words against the 600-word ceiling** (1000
allowlisted, `claude-md-length.go:35`), so **condense first, don't raise the allowlist** (`file-length-allowlist.md`,
`docs/doc-system.md`).

**Checks.** `pnpm check rust --fast` while iterating, then `pnpm check rust`.

### M2–M4 — Sealing (one landing unit)

Deliberately not three shippable milestones. M2 alone builds the mechanism with no trigger and no delta maintenance;
sizes would freeze silently.

**The "approximate" state moves _into_ this unit.** Post-seal `min_subtree_epoch > 0` ⇒ `recursive_size_complete = true`
(`queries.rs:288-289`), and `expected_totals::per_source_contribution` rejects only `min_subtree_epoch == 0`
(`DETAILS.md:487`) — so a copy of a sealed folder gets an inflated denominator and progress overshoots or parks at 100%,
which `design-principles.md:25-27` explicitly forbids. At minimum `per_source_contribution` must return `None` for a
sealed subtree _in this unit_. Only the FE affordance and copy belong in M5.

#### Phase A — Seal state and the recompute boundary

- The sealed-subtrees table (above), path-keyed, resolved to a `HashSet<i64>` once per pass.
- The recompute boundary at both implementation points plus the two sub-recomputes, the `delta.rs` up-walker, and the
  oracle. **Land this before anything writes a seal**, or the first ancestor walk erases it.
- **Second, equally fatal ordering constraint:** the seal-state _consumers_ (especially `local_reconcile`) must land
  before the first seal too, or the next shallow `MustScanSubDirs` re-inserts the subtree and the sealed constant
  double-counts against fresh rows until a repair runs.
- Name the `AggSource::Maps|Sql` interaction explicitly: a `Maps` full aggregate rolls up from the writer's accumulator,
  which sealed dirs never populate (no `InsertEntriesV2` for collapsed files). It funnels through `compute_bottom_up` so
  the fix covers it — but Leak D was exactly a "the maps didn't have what I assumed" bug, so say it out loud.

#### Phase B — Choosing the seal root

- Churn accounting rolled up the ancestor chain (new mechanism), promoted from Spike B's instrumentation.
- `pick_seal_root`: pure, clock-injected, hard stops applied.
- **Provisional seal on size at scan time** (the guarded walker), so the first scan is bounded without a path list. Size
  defers; only churn confirms. See Decision 4.
- **No seed list.** Decision 4 explains why, and what replaces it.

**Tests (test-first — the risky logic).** Table-driven over synthetic trees: flat pathological dir seals itself;
`something/cache/{hex}/{hex}` seals `cache`, never `something`; **a quiet 60k-file photo library is provisionally sealed
at scan time and then unseals on sustained quiet** (the regression that matters most — size defers, only churn
confirms); hard-stop paths never selected. Constants come from Spike B, not from invention.

#### Phase C — Delta maintenance, re-anchor, and unseal

**Deletes are the primary hole.** `indexing/CLAUDE.md`: _"Deletes resolve against the INDEX (unknown = no-op)."_
Collapsed files have no rows, so every delete is a silent no-op while every create credits bytes and a count. The
aggregate — and every ancestor to `~` — **inflates monotonically**, on exactly the churny dirs sealing targets.
DriveFS's ~30 renames/min are delete+create pairs, so it fires continuously. Modification drift is secondary.

**So the re-anchor is the primary correctness mechanism — and it must be gated on measurement, not deferred.** A full
re-anchor of `fetch_temp` is a 1.14M-entry readdir: the same O(children) cost the design exists to avoid, now on a
timer. The honest argument that saves it: **a re-anchor needs only the aggregate, so it is a streaming `readdir` + sum
with zero DB reads, zero writer messages, and zero row writes** — categorically cheaper than the verification diff,
which paid a DB snapshot, a per-child lookup, and 1.08M writer messages. But a `readdir` + `lstat` of 1.14M entries is
still tens of seconds of IO. **Measure it before starting M2–M4**: if the cadence needed to keep drift tolerable makes
this hourly, the feature nets close to zero on the metric it was built for.

- `PropagateDeltaById` (`entry_id`, `logical_size_delta`, `physical_size_delta`, `file_count_delta`, `dir_count_delta`;
  `writer/mod.rs:253-259`) keeps the aggregate _approximately_ live between anchors, re-anchored at the seal-root id. It
  cannot be trusted to stay exact.
- `file_count_delta` drifts alongside bytes, which matters because `expected_totals` uses counts.
- Unseal on sustained quiet with **hysteresis: seal fast, unseal slow**, plus a cooldown. Every cycle costs a full
  subtree scan.
- **Unseal on navigate: price it or narrow it.** Unsealing `fetch_temp` inserts 1.14M rows on the user's navigation path
  — the incident, triggered by one `cd`. And listings come from the filesystem, not the index; the only thing navigation
  needs from the index is per-subdirectory recursive sizes. So either narrow the trigger to "a sealed _child_ whose size
  is being displayed", or drop it in favour of the approximate-size state. **Decision 3.**
- **Search-index reload is a non-issue — do not "price" it.** The reload is lazy: `get_loaded` (`search/volumes.rs:214`)
  compares generations only when a search runs, and `WRITER_GENERATION` is bumped on every root-writer mutation anyway,
  so live indexing already triggers it continuously. Sealing adds no new trigger and strictly _reduces_ reload cost
  (fewer rows). The real cost is the one-time delete/insert burst through the writer channel.

**Tests.** Clock-injected state-machine tests including the thrash case. Delta tests: creates credit correctly;
**deletes in a sealed subtree drift and the re-anchor corrects them** (the test that pins the hole). Aggregate math,
test-first: sealing leaves every ancestor's `recursive_logical_size` byte-identical; sealing is idempotent; a sealed
aggregate survives a writer restart; **a sealed tree passes `check_db_consistency`**; a full rescan does not resurrect
collapsed rows; a `local_reconcile` pass over a sealed subtree leaves it sealed.

### M5 — Transparency (FE only)

The read-side "approximate" state lands in M2–M4; M5 is the affordance and the copy.

- A distinct third size state threaded through `DirStats`, `FileEntry`, the specta bindings, `full-list-utils.ts`, and
  `sorting.rs::known_dir_size`.
- **Do not reuse the `recursive_size_pending` hourglass.** That means "index writes are in flight right now"
  (`isDirSizeUpdating`, `selection/DETAILS.md`). A sealed folder isn't updating, it's permanently approximate; reusing
  it shows a forever-spinning hourglass, the opposite of transparency.
- **Sealed subtrees are unsearchable** and that needs disclosure. `uncovered_scopes` does **not** fit: it is populated
  per scope path and only when the whole _volume_ is unindexed (`search/execute.rs:82-90`), so an unscoped root search
  yields no entry for a sealed subtree inside root. This needs a new field or a redesign of that field's semantics — it
  is not free.
- New user-facing strings need keys in `src/lib/intl/messages/en/*.json` with `@key` descriptions, plus `bindings.ts`
  regeneration and `bindings-fresh` green.
- No settings UI in v1 (Decision 5).

**Tests.** Component tier for the affordance, plus an IPC-contract test on the new `DirStats` field. **No E2E**: the
Playwright lane cannot produce a sealed folder against a 2 s per-test budget without a dev-only "seal this path" hook,
which is its own scope.

### Also to update (currently unlisted)

- The `indexing` MCP tool surface.
- `docs/tooling/logging.md` and the `index-query` dev tool: both will show a tree whose row count no longer matches its
  aggregate, which reads as corruption to a future debugger unless documented.
- `docs/architecture.md`'s map entry.
- `enrichment.rs`'s integer-keyed batch fast path, when a sealed dir's children rows are gone.

## Sequencing and parallelism

**Start Spike B's collection first**, before anything else: it is passive and wall-clock-bound, so it should be running
while other work happens rather than blocking on it.

Then: Spike A → M1 → re-decide M2–M4 on the data → (M2–M4 as one unit, phases A → B → C) → M5.

Every phase of M2–M4 touches the `dir_stats` ledger, single-writer by invariant with a documented leak history (A and
D); parallel agents there would be actively unsafe.

M1 is genuinely independent and shippable on its own, whatever the spikes say.

The one safe overlap: Spike B collects passively in the background while Spike A and M1 proceed. Nothing else here runs
in parallel.

## Non-goals

- Not a user-facing settings table in v1 (Decision 5).
- Not for SMB/MTP volumes in v1 — verification is root-only today.
- Not a replacement for the existing throttles.

## Decisions for David

**Decision 1 — Seal-state identity (blocks Phase A).** Recommendation: key the seal row by `entry_id` with path as a
rebuild-after-`clear_index` fallback, and make an unresolvable row loud and fail-safe. Pure path keying loses the
aggregate on any ancestor rename; pure id keying doesn't survive `clear_index`. The hybrid costs one extra column.

**Decision 2 — Do directory rows survive a seal (blocks Phase A)?** Recommendation: **keep them, collapse only files.**
Dropping dir rows breaks `resolve_path` under the seal root, and with it `reconcile_subtree` re-entry,
`PropagateDeltaById` targeting, `recursive_dir_count`, and unseal scoping. The cost is that a dir-heavy tree
(`cache/{hex}/{hex}`) reduces far less than the `fetch_temp` headline suggests — but `fetch_temp` itself is flat, so the
motivating case is unaffected.

**Decision 3 — Unseal on navigate: price, narrow, or drop?** Recommendation: **narrow it** to "a sealed _child_ whose
size is being displayed". Listings come from the filesystem, not the index, so navigation needs only per-subdirectory
sizes. Full unseal-on-navigate means one `cd` into `fetch_temp` re-inserts 1.14M rows on the user's interaction path —
the incident, on demand.

**Decision 4 — Seed list: dropped. The churn classifier is the only thing that confirms a seal.** (Settled 2026-07-20.)

An earlier draft shipped a list of known-churny patterns (`target/`, `node_modules/.cache`,
`~/Library/Containers/*/Data/tmp`, DriveFS logs, Cmdr's own dev data dir) as a cold-start prior. Dropped, for a stronger
reason than the obvious maintenance burden (Google renames a path and we're stale; `uv` and `bun` ship cache layouts
we've never seen; one machine's churn isn't another's):

**A seed list hides classifier failures.** If the seeds catch `target/` and `fetch_temp`, the classifier never gets
exercised on the two cases we actually understand, and we'd ship it having never watched it work on a known-answer
input. We'd find out it was broken on a user's machine, on a directory we've never heard of.

It is also a direct reversal of `8b0e70ae5`'s own principle: "no per-folder allowlist: the OS-provided churn signal
self-identifies the busy subtrees."

**What fills the hole it leaves.** The seed list did one job the classifier structurally cannot: cover the **first
scan**, where no churn history exists yet. M1's guard covers only `verify_affected_dirs`; the guarded walker has no cap,
so without seeds a first scan (or any `clear_index`) still fully indexes 1.14M empty files.

The replacement is the same generic mechanism applied earlier, not a path list: **provisionally seal on size at scan
time, and let churn be the only thing that decides whether the seal persists.**

- `fetch_temp`: provisionally sealed at scan (1.14M rows never written), churn confirms, stays sealed. Cost ~0.
- A quiet 60k-file photo library: provisionally sealed, goes quiet, unseals within a window, ends up correctly indexed.
  Cost ~1.5× one subtree scan.

So **size is never a seal decision, only a defer decision** — the "reversible beats correct" trade this plan already
commits to, now applied at scan time. One mechanism, no hardcoded paths, first-scan cost bounded.

**Decision 5 — Settings UI in v1?** Recommendation: **no.** The original idea was a Settings > Behavior > File system
watching ignorelist (path pattern / throttle / reason). Reasons to defer: it's a developer control in a consumer file
manager, a throttle-in-seconds column asks users to tune a number nobody can reason about, `Reason` is documentation for
our defaults rather than user input, and discoverability runs backwards (users find it only after the damage). It also
cuts against `8b0e70ae5`'s own principle: "no per-folder allowlist: the OS-provided churn signal self-identifies the
busy subtrees." If user control is still wanted later, a right-click "index this folder less often" with three named
choices beats a pattern table.

## Spike results (2026-07-20) — read this before Phase B

All three spikes ran. Notes: [`../notes/reanchor-cost-spike.md`](../notes/reanchor-cost-spike.md) and
[`../notes/churn-observability-spike.md`](../notes/churn-observability-spike.md).

**Spike A: GO, with conditions.** A re-anchor of the worst directory (1.44M entries) costs 96–181 s wall, 19–29 s CPU,
zero writer messages, and a flat 128 KiB using `getattrlistbulk` — about a quarter of the 426 s verification pass it
replaces, with none of the queue or memory pressure. Three conditions fall out of the numbers: schedule anchors on a
**cost budget, not a fixed clock** (per-entry cost is 1.9 µs at 100k entries and 80 µs at 1.43M); split the anchor into
a **cheap count pass** (`readdir`, ~17× cheaper at the median, hourly is affordable) and an expensive byte pass (every
6–12 h); and **cap the walk and degrade honestly** when it would exceed budget, since `fetch_temp` grows 100–250
entries/min unchecked. An unconditional hourly full byte re-anchor would have been a no-go.

Two hypotheses died usefully: the cost is **IO wait, not syscalls** (16–23% CPU, ~1 random metadata read per entry), and
the **FileProvider theory is refuted** — `fetch_temp`, the Chrome cache, and the control all sit on `disk3s5` with no
FileProvider filesystem mounted. It's a container _directory_, not a container _filesystem_. Warm equals cold above
~400k entries because the metadata working set stops fitting in cache.

**Spike B: the seal-root rule is wrong as written, and this is the important result.** Separation is fast, so Decision 4
holds: `fetch_temp` was classifiable within one rollup period and `target/` within three, on a run whose period was 10 s
(so ≤10 s and ≤31 s, with 10 s of resolution). A classifier stands alone. But "climb while uniformly churny, stop at the
first ratio drop" **over-climbs on real data**: it selects `~/Library/Containers` for `fetch_temp` and
`~/Library/Caches` for the WebKit cache. Sealing either would seal every app's container or every app's cache.

The cause: `fpext`→`Containers` measures a 0.971 churn share, indistinguishable from "uniformly churny", purely because
the other ~40 containers were quiet during the window. **Churn share alone cannot distinguish "this parent is entirely
churny" from "this parent's churn is dominated by one child right now."**

So Phase B must combine churn share with a **content ratio** (entries and/or bytes below the candidate versus below its
parent) — the "by descendant count and by bytes" half of the rule that this spike did not implement and that turns out
to be load-bearing, not decorative. Add `~/Library/Containers` and `~/Library/Caches` to the hard stops as
belt-and-braces. Without this, Phase B ships a rule that seals a user's whole container tree the first time one app
inside it churns.

**Spike B, unfinished:** the hysteresis constants are still unmeasured. The 4-hour window yielded 10 minutes of live
observation across a 42-minute wall span (24% coverage, stitched from three live-loop runs), because a shallow
`MustScanSubDirs` anchor superseded live mode with a rescan still running 85 minutes later, and the monitor only
observes live mode. Underneath that: **14 of 28 recorded scans were triggered by `shallow MustScanSubDirs`**, roughly
one every two hours. Any design assuming "the live loop is generally running" should verify that first. Tracked
separately from this plan.

**Spike C:** one machine, one pathological directory (plus a 119k-child runner-up). M1's census now answers this on real
machines before M2–M4 commit.

## Spikes (do these before M2–M4)

Four of the gates below were really "someone should measure this", which is a plan smell. These resolve them with data
instead of judgment. **None require the feature to exist.**

### Spike A — re-anchor cost — DONE, go with conditions

Measured 2026-07-20. Full numbers, method, and reasoning:
[`../notes/reanchor-cost-spike.md`](../notes/reanchor-cost-spike.md). Tool:
[`../../scripts/reanchor-cost`](../../scripts/reanchor-cost).

**Result: go.** One `getattrlistbulk` re-anchor of `fetch_temp` (now 1.44M entries) is 96–181 s wall, 19–29 s CPU, no
writer messages, and flat 128 KiB memory, against 426 s plus a pegged writer queue plus 1.01 GB for the verification
pass it replaces. Three conditions bind Phase C:

1. **Cadence per directory, from its own measured walk cost**, not one global timer: per-entry cost runs 1.9 µs at 100k
   entries and 80 µs at 1.43M, so no single interval fits both.
2. **Split the anchor**: a `readdir`-only count pass is 5.4–10.8 s at ~1.44M (10.6–28× cheaper across the six paired
   runs, median ~17×) and fixes count drift, so it can run hourly while the byte pass runs every 6–12 h.
3. **Cap the walk and fall back to the approximate state** when a pass would exceed its budget. `fetch_temp` grows
   100–250 entries/min with nothing pruning it, and per-entry cost rises with size, so the walk gets worse over time.

Two planning assumptions the data corrected: the cost is directory size, not the FileProvider container (a self-made
1.43M-entry directory outside any container is just as slow), and `getattrlistbulk` does not collapse the cost at
pathological scale (1.2–1.8× over `lstat` there, though 2–24× on ordinary directories depending on cache state, so still
worth using).

### Spike B — churn observability (passive collection, ~4 h window)

Log per-subtree churn from the existing FSEvents stream, rolled up the ancestor chain. Read-only instrumentation on the
live event loop; it writes no index state and changes no behaviour.

Answers three things at once:

- **How fast does `fetch_temp` / `target/` separate from background noise?** This is the "can we drop the seed list"
  question (Decision 4), answered with data rather than judgment. If separation takes hours, provisional-seal-on-size is
  carrying more weight than assumed and Decision 4 needs revisiting.
- **What does the ratio-drop boundary look like on a real tree?** Phase B's seal-root selection is the riskiest logic in
  this plan and currently rests on an invented worked example (`something/cache/{hex}/{hex}`). Real ancestor-chain churn
  ratios either support it or don't.
- **What hysteresis constants does the data suggest?** Currently an open question the plan explicitly refused to guess
  at.

Not throwaway: this instrumentation _is_ most of Phase B's churn accounting, so it gets promoted rather than deleted.

Deliverable: a few hours of collected data plus an analysis note in `docs/notes/`. A short window is sufficient — the
motivating churn sources (DriveFS log rotation ~1/min, `fetch_temp`, a `cargo build`) all cycle in minutes.

### Spike C — the n=1 question (cheap, opportunistic)

Query child-count distributions from the existing index DBs. One machine is already known (`fetch_temp`, plus a
119k-child runner-up); the gap is _other_ machines, which is what M1's counter rides along to answer over time.

## Gates and open questions

In order:

1. **Gate on M2–M4 existing at all: the n=1 answer** (Spike C + M1's counter). If it comes back n≈1 across real
   machines, then **M1 alone is the whole feature** and this five-phase change to the ledger is not justified. Do not
   sequence M2–M4 unconditionally after M1.
2. **Gate on starting M2–M4: re-anchor cost** (Spike A). **Passed 2026-07-20**, with the three Phase C conditions above.
3. **Gate on Phase A: seal-state identity** (Decision 1). A silent-data-loss question, not a cost question, so it
   outranks the remaining ones.
4. **Gate on Phase A: the directory-rows question** (Decision 2).
5. **Gate on Phase B: churn separation speed and the boundary shape** (Spike B). Also supplies Phase C's hysteresis
   constants.
6. **Gate on Phase C: unseal-on-navigate** (Decision 3), a UX/cost tradeoff.

Open beyond the gates:

- Cost item, not a correctness one: a sealed subtree keeps raising `MustScanSubDirs` and keeps consuming the 60 s
  throttle drain for a walk that now does nothing. `route_must_scan_sub_dirs` needs no correctness change (it is a
  router; the walkers downstream are guarded), but the wasted drain slot is worth measuring.

## Housekeeping

- Listed in [`index.md`](index.md).
- Unrelated drift spotted while planning: `indexing/DETAILS.md:401` heads a section "Search stays single-volume (D7)",
  but `search/execute.rs::resolve_targets` (`:33-60`) now resolves and merges multiple volumes. Worth a separate fix.
