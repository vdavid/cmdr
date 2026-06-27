# Honest index sizes (exact / ≥lower-bound / unknown, plus fresh vs stale)

Status: planning (revised after adversarial review round 1). Created 2026-06-25. Worktree: `index-honest-sizes`.

## Why (the problem and the trigger)

A prod SMB index showed many non-empty folders as "0 bytes". Root cause traced from the prod DB + logs: the SMB
connection dropped mid-scan, and `volume_scanner` treats a per-directory listing error as "skip this one dir, keep
walking" (`continue`). On a whole-connection disconnect, _every_ remaining `list_directory_for_scan` failed instantly,
so the BFS churned through ~6,475 still-queued directories in ~1 s, recorded each as **empty**, drained the queue,
exited with `was_cancelled = false`, wrote `scan_completed_at`, and aggregated. The index looked "complete and Fresh
(green)" but was missing every folder queued after the disconnect.

The deeper defect is representational: one displayed value, "0 bytes", is overloaded to mean three different things —
_exact zero_ (genuinely empty), _not-yet-scanned_ (unknown), and _partially-scanned_ (a lower bound). The aggregator
writes a `dir_stats` row for every directory (unscanned ones get `recursive_logical_size = 0`), so "unknown" is
indistinguishable from "empty". That is why the disconnect produced confident lies instead of honest gaps.

This plan fixes the representation, which makes the disconnect bug (and several others) fall out as a consequence, and
lays the foundation for two future features David wants (lazy navigation-driven fill; browse a disconnected drive).

## Design principles in force

- **Rock solid + honest progress** (AGENTS.md §3): never show a confident number we don't have. A partial index must
  _say_ it is partial.
- **Protect the user's data** (§4): the index is a non-load-bearing cache; destructive ops already re-stat live
  (`write_operations/conflict.rs`), unchanged here. But the index must never _imply_ completeness it lacks.
- **Elegance above all** (§2): one self-describing data model unifying the disconnect case, the partial-scan case, the
  solo-NAS "stale but accurate" case, and the two future features — not many special cases.
- **Respect the user's resources** (§5): no double disk space (rejecting shadow-DB/atomic-swap); a continuity break is
  O(1), not an O(dirs) rewrite.
- David's workflow: solo NAS user. On launch/reconnect the data is technically stale but actually accurate, and he wants
  to _see_ those sizes (clearly flagged) while a rescan refreshes them, rather than a blanked-out index.

## The model: two orthogonal axes, one stored integer each

Today "0 bytes" conflates three meanings. Split into two independent facts:

**Per-directory coverage + freshness, from one stored epoch.**

- `entries.listed_epoch` (NEW, on the dir's own row): the volume epoch at which this directory's _direct contents_ were
  last successfully listed. `0` = never listed (discovered via its parent, its own contents unknown).
- `dir_stats.min_subtree_epoch` (NEW, rolled up bottom-up): `min` over `{this dir's listed_epoch}` ∪
  `{each child dir's min_subtree_epoch}`. Epochs start at 1 and unlisted = 0, so a single unlisted dir anywhere in the
  subtree drags the whole subtree's `min_subtree_epoch` to `0`. One integer encodes _both_ "is the subtree fully
  covered?" (`> 0`) and "how current is the oldest listing in it?" (the value).
- `recursive_logical_size` / `recursive_physical_size` / counts (EXISTING): reinterpreted as **lower bounds of what is
  known**. Exact iff `min_subtree_epoch > 0`.

**Per-volume `current_epoch`** (NEW, in `meta` + mirrored in the registry): bumped on every continuity break —
reconnect, watcher death, change-notify overflow, launch-loading-a-non-journaled-index-as-Stale, and **every full-rescan
trigger (local journal-gap / stale / overflow rescans included)**. A scan/reconcile _stamps_ listed dirs with the
current epoch; it does **not** bump it.

Derived at read time by comparing `min_subtree_epoch` to `current_epoch`:

| Stored state                                      | Meaning                        | Display                    |
| ------------------------------------------------- | ------------------------------ | -------------------------- |
| `min_subtree_epoch == 0`, `size == 0`             | nothing known below here       | `—` (unknown)              |
| `min_subtree_epoch == 0`, `size > 0`              | partially scanned, lower bound | `≥1.2 GB`                  |
| `min_subtree_epoch == current_epoch`              | exact and current              | `1.2 GB`                   |
| `0 < min_subtree_epoch < current_epoch`           | exact as of an older epoch     | `1.2 GB` + stale treatment |
| `min_subtree_epoch == current_epoch`, `size == 0` | genuinely empty, current       | `0 bytes`                  |

The crux: **`listed_epoch > 0` distinguishes a genuinely-empty `0 bytes` folder from an unknown `—` folder** — the
distinction the current schema cannot make.

### Why this shape (decisions and intentions)

- **Why an epoch counter, not a per-dir "stale" bit flipped by a recursive pass.** A recursive `UPDATE` on reconnect is
  O(dirs) write + WAL churn on every reconnect/launch — fine for naspi (~7 k dirs) but real for `root` (~538 k dirs,
  millions of entries). An epoch counter makes it O(1); per-dir staleness is _derived_ on read. It also distinguishes a
  freshly-rescanned `/a` from a still-stale sibling `/b` after a _partial_ rescan (the `/a /b /c` gap) without a full-DB
  write.
- **Why `listed_epoch` lives on `entries`, not `dir_stats`.** During a scan a dir's `dir_stats` row doesn't exist until
  aggregation; its `entries` row exists once its parent listed it. `listed_epoch` is set during the walk.
- **Why a `min` rollup (not sum/OR).** Coverage-and-freshness is "weakest link in the subtree." But see the propagation
  reality below: `min` is _not_ the same shape as the `recursive_has_symlinks` OR-rollup — it depends on the dir's own
  `listed_epoch`, not only on children — so its incremental maintainer is a distinct (if similarly-structured) walk.
- **Why carry derived booleans over IPC, not raw epochs.** The frontend shouldn't learn the epoch scheme. Enrichment
  (which has both the dir's `min_subtree_epoch` and the volume's `current_epoch`) sends `recursive_size_complete` and
  `recursive_size_stale`. Display is a pure function of `{size, complete, stale}`.
- **Why the epoch bump aligns with Fresh/Stale.** A continuity break ⟺ `current_epoch++` ⟺ volume Stale. The per-volume
  `Freshness` becomes consistent with `root.min_subtree_epoch == current_epoch ? Fresh : Stale` (modulo Scanning). We
  keep the enum as the badge summary; its Stale transitions also bump the epoch.
- **Why not a shadow DB + atomic swap:** double disk, all-or-nothing (no incremental fill), doesn't feed the future
  features.

## Scope and phasing

Four milestones, each independently valuable and testable. M1 (with its live-watch sub-part) makes any
partial/interrupted index honest end to end and fixes the "0 bytes" lie. M2 makes a mid-scan disconnect leave a visible
honest partial. M3 (perf+correctness-gated) makes rescans non-destructive. Plan-1/Plan-2 are out of scope but enabled.

The `SCHEMA_VERSION` bump forces a one-time index rebuild for every user on next launch (disposable cache → drop +
rebuild; no migration). Acceptable and expected, but it is a user-visible one-time full rescan — a conscious choice.

---

## Milestone 1 — Honest size data model (foundation), across ALL FOUR write paths

**Intention:** make the index self-describing so unknown ≠ empty ≠ lower-bound, end to end, for every volume kind
(local/SMB/MTP) uniformly. Critically, this must hold not only after a scan but under **live mutation** — the local
index spends ~all its life in live-watch mode, so the live path is first-class here, not a footnote.

The four write paths that must maintain `listed_epoch` / `min_subtree_epoch` honestly: (1) the **scanner** (jwalk
local + `volume_scanner` SMB/MTP), (2) the **aggregator** (full + partial), (3) the **live-watch delta propagation**
(`writer/entries.rs` + `writer/delta.rs`), and (4) the **verifier / reconcile** corrections.

### 1A. Schema (`store.rs`), bump `SCHEMA_VERSION` "12" → "13"

- `entries`: add `listed_epoch INTEGER NOT NULL DEFAULT 0`.
- `dir_stats`: add `min_subtree_epoch INTEGER NOT NULL DEFAULT 0`.
- `meta`: add `current_epoch` (TEXT; all meta values are TEXT). Seeded `"1"` at first scan; read by the scanner and the
  read side. **Degrade gracefully when absent** (older/first run): treat missing/`"1"` as epoch 1 so a volume with no
  recorded epoch behaves as "all current" rather than "all stale".
- Extend `DirStatsById` with `min_subtree_epoch: u64`; update `get_dir_stats_by_id`, `get_dir_stats_batch_by_ids`,
  `upsert_dir_stats_by_id`, and the path-keyed IPC `DirStats` struct.
- Keep `UNIQUE (parent_id, name_folded)` and `name_folded` exactly as-is (load-bearing). Additive columns only.

### 1B. Scanner stamps listed directories (path 1)

Both scanners must record "I successfully listed this dir at epoch E", **including empty dirs** (empty-but-listed →
`0 bytes`; unlisted → `—`).

- New writer message `MarkDirsListed { ids: Vec<i64>, epoch: u64 }` → PK-keyed
  `UPDATE entries SET listed_epoch=? WHERE id IN (...)` (PK lookup, no `platform_case` cost; cheap and batchable).
- **Mark mechanism (resolves the round-2 mark-before-row-flush hazard): accumulate, emit at the end, not per-dir.** A
  dir's `entries` row is inserted as part of its _parent's_ `InsertEntriesV2` batch, which flushes only at `BATCH_SIZE`
  — so a per-dir `MarkDirsListed` emitted right after listing a dir can run an `UPDATE` _before_ that dir's row has been
  flushed (it's still in the pending `batch` vec), matching zero rows and leaving it `listed_epoch=0` forever. Fix: the
  scanner accumulates the ids of every **successfully-listed** dir in a `listed_ids` vec and emits `MarkDirsListed`
  (chunked) **once after the final `flush_batch`** — at which point every entry row is committed-in-order — and
  **before** `ComputeAllAggregates`. On a mid-scan abort (M2 disconnect), flush the batch and emit the accumulated marks
  too, so the partial is honest. This auto-satisfies the ordering invariant below and avoids a flush-per-dir.
  - _Consequence (accepted):_ during the scan, every dir reads `listed_epoch=0` → partial aggregation shows all sizes as
    `≥` lower bounds (honest — they genuinely are mid-scan), snapping to exact at completion. This is _more_ honest than
    today's growing exact-looking numbers. Incremental marking (so a fully-walked early subtree shows exact mid-scan) is
    a possible later refinement, deliberately deferred to avoid the flush-ordering hazard.
- `volume_scanner.rs`: accumulate a dir's id on a **successful** `list_one_directory`, **including an empty result**
  (handled outside the `for entry in entries` loop so empty listings still mark). A listing that **errors does not
  mark** it. (S3.)
- `scanner.rs` (jwalk local): jwalk has no per-entry "I finished listing dir X" flag — derive listed-ness from jwalk's
  **per-directory read result** (its `process_read_dir`/client-state hook exposes each directory's children Vec,
  including an empty-but-successful read and a fully-errored readdir). Contract: mark every dir whose readdir
  **succeeded** (incl. empty); a dir whose readdir wholly failed stays `listed_epoch=0` (honest `—`; also fixes the
  FDA-denied-folder case from a misleading placeholder to honest unknown). Pin the exact jwalk hook during
  implementation. Note `run_scan` does **not** itself send `ComputeAllAggregates` — the local completion path does — so
  the marks-before-final-aggregate ordering point for local is the completion path, where the accumulated `listed_ids`
  must be emitted before the final aggregate.
- The scanner reads `current_epoch` once at scan start (seeding meta to `"1"` if absent) and stamps with it. **The epoch
  bump + persist for this scan must be committed before the scan thread reads it** (the scanner opens its own
  connection): sequence the bump write + flush before spawning the walk, else it stamps the stale epoch. (round-2 #9.)
- **Ordering invariant (the single most fragile point, must be explicit and tested):** every `MarkDirsListed` is
  enqueued **before** the final `ComputeAllAggregates`. Single in-order writer ⇒ this is an ordering contract: send all
  marks, then `ComputeAllAggregates`, then `WalCheckpoint`. A mark queued _behind_ the final aggregation leaves that dir
  at epoch 0 → whole tree rolls to `min_subtree_epoch=0` → a cleanly-scanned volume renders "≥"/Stale forever. The
  accumulate-and-emit-at-end mechanism above gives this for free.
- `MarkDirsListed` must NOT call `MutationTracker::bump()` (the bump is per-handler, not central — `UpdateMeta`/
  `DeleteMeta` already omit it): it changes nothing search cares about, so it must not thrash a root-search reload each
  scan. (N4.)

### 1C. Aggregator computes `min_subtree_epoch` (path 2)

- `compute_bottom_up`: per dir,
  `min_subtree_epoch = min(self.listed_epoch, min over child dirs' computed min_subtree_epoch)`, with `0` absorbing. It
  gains a **new input map `dir_id → listed_epoch`**.
- **All FOUR `compute_bottom_up` callers must supply that map (round-2 critical #4 — the prior draft named only two):**
  1. `compute_all_aggregates_with_maps`: read `dir_id → listed_epoch` from `entries` in the same scan that already loads
     `load_all_directory_ids` (cheap, no extra full scan). NOT carried in `AccumulatorMaps` (those are keyed by
     `parent_id`, never see a dir's own epoch, and the mark arrives via a separate message). The accumulator is
     unchanged (C3 correction).
  2. `compute_all_aggregates_reported` (SQL path): read it from `entries` likewise.
  3. `compute_subtree_aggregates` (runs after every `scan_subtree`, via `ComputeSubtreeAggregates`): supply a **scoped**
     `listed_epoch` read for the subtree (mirror its scoped CTE child queries).
  4. `backfill_missing_dir_stats` (after reconciler / cold-start replay): supply the full-table read. If any caller
     passes an empty/None map, every dir it touches gets `min_subtree_epoch=0`, re-breaking coverage after each subtree
     scan or backfill — so all four are mandatory, not optional.
- **Partial path (`compute_partial_aggregates`) invariants (S4):** still derives its dir list from the **borrowed maps**
  (NOT a SQL dir list — that would be the forbidden empty-maps fallback, DETAILS gotcha), and still **no-ops on empty
  maps**. Enrich with `listed_epoch` via a **single batched `WHERE id IN (...)`** read for the dirs already in the maps
  (not per-dir N+1 — this runs frequently mid-scan). Writes a depth-≤3 subset as today; the in-memory min is honest for
  what it writes (deep unlisted children → 0). Don't add a SQL fallback.

### 1D. Live-watch + delta propagation (path 3) — the path local lives on

This is the part the first draft missed (review C1/C2). After a scan, all mutations go through `UpsertEntryV2` /
`DeleteEntryById` / `DeleteSubtreeById` / `MoveEntryV2` / `propagate_delta_by_id` (`writer/entries.rs`,
`writer/delta.rs`). Two failure modes to prevent, both of which would make a Fresh _local_ volume start lying:

1. **Never default-reset `min_subtree_epoch`.** `propagate_delta_by_id` (`delta.rs`) already does read-modify-write and
   carries `recursive_has_symlinks` through unchanged — so the fix is just to **add `min_subtree_epoch` to the carried
   tuple** (size/count deltas don't change coverage; no signature change). The real default-reset risks are narrower and
   must be handled explicitly: (a) the **zero-init `dir_stats` literal for a NEW dir** in `handle_upsert_entry_v2` —
   here `min_subtree_epoch: 0` is _correct_ (a new dir is unlisted), so set it to 0 deliberately; (b) any
   **`None`-branch row construction** in `delta.rs` that builds a fresh row — must carry the existing value, not the
   default. So: preserve on the bump paths, 0 only on genuine new-dir creation.
2. **Propagate coverage changes.** Coverage (`min_subtree_epoch`) changes when the _tree shape_ changes, not when a file
   size changes:
   - A **new directory** created live (`UpsertEntryV2` for a dir) has `listed_epoch = 0` (no scanner listed its
     contents) — correct/honest — and its appearance must drop every ancestor's `min_subtree_epoch` to `0` (a new
     unlisted subtree exists). A later verifier `scan_subtree` (1E) stamps it and lifts coverage back.
   - A **deleted** subtree may _raise_ a parent's `min_subtree_epoch` (the incomplete child is gone).
   - A **move** changes coverage on **both** ancestor chains: fire `propagate_min_subtree_epoch` on the old `parent_id`
     and the new `parent_id` (the precedent is `handle_move_entry_v2`'s existing dual-chain `recursive_has_symlinks`
     recompute). The moved subtree's own `min_subtree_epoch` is unchanged (it moved intact) — do not recompute it, only
     the two ancestor chains. Add `propagate_min_subtree_epoch(conn, start_id)`: structurally like
     `propagate_recursive_has_symlinks` (walk the parent chain, recompute, short-circuit when the stored value
     stabilizes) **but the per-dir recompute reads `self.listed_epoch` AND every child dir's `min_subtree_epoch`** (not
     children-only — that's the C1 difference; the OR-precedent does not cover the self-dependence). The
     short-circuit-on-stable still holds. Fire it from the create/delete/move handlers alongside the existing size-delta
     and symlink-flag propagation.
   - Note: `min` is monotone-down on coverage loss and monotone-up on coverage gain; the recompute-from-children +
     stop-when-stable pattern handles both (same as the OR aggregate's removal-recompute case).

### 1E. Verifier / reconcile corrections (path 4)

The live _fill_ path is `reconcile_subtree`, NOT `scan_subtree` (round-2 critical #3): the per-navigation verifier, the
SMB-overflow `FullRefresh`, and the `MustScanSubDirs` path all go through `reconcile_subtree`, which does its **own**
directory walk via `UpsertEntryV2` + recursion and never calls `scan_subtree`. So:

- `reconcile_subtree` must **mark every directory it lists** (it has `dir_id` in hand each iteration; include empty
  listings) at the current epoch, and fire `propagate_min_subtree_epoch` after. Without this, a reconcile-discovered
  subtree is fully listed on disk but stays `listed_epoch=0`, permanently dragging ancestors to incomplete — a
  regression on exactly the local live path this milestone protects.
- The verifier's `scan_subtree` path (used for some newly-discovered dirs) likewise stamps `listed_epoch` (reuse the 1B
  accumulate-and-mark, emitted before its `ComputeSubtreeAggregates`), and post-scan propagation lifts ancestors.
- For **deletions** found by verification: propagate the possible coverage _raise_ (1D #2).
- Leaving any of these un-epoched lets post-scan verification quietly desync coverage from reality (review N6).

### 1F. Enrichment carries derived booleans (read side)

`enrichment.rs`: when applying `DirStatsById` to a `FileEntry`, read the volume's `current_epoch` once per pass (from
meta via the same `ReadPool` conn; absent ⇒ 1) and set:

- `recursive_size_complete = stats.min_subtree_epoch > 0`
- `recursive_size_stale = stats.min_subtree_epoch > 0 && stats.min_subtree_epoch < current_epoch`
- `recursive_size` stays the (lower-bound) value. **There are TWO read surfaces, both need the derived booleans (round-2
  #6):** (a) the `FileEntry` enrichment path (1G), and (b) the **path-keyed `DirStats` IPC struct** (`store.rs`) built
  in `get_dir_stats_on_volume` / `get_dir_stats_batch_on_volume` (`queries.rs`), which is what `refreshIndexSizes`
  consumes. Add `recursive_size_complete`

* `recursive_size_stale` to `DirStats` too (derive them backend-side from `min_subtree_epoch` vs `current_epoch`; do NOT
  ship raw epochs to the FE). The batch path reads `current_epoch` once per call; the single `get_dir_stats_on_volume`
  reads it within its `with_conn`. The `..` parent row renders from the dir's own stats (DETAILS), so a
  partially-scanned current dir shows `..` as `≥` — confirm it carries the flags (N3). Unindexed volumes still skip
  enrichment entirely (`get_read_pool_for` → `None`), unchanged.

### 1G. `FileEntry` + bindings

Add `recursive_size_complete: Option<bool>` and `recursive_size_stale: Option<bool>` to
`file_system/listing/metadata.rs::FileEntry`, default `None` in `new()`. Regenerate `bindings.ts`.

### 1H. Cross-cutting consumers (don't let lower-bounds leak into load-bearing math)

- **`expected_totals_for_sources()`** (write-op progress denominator): must return `None` for any source whose subtree
  is incomplete (`min_subtree_epoch == 0`), exactly as it already returns `None` when a source isn't indexed — else a
  partial lower-bound size makes copy/move/delete progress bars overshoot 100%. (review N1.)
- **Destructive ops:** unchanged — they re-stat live (`conflict.rs`); the index is never load-bearing there. Re-confirm,
  don't modify.

### 1I. Frontend display (paths → pixels)

`full-list-utils.ts::getDirSizeDisplayState` + `FullList.svelte`. Today returns
`'dir' | 'scanning' | 'size' | 'size-stale'`, where `'size-stale'` means _in-flux during a scan/pending write_ —
**rename to `'size-updating'`** to free the word "stale" for the freshness concept. New mapping from
`{recursiveSize, complete, stale, indexing, pending}`:

- `complete === false && size === 0` → `—` (unknown)
- `complete === false && size > 0` → `≥` prefix on the formatted size
- `complete === true && stale === false` → formatted size
- `complete === true && stale === true` → formatted size + stale treatment
- the in-flux `indexing || pending` hourglass overlay is orthogonal and still applies.
- `≥` attaches in the size-formatting path (`getDisplaySize` / `formatSizeForDisplay`); `—` is a distinct render;
  tooltip (`buildDirSizeTooltip`) gains a one-line state label.
- **Stale visual (DEFAULT, tunable):** muted number (reduced opacity / secondary text color), matching the yellow=stale
  freshness language of the per-drive badge, explanation in the tooltip. No new icon. Easily retuned — flagged for
  David, not a blocker. (Open decision #1.)
- **Sort-by-size semantics (N2):** define explicitly. Unknown (`—`) and lower-bound (`≥`) must not silently sort as if
  exact-0/exact-N and re-conflate what we just separated. Proposal: sort by the known numeric value, but order unknown
  (`—`, no data) consistently at one end (e.g. treated as the smallest, after genuine 0-byte dirs) with a stable
  tiebreak; lower-bounds sort by their known floor. Implement in the sort comparator + reflect in
  `measure-column-widths` if width depends on glyphs.
- All user-facing strings via `t()`/`getMessage()` with `@key` descriptions (the `≥`/`—` glyphs are symbols, not
  translatable copy; tooltip labels are).

### Tests (M1)

- **TDD (red→green), aggregator:** tree with a listed parent, a listed-empty child, an unlisted child → parent
  `min_subtree_epoch == 0`; listed-empty child `> 0` with size `0` (genuinely empty); unlisted child `0`.
- **TDD (red→green), scanner:** `volume_scanner` over an `InMemoryVolume` where one subdir's listing errors → that
  subdir is **not** marked → `min_subtree_epoch == 0`, parent incomplete, siblings exact. Unit-level disconnect-shaped
  anchor.
- **TDD (red→green), mark ordering (C4):** assert all `MarkDirsListed` precede the final `ComputeAllAggregates` and a
  clean scan yields `root.min_subtree_epoch == current_epoch` (catches the render-Stale-forever race).
- **TDD (red→green), LIVE PATH (the highest-risk new code, review N7):** after a clean scan of a dir (epoch E,
  complete), a live `UpsertEntryV2` file-add into it keeps `min_subtree_epoch == E` (NOT reset to 0) and bumps size. A
  live new _directory_ drops ancestors to `min_subtree_epoch == 0`; a subsequent `scan_subtree` stamps it and restores
  coverage. A subtree delete raises the parent's coverage appropriately. (`writer/delta.rs::tests`,
  `writer/entries.rs::tests`.)
- **After:** extend `stress_tests_partial_aggregation.rs` to assert byte-identical **final** `min_subtree_epoch` between
  final-only and partial-interleaved arms (S5: the equality is on the _end_ state; during the run the partial arm
  legitimately differs). Recompute-from-`entries` oracle covers map corruption.
- **After:** store round-trip for the new columns; schema-bump drop+rebuild still works; `expected_totals` returns
  `None` for an incomplete source.
- **After (Vitest):** `getDirSizeDisplayState` + formatting truth table for all five states incl. `≥`/`—`; sort
  comparator places unknown/lower-bound correctly.
- **Docs:** `indexing/CLAUDE.md` must-know ("size is a lower bound unless `min_subtree_epoch>0`; `listed_epoch`
  distinguishes empty from unknown; live writes preserve epoch, tree-shape changes re-min up the chain");
  `indexing/DETAILS.md` new "Honest sizes" section (two axes, rollup, display table, the four-write-path discipline,
  decisions); `src/lib/indexing/CLAUDE.md` (FE rendering + sort); schema section in DETAILS.
- **Checks:** `pnpm check rust`, `pnpm check svelte`, `pnpm check`; `--include-slow` before wrapping.

---

## Milestone 2 — Honest disconnect handling + per-dir freshness wiring

**Intention:** a mid-scan SMB/MTP disconnect should stop immediately (not churn thousands of failing listings into empty
rows), keep the partial _visible and honest for the session_, and mark the volume Stale — instead of lying "complete"
(today's bug) or blanking to gray (today's error path).

1. **Terminal-vs-transient listing errors** (`volume_scanner.rs`). Keep per-dir skip-and-continue for a single
   transient/permission failure; match the **typed** `VolumeError::DeviceDisconnected(_)` (and `Disconnected`) — never a
   string (`.claude/rules/no-string-matching.md`) — and treat it as terminal: stop the walk and return a typed
   `VolumeScanError` the completion handler routes to "disconnected-interrupted". Backstop: also abort on **N
   consecutive listing failures** (any disconnect-shaped error that doesn't map to the typed variant), logging what was
   abandoned (no silent truncation).
   - **The terminal-abort branch must do the partial-preserving write sequence in ONE place (round-3 SF-1).** Today only
     the `cancelled` branch flushes; the error returns (root-fatal and the catch-all `Err(other)`) return _without_
     `flush_batch` and without emitting marks, so they'd drop the last in-flight batch (up to `BATCH_SIZE`) and every
     accumulated `MarkDirsListed`. The new terminal-disconnect branch must, before returning `Err`: (a) `flush_batch`,
     then (b) emit the accumulated `MarkDirsListed` for already-listed dirs, then (c) emit `ComputeAllAggregates`. It
     must **not** write `scan_completed_at`. (b)+(c) are what make the kept partial honest — see §2.

2. **Completion-handler branch (S2 — this branch lives in `manager.rs`, NOT the freshness table).** The disconnect-vs-
   cancel split is a `match result` branch in `start_volume_scan`'s completion handler (currently the single `other =>`
   arm), not a freshness-enum change. The CLAUDE.md guardrail "freshness has ONE transition table; don't branch
   elsewhere" forbids adding _freshness states_ outside the table — it does not forbid the completion handler from
   choosing which freshness _event_ to apply. Two outcomes:
   - **User cancel** (`Ok(summary)` with `was_cancelled`): keep today's behavior — `reset_to_not_indexed` / heal-to-
     rescan; the partial is discardable.
   - **Disconnect** (`Err(VolumeScanError::Volume(DeviceDisconnected))` / consecutive-failure abort): **keep the
     instance + DB**, do not write `scan_completed_at`, apply `WatcherDied` (⇒ Stale) and bump `current_epoch`. The
     partial is now served and honest (unscanned subtrees `—`/`≥`, scanned ones exact-but-stale).
     - **Run a final `ComputeAllAggregates` on the abort (round-3 SF-2).** `dir_stats` rows (hence `min_subtree_epoch`)
       are written only by aggregation; the mid-scan partial passes write only a depth-bounded subset, so without a
       final aggregate a fully-listed _deep_ subtree would have no row and render `—` despite being exact. Running
       `ComputeAllAggregates` over what's present rolls marked subtrees up to `min_subtree_epoch = epoch > 0`
       (exact-but-stale once the epoch is bumped) and unmarked ones to `0` (`—`/`≥`) — exactly the honest partial. This
       is NOT the forbidden action: the forbidden thing on an interrupted scan is writing `scan_completed_at`, which we
       still skip. (The `volume_scanner` terminal branch already emits this per §1.c.)
   - Reconcile with the buffered-changes lifecycle: a disconnect path calls `discard_buffered_changes` (the buffer is
     meaningless), same as the existing interrupt; only the gray-reset is replaced by keep-instance-Stale.

3. **Relaunch coherence (S1 — stated limitation, fully resolved by M3).** With truncate-first still in place (pre-M3),
   the kept partial is **session-scoped**: on relaunch, `resume_or_scan` sees no `scan_completed_at` ⇒
   `IncompletePreviousScan` ⇒ a fresh scan (which truncates). So a disconnected-at-relaunch SMB user gets a (failing)
   rescan, not the preserved partial. This is the accepted limitation of M2 alone; **M3 resolves it properly** — once
   rescans reconcile instead of truncate, the prior _complete_ index is never destroyed in the first place, and a
   persisted index is shown stale-but-whole. Do not try to fake cross-relaunch partial persistence with
   `scan_completed_at` semantics — one flag cannot mean both "heal to rescan" and "serve this partial".

4. **Epoch bump sites + launch (round-2 #7 — bump at the funnels, not via the notification enum).** Every full (re)scan
   funnels through `start_scan` (local) and `start_volume_scan` (SMB/MTP) regardless of trigger (the `RescanReason`
   variants are FE-toast notifications, not control-flow points — enumerating them invites missed cases like
   `WatcherChannelOverflow`/`ReconcilerBufferOverflow`). So bump + persist `current_epoch` **at scan start in those two
   funnels** (this covers reconnect rescans, journal-gap/stale/overflow rescans, and `force_scan`), plus on the
   non-rescanning continuity breaks `on_smb_watcher_died` / `on_smb_overflow` / `on_mtp_device_disconnected` and the
   disconnect completion branch (2). The first-ever scan also bumps (1→2 with nothing yet at epoch 1) — benign; the
   "continuity break ⟺ bump" framing should note the first-scan bump is harmless. The bump write+flush must precede the
   scan thread reading `current_epoch` (1B note / round-2 #9).
   - **Launch.** `initial_freshness_on_launch` is a **pure function** `(scan_completed_at_present, journaled)` with no
     volume id / DB handle — it CANNOT bump. The launch bump for a non-journaled (SMB/MTP) index that loads Stale
     happens at the **call site** in `start_indexing_for` (volume id + store in hand): on loading Stale, bump and
     persist `current_epoch`, so persisted dirs read stale-but-visible (David's solo-NAS case). A journaled local index
     loading Fresh does not bump (and skips a rescan, so its funnel bump doesn't fire) → stays Fresh, exact.
   - **`clear_index`** deletes the DB (gray, no instance) — no epoch concern. **`disable` then re-enable** resumes the
     persisted DB; the re-enable path is a `start_indexing_for` ⇒ launch-bump rule applies.

5. **Per-volume Freshness derivable.** Keep the `Freshness` enum as badge summary; verify/document it equals
   `root.min_subtree_epoch == current_epoch ? Fresh : Stale` (modulo Scanning). The badge behavior is unchanged; this is
   the consistency check that the two layers can't drift.

### Tests (M2)

- **TDD (red→green):** stub volume returns `DeviceDisconnected` after K of N dirs → the walk stops promptly (does not
  attempt the remaining N−K), returns the typed disconnected error, and the completion path writes **no**
  `scan_completed_at`. Direct regression test for the reported prod bug.
- **TDD (red→green):** the consecutive-failure backstop trips after N and aborts.
- **After:** disconnect completion branch keeps the instance alive (ReadPool present → sizes still served), marks Stale,
  bumps `current_epoch`; user-cancel still resets to gray. (`manager.rs`/`state.rs`/`freshness.rs` tests.)
- **After:** launch-as-stale bumps the epoch at the call site so persisted dirs read stale; local-rescan triggers bump;
  local Fresh load does not.
- **After (integration, Docker SMB if feasible):** mid-scan disconnect leaves a navigable honest partial (scanned dirs
  exact-but-stale, unscanned `—`), not gray, not lying. Mirror of `smb_integration_enrich_listing_shows_sizes`.
- **Docs:** DETAILS "SMB indexing and the freshness model" — replace "interrupted SMB scan ⇒ gray" with the new
  "disconnect ⇒ keep honest partial + Stale (session-scoped pre-M3); user-cancel ⇒ heal-to-rescan" split, plus the
  completed epoch-bump-site list; CLAUDE.md guardrail update.

---

## Milestone 3 — Non-destructive rescan (reconcile, not truncate) — BENCHMARK + CORRECTNESS GATED

**Intention:** today every (re)scan `TruncateData`s up front, so a rescan blanks the index until rebuilt and a
mid-rescan disconnect loses the prior _complete_ data. Reconcile in place so the last-good index is never blanked or
regressed: epoch bump marks everything stale, the walk re-stamps and diffs per directory, unchanged rows are never
rewritten. Also the proper fix for M2's relaunch limitation (S1).

**The perf-sensitive milestone and the natural decision gate.** Truncate-first exists because `INSERT OR REPLACE` on a
populated DB is catastrophic (~30 min vs ~2.5 min on 5.5 M entries, `platform_case` B-tree cost) and un-truncated
`INSERT OR IGNORE` orphans rows (3–4× bloat). Reconcile avoids both _only if_ it writes solely changed rows. The
existing `reconcile_subtree` (verifier) already diffs per-dir, so the machinery exists; M3 routes the full rescan
through it.

### M3.0 — Gate (benchmark AND correctness), reported to David before M3.1+

- **Perf:** reconcile-no-op cost (nothing changed) on naspi (~63 k entries) and `root` (~538 k dirs);
  `min_subtree_epoch` re-min propagation cost on `root`; vs today's truncate + bulk-insert.
- **Correctness (review S6 — not just cost):** prove an **interrupted** reconcile leaves no orphan/ghost rows across
  _repeated_ disconnect→reconnect→reconcile cycles (the 1.83 TB ghost-size class). Truncate-first swept orphans every
  scan for free; reconcile only does if the diff is exhaustive, and a mid-reconcile disconnect makes it non-exhaustive.
  An orphaned unlisted subtree (old epoch) would silently drag a parent's coverage to incomplete.
- If perf is unacceptable or correctness can't be guaranteed cleanly: fall back to keep-truncate-for-local /
  reconcile-network-only, or keep M1+M2 and defer M3. **This is the one planned pause** — report findings, then decide.

### M3.1+ — Implementation (only if the gate passes)

- Route rescan through a reconcile walk: per listed dir, diff DB children vs live listing (add new, delete gone, update
  changed size/mtime), set `listed_epoch = current_epoch`, never rewrite unchanged rows. Reuse/generalize
  `reconcile_subtree`'s diff; keep `next_id` from the shared `Arc<AtomicI64>` (never `MAX(id)`); IDs growing across
  rescans is harmless (i64).
- Drop up-front `TruncateData` for rescans; keep it only for a true first scan / `clear_index` rebuild.
- Epoch bump at rescan start (continuity break) → tree shows stale-but-visible; each reconciled dir flips fresh as
  re-listed (partial-rescan `/a` fresh / `/b` stale).
- Preserve pre-arm-before-snapshot live-change buffering, adapted (no truncate to race).
- **No orphan sweep.** Evaluated and dropped: after a complete reconcile the per-dir delete branch has already pruned
  every gone child under a re-listed parent, so an epoch sweep keyed on `listed_epoch < rescan_epoch` AND parent
  re-listed this epoch matches nothing real — only present-on-disk dirs whose own listing failed transiently, which it
  would wrongly delete. The interrupted→complete self-heal needs no sweep.

### Tests (M3)

- Benchmark + correctness harness output recorded in `docs/notes/`, linked from DETAILS.
- Reconcile diff correctness (add/remove/modify/type-change) — extend verifier-test precedent for the full-rescan entry
  point and epoch re-stamp; deletion of vanished entries covered.
- Rescan over an unchanged tree writes zero entry rows (the no-op-cheap property the gate relied on).
- Mid-rescan disconnect leaves prior complete data intact (now possible, no truncate), marked stale.
- Repeated disconnect→reconcile cycles accumulate no orphans/ghost sizes (the S6 correctness gate, as a standing test).
- **Docs:** DETAILS — replace truncate-first rationale with the reconcile model + benchmark/correctness evidence anchor;
  update the "INSERT OR REPLACE catastrophic" gotcha to explain why reconcile sidesteps it.

---

## Future (out of scope here; the model enables these)

- **Plan 1 — lazy navigation-driven fill.** A non-recursive navigation listing is a per-dir reconcile that sets
  `listed_epoch`; when all of a parent's children are complete, its `min_subtree_epoch` flips `> 0` and its size becomes
  exact — for free via the rollup. Same write path as M3, triggered by navigation. Requires FS-watching connected drives
  even when indexing is "off" (watch scope bounded to filled subtrees — open question for that effort) and a precise
  definition of indexing ON/OFF once lazy fill exists.
- **Plan 2 — browse a disconnected drive.** Serve listings from the index when the volume is gone; complete/stale/`—`
  flags make it honest. This model is the prerequisite.

## Parallelization notes

Largely sequential (each milestone builds on the prior schema/fields). Within M1, the frontend display work (1I + its
Vitest) can proceed in parallel with the backend once the `FileEntry`/bindings shape (1G) is fixed — they meet only at
the typed boundary. The four backend write-path pieces (1B–1E) share the schema (1A) and the
`propagate_min_subtree_epoch` helper (1D), so do 1A → 1D-helper → then 1B/1C/1E; not worth parallelizing. We're not in a
hurry; sequential is fine.

## Open decisions to confirm with David

1. Stale visual treatment (1I): muted-number default proposed; David may want a specific affordance.
2. M3 go/no-go and any local-vs-network split: decided by the M3.0 perf+correctness gate, reported to David.
3. The schema bump forces a one-time full rescan for all users on upgrade — confirmed acceptable (disposable cache).
4. M2 partial is session-scoped pre-M3 (relaunch heals to rescan); confirm that's acceptable as an interim, given M3
   resolves it.
