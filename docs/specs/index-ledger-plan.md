# Drive-index aggregate integrity: the dir_stats ledger

2026-07-16. Worktree `index-ledger`, branch `david/index-ledger`. Reviewed 6× (fresh-eyes agents): rounds 1-4 findings folded in, round 5 clean; round 6 covered the removal-storm-coalescing addition (root cause 7), 8 findings folded in.

## The incident that motivated this

David deleted `~/projects-git/vdavid/cmdr/.claude/worktrees/e2e-budget/target` (~60 GB) from the terminal while
`pnpm dev -m` ran with background agents churning sibling worktrees. Observed: no hourglass, the parent folder's size
stayed at the stale 60 GB, then decreased only when navigating back and forth, then settled at a confident **"0 bytes"**
for a folder that really holds 1.21 GB / 72,594 files — and stayed there.

Forensics on the live `index-root.db` (2026-07-16, dev instance):

- `e2e-budget`'s `dir_stats` row: `0 bytes / 0 files / 1,492 dirs`. Truth from its `entries` subtree:
  `1.21 GB / 72,594 files / 12,346 dirs`. An internally impossible aggregate, stored and served.
- `worktrees`' stored `recursive_file_count` (489,696) is ~102k BELOW the sum of its direct children's stored rows
  (591,661): ancestors were debited for the delete but were never credited for growth that happened before it.
- The session log shows ~69k `skipped removals for unknown paths` (benign: children of an already-subtree-deleted dir)
  and 14k+ `skipped events for missing parents` (NOT benign: dropped credits).

## Root causes (all confirmed in code)

The system keeps `dir_stats` as an incrementally delta-adjusted ledger. **Debits are exact** (computed from the
`entries` table via recursive CTE at delete time), but **credits leak or race** on several paths — and the mismatch is
then silently clamped. Drift is guaranteed; only its speed varies.

1. **Leak A — subtree-scan ancestor credits are off-writer compensation that double-counts and silently skips.**
   `scanner::scan_subtree` (called for NEW dirs discovered by post-replay background verification and the navigation
   verifier) runs `DeleteDescendantsById` (deliberately no debit) → `InsertEntriesV2` (no credit) →
   `ComputeSubtreeAggregates` (recomputes ONLY rows inside the subtree; walks ancestors only for
   `recursive_has_symlinks`). Both production callers (`verifier.rs` ~437–451,
   `event_loop.rs::run_background_verification` ~1164–1219) then compensate manually: flush the writer, read the new
   dir's computed `dir_stats` off the `ReadPool`, and send `PropagateDeltaById(parent, +totals)`. The flushes DO
   order the read after the aggregate (no read-before-aggregate race), but the compensation still leaks two ways:
   (a) **concurrent-live double-count** — the live event loop runs during verification by design
   (`event_loop.rs` ~901–903), so live upserts inside the just-discovered subtree credit ancestors, then
   `DeleteDescendantsById` removes those rows with no debit and the full-totals compensation credits the same bytes
   AGAIN; (b) **silent skips** — `get_read_pool()` returning `None`, a `resolve_path` miss, or a failed flush drops
   the credit for a whole subtree with no trace. The structural fix is to move the ancestor adjustment INTO the
   writer message (ordering and atomicity are then free) and delete the compensation.
2. **Leak B — missing-parent live events are dropped.** `reconciler.rs::handle_creation_or_modification`, the
   `Ok(None)` parent-resolve arm: a creation whose parent isn't in the index yet is skipped (`STALE_PARENT_SKIPS`).
   `reconcile_subtree` has its own variant: "neither root nor parent in DB, skipping" (~line 794) — a
   `MustScanSubDirs` for a path on a missing chain is dropped the same way. The entries usually land later via some
   other rescan, but nothing guarantees it, and during deep-tree creation storms (cargo, `git worktree add`) many
   credits die here.
3. **Leak C — backfill writes missing rows without crediting ancestors.**
   `aggregator::backfill_missing_dir_stats` computes correct stats for dirs that have entries but no `dir_stats` row,
   writes ONLY the missing rows, and never adjusts ancestors that were, by construction, never credited (a missing row
   means no delta ever walked through that dir).
4. **Leak D — accumulator-map pollution turns the NEXT full aggregate into whole-tree corruption.**
   `handle_insert_entries_v2` accumulates into the writer's `AccumulatorMaps` unconditionally; a subtree scan's
   batches therefore populate the maps with subtree-only data, and `handle_compute_subtree_aggregates` never clears
   them. The next `ComputeAllAggregates` on that long-lived writer (journal-gap rescan, `force_scan`, stale-launch —
   all route through `finish_reconcile`) picks the maps path whenever `!direct_stats.is_empty()`
   (`writer/aggregation.rs` ~175) and `compute_all_aggregates_with_maps` treats the maps as the ONLY direct-stats
   source for EVERY dir — every dir outside the polluted subtree rolls up from zero direct contributions.
   Incident-class, whole-tree, and independent of the other leaks. (The DETAILS claim "after a reconcile the maps are
   empty" holds only if no subtree scan ran earlier in the session.) And the windows are real, not theoretical:
   `run_background_verification` is SPAWNED concurrently (`event_loop.rs` ~907) and the replay-overflow
   `TooManySubdirRescans` fallback full rescan can start while its `scan_subtree` batches are still in flight; a
   `force_scan` doesn't cancel an in-flight navigation verification either (its `cancelled` flag is a never-set
   local). So a reconcile's `ComputeAllAggregates` can land BETWEEN a verification's `InsertEntriesV2` batches and
   their `ComputeSubtreeAggregates` — meaning a clear-the-maps-in-the-subtree-handler fix alone does NOT close the
   window; the full-aggregate senders must declare their source explicitly.
5. **The sink — `.max(0)` clamps over-debits into permanent lies.** `writer/delta.rs::propagate_delta_by_id` clamps
   each field independently, in BOTH branches (eight clamp sites: four in `Some`, four in `None`). When an exact debit
   exceeds a drifted-low balance, bytes/files floor to 0 while dir_count keeps a positive remainder — the exact stored
   fingerprint above. The `None` branch is its own trap: a negative delta hitting a MISSING row (which is Leak C by
   construction) silently materializes a zeroed row. Nothing on the live path ever corrects these; the wrong rows
   survive until a full rescan.
6. **The invisibility — the coalesced-delete path is silent to the UI.** `reconciler.rs::process_live_event` early
   returns on `must_scan_sub_dirs`, so `pending_paths` (which feeds both `pending_sizes.mark` and the
   `index-dir-updated` emit) never sees the affected dirs. The detached `reconcile_subtree` mutates the DB with no
   hourglass and no pane refresh; only navigation (the verifier + `getDirStatsBatch` refetch) made changes visible.
7. **The slowness — a bulk delete is processed as a per-file event storm, minutes-scale.** `rm -rf` works depth-first
   (unlink all files, THEN rmdir each emptied dir; the deleted root itself comes LAST), and FSEvents reports that
   order faithfully — so the fast one-`DeleteSubtreeById` path fires only at the very END of the storm, after the
   reconciler has already chewed through hundreds of thousands of individual removals (stat + resolve + delete +
   O(depth) ancestor walk, each). Incident evidence: one writer batch carried 19,953 individual deletes; overall
   throughput was ~1–3k events/s, so a 60 GB / ~500k-entry `target` costs 2–5 MINUTES of background churn (David
   watched the ancestor size tick down ~1 GB per navigation). The 69k "unknown path" skips were the tail where
   interleaved dir-removal events had already subtree-deleted chunks — proof the cheap mechanism exists but engages
   too late because event order hands us the leaves first.

Related but NOT bugs (verified, keep as-is):

- Unknown-path *removals* skipped in `handle_removal` are correct: nothing in the index ⇒ nothing to debit.
- Cross-parent `MoveEntryV2` debits and credits the same stored value — internally consistent even if that value has
  drifted (the drift is the leaks' fault, not the move's).
- `BulkReconcileGuard` / `SetDeltaPropagation(false)` for FULL reconciles is deliberate and perf-load-bearing (the
  hours-long writer wedge; DETAILS § "Decision: the full reconcile suppresses per-entry ancestor propagation"). The
  final `ComputeAllAggregates` makes those walks self-consistent. **Do not touch this.**

Known, accepted drift windows (documented, not fixed here):

- **A cancelled full local reconcile** (`local_reconcile.rs` cancel arm, ~361–367) exits with no marks and no final
  aggregate while its walk ran under `SetDeltaPropagation(false)` — entries already diffed have no ancestor
  propagation until the next COMPLETED rescan (next launch takes the `IncompletePreviousScan` path). Accepted: the
  index reads stale, and the heal is the already-existing next-rescan flow. M3 fixes the in-code comment that
  currently claims (falsely) that partial writes "stay size-consistent" there, and M4a's stress-test invariant carves
  this window out explicitly.
- **A cancelled `scan_subtree`** would be destructive-without-repair (`DeleteDescendantsById` already sent, no
  aggregate on the cancel path). Currently latent — both callers pass a never-set `AtomicBool` — but it violates
  ledger rule 2 structurally, so M2 makes the cancel arm still send `ComputeSubtreeAggregates` (which after M2
  repairs ancestors too). Cheap insurance for when a real cancel signal arrives.

## Design

### The ledger rules

One sentence: **every path that changes `entries` must leave ancestor `dir_stats` consistent, and any place that
cannot apply an exact delta escalates to a scoped recompute instead of dropping or clamping.**

The FSEvents stream is an accurate per-file log (the incident log proves delivery: all ~69k removals arrived), so
delta roll-on stays the primary path — it is near-optimal (a 60 GB subtree delete is one CTE + one subtree delete +
O(depth) row updates, well under a second). What was missing is *discipline* on the rare paths, not a different
architecture. Three principles:

1. **Deltas stay the fast path** for single-entry live mutations (upsert/delete/move), computed on the writer thread
   against the rows it holds — keep that.
2. **Structural rewrites propagate their net effect, on the writer.** Any operation that replaces a subtree wholesale
   (subtree scan, backfill) finishes by repairing the ancestor chain (below) inside the writer's message handling —
   never via off-writer read-then-credit, which is exactly what races today (Leak A).
3. **Detected drift triggers repair, never a clamp.** A subtraction that would go negative is arithmetic *proof* the
   stored balance is wrong. That moment is exactly when we know something is off and exactly where (this dir, this
   chain) — repair there, log it, continue with corrected values.

The rule of thumb the code should read as: *delta when you know exactly what changed; repair when you know something
changed but not exactly what.*

### The repair primitive: `repair_dir_stats_upward`

New function in `writer/delta.rs`, the universal escalation:

```
repair_dir_stats_upward(conn, start_id):
    current = start_id
    while current != 0:
        fresh = recompute one level from committed rows:
                  SUM over direct file children (entries) +
                  SUM over direct child dirs' stored dir_stats rows
                  (sizes, file/dir counts; recompute has_symlinks + min_subtree_epoch
                   with the SAME per-level semantics as the existing walkers)
        if fresh == stored row: break          # short-circuit, like the symlink/epoch walkers —
                                               # compares ALL fields; an epoch-only or symlink-only
                                               # difference keeps walking (coverage restoration
                                               # depends on it, see M2's flow test)
        write fresh; current = parent          # stop after ROOT_ID, same boundary convention
```

Intentions behind the shape:

- **Recompute-from-children, not from the whole subtree.** One indexed SUM per level (O(children)), never a recursive
  CTE — repairing near the root costs ~depth × one aggregate query, milliseconds even on huge trees. It trusts the
  already-stored child rows: the result is self-consistent with current state, and any error below heals the next
  time anything below repairs or recomputes. Monotone convergence toward truth instead of preserved corruption.
- **Idempotent and order-independent.** Two callers produce the same rows; a duplicate call is a cheap no-op after
  the short-circuit. That's what makes it safe to fire from every escalation site without coordination.
- **It mirrors the two existing per-field up-walkers** (`propagate_recursive_has_symlinks`,
  `propagate_min_subtree_epoch`): same walk shape, same short-circuit contract. Reuse their per-level recompute
  pattern where that simplifies; unifying the three walkers is NOT a goal — do it only if it falls out naturally
  in M1.
- **Writer-thread only.** It runs inside the writer's message handling like every other mutation, so ordering against
  other writes is free. Don't add a `WriteMessage::RepairDirStats` until a real off-thread caller exists (none of
  M1–M5 needs one).
- **Volume-root boundary:** repairing from a subtree root that has no parent (or whose parent is the `ROOT_ID`
  sentinel boundary) must no-op gracefully — same convention the existing walkers use. M2's tests own this case.
- **Missing-child-row semantics (spelled out so the executor doesn't guess):** when a child dir has NO `dir_stats`
  row, repair's per-level recompute treats it as contributing **0 to sizes and counts** (LEFT JOIN + COALESCE 0),
  **absorbing `min_subtree_epoch` to 0** (matching `store::recompute_min_subtree_epoch`'s COALESCE and
  `compute_bottom_up`'s else-branch), and **false for symlinks** (matching `recompute_recursive_has_symlinks`'s
  inner JOIN). This diverges from `ComputeAllAggregates` (which computes the child first) by design: the resulting
  under-count is the accepted monotone-convergence state — coverage honestly reads incomplete (epoch 0), and the
  backfill fix (Leak C) heals the row and repairs upward. M1's missing-row test pins these exact values.

### Fix per leak

- **Leak A** (`handle_compute_subtree_aggregates`): after the scoped recompute writes the subtree's rows (including
  the subtree root's fresh totals), call `repair_dir_stats_upward(parent_of_root)`. No before/after bookkeeping —
  the repair walk reads the fresh root row via the children-SUM naturally, and running inside the same writer message
  makes it race-free by construction. This subsumes and replaces:
  - the symlink-only ancestor walk currently at the end of that handler, and
  - **both off-writer compensation blocks** (`verifier.rs` and `event_loop.rs::run_background_verification`
    manual `PropagateDeltaById` loops) — DELETE them. Only the `verifier.rs` block also sends
    `PropagateMinSubtreeEpoch` (the event_loop block never did — an existing coverage gap the repair now fixes);
    keep that one send ONLY if the repair's per-level epoch recompute doesn't already cover the chain (it should;
    verify in M2). Leaving the manual credits in place would double-count every verified new dir post-fix.
  The mark-before-aggregate ordering invariant is untouched (repair runs strictly after the scoped recompute, in the
  same message).
- **Leak B** (both drop sites): instead of dropping, escalate: resolve the deepest ancestor of the event path that IS
  in the index **as a directory** (walk path components down from the volume root via `space.resolve_abs` —
  index-only, cheap; a component that resolves to a FILE row counts as missing, else the escalation would parent new
  rows under a file id — the type-change orphan class), and queue a rescan of the highest MISSING dir (the child of
  that deepest existing dir), so `reconcile_subtree` anchors at an existing parent and discovers the whole missing
  chain (it already recurses into new dirs, credits via per-entry propagation, and stamps via `MarkDirsListed`).
  Harden `reconcile_subtree`'s root-not-in-DB fallback the same way: verify the resolved parent's `is_directory`
  before upserting under it.
  - Plumbing: `handle_creation_or_modification` is a free function with no queue access — signal the escalation back
    to `process_live_event` via the return value (a small enum or an out-param alongside `affected`), which owns the
    reconciler state and can call `queue_must_scan_sub_dirs`.
  - `reconcile_subtree`'s own skip branch escalates via its caller using the SAME deepest-existing-dir-ancestor
    resolve (re-queueing just the parent would cost one full rescan cycle per missing level on a deep chain).
  - **Live mode only.** During replay, `MustScanSubDirs` handling already defers rescans into the capped list whose
    overflow escalates to a full rescan (`event_loop.rs` ~914–931, `TooManySubdirRescans`); escalation events during
    replay defer identically (they're the same "go look here" signal), so a churny replay degrades to the existing
    full-rescan backstop instead of a new storm path.
  - **Storm control (load-bearing):** cargo-style bursts can produce thousands of these. The `pending_rescans` set
    already dedups identical paths and runs 1-concurrent; add ancestor-collapse when picking the next rescan
    (`start_next_rescan`): drop any queued path that is a descendant of another queued path (the ancestor's reconcile
    covers it), releasing the dropped path's pending hold (see below). This bounds a storm to one subtree walk.
  - Keep the skip-aggregator log lines as the signal, reworded to "escalated" counts.
- **Leak C** (`backfill_missing_dir_stats`): after writing the missing rows, compute the set of "missing roots"
  (missing dirs whose parent is NOT missing) and `repair_dir_stats_upward(parent)` for each. Dedup shared ancestor
  chains only as a perf nicety (repair is idempotent; correctness never depends on the dedup).
- **Leak D** (map pollution): make full-aggregate senders declare their source — a `source: Maps|Sql` parameter on
  `ComputeAllAggregates`, mirroring `ComputePartialAggregates`' existing `PartialAggSource`. The `Maps` senders are
  exactly the fresh full-scan completions (`scanner/mod.rs` scan_volume completion, and `volume_scanner`'s
  `finish_partial_scan` — the fresh trait scan, which truncates first so its maps are complete even for a kept
  partial); `finish_reconcile` and the one-shot heal send `Sql` — a reconcile's maps are empty in the happy case
  anyway (it writes via `UpsertEntryV2`), so this changes nothing there EXCEPT closing the pollution window
  (verification's subtree batches can interleave with a reconcile's finish; see root cause 4).
  - **`Maps` with empty maps falls back to SQL** (today's heuristic, kept): an explicitly-Maps sender whose maps got
    consumed must not treat "empty" as "everything is zero".
  - **No accumulator clear in `handle_compute_subtree_aggregates`.** A clear there sounds like defense-in-depth but
    opens its own corruption window: a `force_scan` over a never-completed partial takes the truncate + fresh-walker
    (Maps) path, and an uncancelled in-flight verification's `ComputeSubtreeAggregates` landing mid-scan would wipe
    maps that then partially repopulate — the final Maps aggregate would roll the tree up from incomplete direct
    stats. `TruncateData` already clears the maps at the start of every legitimate Maps flow; with sender-declared
    sources, the clear buys nothing and risks that window. Leave the accumulator alone.
  - Test/bench senders pick sources deliberately when the message changes: `reconcile_bench.rs` baselines the MAPS
    path (passing `Sql` there would silently skew the benchmark), and the back-to-back double-send stress test pins
    the empty-Maps fallback semantics above.
- **The sink** (`propagate_delta_by_id`): remove all EIGHT `.max(0)` clamps.
  - `Some` branch: when any field would go negative, switch the remainder of the walk to
    `repair_dir_stats_upward(current_id)` and `log::warn!` once with the volume, dir id, and the offending delta —
    our drift telemetry. It should become rare; a steadily-firing warn means a new leak to find.
  - `None` branch: a missing row with any NEGATIVE delta component is Leak-C territory — escalate to repair from
    that dir instead of materializing a zeroed row. Pure-positive deltas keep creating the row (correct and
    load-bearing for live-created dirs, epoch 0 as today).
- **Invisibility** (M4b, below): mark + hold `pending_sizes` for a queued `MustScanSubDirs` scope, and emit
  `index-dir-updated` for the rescan root + its ancestor chain when the reconcile completes.

### UI visibility for coalesced rescans (the hourglass)

`PendingSizes` today: mark (path + ancestors) from the event loop's `pending_paths`, clear wholesale when the writer
queue drains. Two gaps for a long-running detached reconcile: nothing marks its scope at queue time, and the
queue-drain clear wipes any mark long before the reconcile finishes (the writer queue oscillates empty during a
multi-second walk).

Design: give `PendingSizes` a second, *held roots* tier.

- `hold(root)` / `release(root)` maintain a small set of **rescan root paths only** (no ancestor expansion).
  `is_pending(path)` = transient-set membership OR `path` is related to any held root in EITHER direction
  (component-aware prefix test both ways): an ancestor-or-equal of a held root (its aggregate includes the subtree
  being rewritten) OR a descendant of one (its own rows are being rewritten). The held set is bounded by
  `pending_rescans` (a handful), so the linear scan per query is trivial.
  - Why roots + query-time prefix test instead of expanding ancestors into a held set: overlapping rescans
    (`/a/b` and `/a/c`) would share ancestor entries, and releasing one would either strip `/a` while the other is
    in flight or leak it forever. Holding only roots keeps release exact, needs no refcounting, and preserves the
    module's "no per-entry bookkeeping to leak" property.
- `queue_must_scan_sub_dirs` calls `hold(path)` when inserting into `pending_rescans`. Releases happen at EVERY exit
  — enumerated so none strands a hold: the `reconcile_subtree` success arm, its failure arm, the
  connection-open-failure early return in `start_next_rescan` (which recurses to the next rescan — release before
  recursing), and ancestor-collapse dropping a queued descendant. A release SKIPS when the same root is back in
  `pending_rescans` (re-queued mid-rescan) — otherwise the second rescan would run unheld.
- Volume-id plumbing: `hold`/`release` must target the volume's own tracker (`get_pending_sizes_for(volume_id)`),
  but `EventReconciler` and the rescan task carry only `space`, not the volume id — thread the id in (the event loop
  has it and already routes marks per-volume). Defaulting to the root-only `get_pending_sizes()` would recreate for
  holds the exact cross-volume bug this milestone fixes for clears.
- `clear()` (writer drain) clears ONLY the transient set. While in here, fix the pre-existing cross-volume routing
  bug: the writer-drain clear calls root-only `get_pending_sizes()` from EVERY volume's writer loop
  (`writer/mod.rs` ~873–877) while marks route per-volume (`get_pending_sizes_for`) — so a non-root writer drain
  wipes root's hourglass early, and non-root trackers never clear (stuck hourglass). Route the clear through
  `get_pending_sizes_for(volume_id)`, and target `hold`/`release` at the volume's own tracker.
- On reconcile completion, sequence: `release(root)` FIRST, then emit — otherwise the triggered refetch re-reads
  `pending = true`. Emit `root` plus its ancestor chain (`collect_ancestor_paths`, the existing helper — a pane on a
  grandparent needs the refresh too), and do it via the existing `WriteMessage::EmitDirUpdated` so it sequences after
  the rescan's writes land (the completion arm runs in `spawn_blocking` with no `AppHandle`; the writer message
  already solves both problems).

**No frontend changes.** The FE already renders `recursiveSizePending` (hourglass over a still-visible stale size),
and the epoch system already distinguishes empty/unknown/lower-bound. The incident's "confident 0 bytes" was the
backend lying in the data, not the FE rendering it wrong. (Deliberate non-fix: a value-shape write guard like
"dirs > 0 but 0 bytes ⇒ reject" is WRONG — a tree of empty dirs legitimately looks like that. Drift detection stays
arithmetic-evidence-based: the negative-delta trigger.)

### Removal-storm coalescing (root cause 7 — the 2–5 minute chew)

Intent: when the kernel doesn't coalesce a bulk delete for us, synthesize the coalescing ourselves and route it
through the SAME machinery the coalesced case uses — one subtree reconcile with the hourglass held — instead of
chewing hundreds of thousands of per-file removals. The design deliberately adds no new pipeline: it reuses
`queue_must_scan_sub_dirs`, the held-roots pending tier, the completion emit, and the M4a stress invariant.

Two composable pieces, in the live event loop's per-batch processing:

1. **Storm detection → escalate to a subtree reconcile.** The hook is `process_live_batch` — live mode genuinely
   forms a 1 s batch (`pending_events` drained on `LIVE_FLUSH_INTERVAL_MS`); the detector lives INSIDE that function
   so BOTH live loops get it (`run_live_event_loop` AND the post-replay loop inside `run_replay_event_loop`). Per
   batch, count removal events grouped by a shallow common prefix (component-truncated, depth cap ~6–8) — but the
   capped prefix is ONLY the grouping key: the queued rescan anchors at the group's **deepest common ancestor**
   (the incident path is ~11 components deep; anchoring at the cap would re-list a whole worktree, node_modules and
   all, instead of `target` — the exact over-scope the cap was supposed to prevent). When a group exceeds
   `REMOVAL_STORM_THRESHOLD` (initial ~200 — well above organic per-batch delete rates, far below storm scale; tune
   by measurement), STOP per-file processing for that scope: `queue_must_scan_sub_dirs(anchor)` (hourglass via the
   held tier, dedup + ancestor-collapse + 1-concurrent all inherited), and from then on removal events under a
   queued-or-active rescan prefix are dropped. Three load-bearing rules on the drop:
   - **The reconciler must be able to SEE the active rescan path.** Today `start_next_rescan` pops the path from
     `pending_rescans` before spawning and `rescan_active` is a bare bool — the active path lives only in the task's
     local. Retain it in a shared slot (`Arc<Mutex<Option<PathBuf>>>` set at spawn, cleared on completion) or the
     drop rule reads an empty set and drops nothing.
   - **Drop STRICT descendants only — never the scope's own removal event.** The deleted root's own `rmdir` arrives
     LAST; it must take the normal per-file path (stat fails → `DeleteSubtreeById` — the cheap mechanism), because
     `reconcile_subtree` on a root that's in the DB but gone from disk currently deletes nothing (resolves the id,
     fails the listing, returns 0/0/0) and would strand the whole subtree.
   - **Every dropped event re-queues the anchor into `pending_rescans`** (set-dedup makes it idempotent and ~free).
     The existing re-queue rule fires only on `must_scan_sub_dirs` events; without this, a sub-threshold tail batch
     dropped after the walk already listed those dirs leaves stale rows with no follow-up — and a created-then-
     removed merge (which `merge_fs_events` collapses to `item_removed`) could otherwise eat a real change with no
     recovery. This is also what makes M4a's fixed-point termination argument hold for storms.
   A reconcile running while the `rm` is still in flight is safe: it diffs current disk state, and later events
   re-queue the anchor. Why route through the rescan queue instead of a bespoke "big delete" path: it is EXACTLY the
   kernel's `MustScanSubDirs` semantic ("too much changed here — go look"), produced in user space. One code path,
   one set of invariants, one hourglass story, and the M4b visibility work covers it for free.
   Creations/modifications under the scope keep flowing per-event (the drop rule keys on `item_removed`; a mixed
   create+delete storm still converges — the reconcile sees final disk state).
2. **Parent-first ordering within a batch** (cheap complement, below the threshold): sort each batch's removal
   events dirs-before-files, shallower-paths-first, before per-event processing. `rm -rf` emits a dir's `rmdir`
   AFTER its children's unlinks but usually in the SAME 1 s batch (small dirs empty fast), so processing the dir
   first turns its children's events into cheap unknown-path skips — the mechanism the incident log shows working,
   engaged early instead of accidentally. This is a ~3–5× saver on its own (each skip still pays a stat + resolve),
   which is why the storm escalation above stays the headline fix. `item_is_dir` comes straight from FSEvents flags
   (`parse_fsevent` maps `StreamFlags::IS_DIR`, no stat — valid for deleted paths, and `merge_fs_events` ORs it
   through dedup) so the sort input is solid on macOS; on Linux the flag defaults false for removals (the notify
   translation stats the gone path), so the sort degrades to a harmless no-op there — optionally map notify's
   `RemoveKind::Folder/File` to fix that in passing.

Expected effect on the incident scenario: storm detected in the first 1–2 batches → hourglass on `e2e-budget` and
ancestors within ~2 s → `rm` runs at its own pace → one `reconcile_subtree` over the survivors (~5–15 s for 72k
files) + aggregate (~3 s) + in-place emit. **Index latency ≈ 15–30 s after the `rm` finishes, versus 2–5 minutes of
per-event churn — and ~20× less CPU/IO** (one re-list versus ~500k stat+resolve+delete+walk cycles), per the
"respect the user's resources" principle.

Perf caveat to respect: the escalation trades per-file precision for a re-list of the SURVIVING siblings under the
prefix. Choosing the prefix too shallow (e.g. `/Users/x` because deletes were scattered) would re-list a huge tree —
that's what the depth cap and the per-group threshold guard. The threshold/depth constants live next to the
detector with a comment tying them to this section.

### Healing existing installs

Every existing DB (David's dev instances, beta users) carries accumulated drift that the fixes prevent going forward
but don't retro-correct (repair only fires on touch). One-shot heal, per volume DB, keyed on meta
`aggregates_rebuilt_for_ledger`.

Mechanism (writer-side, failure-safe): the key is written **from inside the writer's `ComputeAllAggregates` handler,
on `Ok` only**, via a **writer-side latch** — armed once at startup when the key is absent, consumed by the first
SUCCESSFUL full aggregate on that writer, whichever flow sends it. This gives every property in one place: a quit OR
a failed aggregate leaves the key unset (the handler currently swallows `Err` with a warn and moves on — an
externally-queued `UpdateMeta` would set the key even after a failed recompute, stranding the drift forever);
"ride whichever aggregate runs" needs no flag threaded through the five shared send-site signatures
(`scan_volume` / `finish_reconcile` / `finish_partial_scan` and their callers); and flows that bypass
`resume_or_scan` entirely (`force_rescan`) are covered for free — their aggregate fires the latch too, correctly,
since it did heal the drift.

Placement of the DECISION (arm latch + maybe enqueue): `resume_or_scan` / `resume_or_scan_network` — the one place
where replay-vs-full-scan is decided per launch:

- Branches that end in `start_scan` / `start_volume_scan` (stale-launch, journal-gap, `IncompletePreviousScan`):
  arm the latch only — that flow's own final aggregate consumes it; no second whole-tree aggregate (a redundant
  multi-second writer stall on a large DB, possibly against a mid-rebuild table).
- The journal-REPLAY branch (the normal boot-disk launch — replay runs NO full aggregate, only backfill) and the
  SMB/MTP completed-index branch (which does NOT rescan on connect; it loads Stale and waits for a user rescan):
  arm the latch AND enqueue the heal's own `ComputeAllAggregates { source: Sql }` after the event loop starts.
  These are the COMMON arms — the heal's own aggregate is the normal case, not the exception.

Why not a DB rebuild: that rescans the disk for no reason — `entries` is fine, only `dir_stats` drifted, and the SQL
aggregate recomputes exactly that from `entries` (O(dirs) bulk SQL, measured faster than a truncate rebuild; DETAILS
§ "Non-destructive rescan"). Why a meta key and not a schema bump: a schema mismatch deletes the DB
(disposable-cache rule) — needlessly destructive here.

## What this deliberately does NOT do

- No change to `BulkReconcileGuard` / full-reconcile suppression, the single-aggregate finish, or the
  mark-before-aggregate ordering (all perf/correctness load-bearing, see DETAILS).
- No per-entry ancestor propagation added to any bulk path (the historical hours-long wedge).
- No new "dirty" column or schema change: `pending_sizes` (transient) + the epoch/coverage system (persistent) +
  repair-on-touch cover the states we need. If a future case needs persistent dirtiness, `min_subtree_epoch = 0`
  is the existing vocabulary for "this subtree needs recount" — extend that, don't invent a parallel flag.
- No FE changes.
- No fix for the cancelled-full-reconcile drift window (documented above; the existing next-rescan flow heals it).
- No attempt to make SMB/MTP live paths escalate like Leak B (their watch paths differ; SMB overflow already routes
  to `FullRefresh` → `reconcile_subtree`, which after M1+M2 credits correctly). The writer-side fixes (M1–M3) are
  volume-agnostic by construction — same writer code serves every volume.

## Milestones

Run `pnpm check -q --fast` while iterating, `pnpm check -q` at each milestone end, `pnpm check -q --include-slow`
before wrap-up. All Rust work: `pnpm check clippy rust-tests` scoped runs are fine mid-milestone.

### M1 — the repair primitive + un-clamp (the core bug fix)

TDD, strict red→green (this is the bug-fix heart of the effort):

1. RED: a writer-level test reproducing the incident fingerprint: build a tree, artificially drift an ancestor's
   `dir_stats` low (direct SQL — simulating pre-fix leaked credits), `DeleteSubtreeById` a large child, assert the
   ancestor ends EXACT (equal to a recompute-from-entries oracle — reuse
   `stress_test_helpers::check_db_consistency`'s recompute). Watch it fail: today it floors to 0.
2. RED: unit tests for `repair_dir_stats_upward` itself: repairs a wrong middle row; short-circuits (stops walking
   when a level is already correct — assert via a poisoned-above sentinel that stays poisoned); handles a missing
   `dir_stats` row mid-chain; recomputes has_symlinks + min_subtree_epoch consistently with the existing walkers'
   semantics.
3. RED: the `None`-branch trap — a negative delta addressed to a dir with no `dir_stats` row must NOT materialize a
   zeroed row (assert repair output instead). Twin test pinning the KEPT behavior: a pure-POSITIVE delta to a missing
   row still creates it (load-bearing for live-created dirs).
4. GREEN: implement `repair_dir_stats_upward`; replace all eight `.max(0)` clamps with negative-detection → repair
   escalation + single `warn!` (`Some` branch) and the negative-component escalation (`None` branch).
5. One repair test on a mount-rooted (non-`/`) volume DB — pins the "volume-agnostic by construction" claim cheaply.
6. Regression anchor comment on the fingerprint test: `// Pre-fix this stored 0/0/1492 for a 1.21 GB subtree.`
7. Register this plan in `docs/specs/index.md` in the first commit that includes it (`docs-reachable` is
   error-level; don't defer to M5).

Checks: `pnpm check -q` (expect `file-length` warns if `delta.rs` grows — split rather than allowlist-bump if it
trips).

### M2 — Leak A: subtree scans repair ancestors on the writer

1. RED (message-level): seed a tree with correct aggregates; replay the real sequence `DeleteDescendantsById` →
   `InsertEntriesV2` → `ComputeSubtreeAggregates`; assert ancestors reflect the change, in BOTH directions (grow and
   shrink). Fails today (ancestors unchanged by the messages alone). Include the volume-root-parent boundary case
   (repair from a parentless root no-ops).
2. RED (flow-level — this is the one that catches double-counting): a test through the verification flow
   (`verify_and_correct` / `run_background_verification`-shaped, whichever harness exists or is cheapest to add) with
   a new dir appearing on disk: assert ancestors end EXACT, not doubled. With the writer-side repair added but the
   manual compensation still in place, this fails with 2× credit — proving the deletion below is load-bearing. If a
   harness for the pre-fix concurrent-live double-count (a live upsert landing inside the subtree between the diff
   and `DeleteDescendantsById`) is cheap, pin that too; if not, the 2×-credit assertion covers the class.
3. GREEN: `repair_dir_stats_upward(parent)` at the end of `handle_compute_subtree_aggregates`; DELETE the manual
   `PropagateDeltaById` compensation blocks in `verifier.rs` and `event_loop.rs::run_background_verification`.
   Note the asymmetry: only `verifier.rs` also sends `PropagateMinSubtreeEpoch` — the event_loop block never did,
   meaning verification-discovered subtrees on that path never recomputed ancestor COVERAGE at all (an existing gap
   the repair's per-level epoch recompute now fixes — assert it in the flow test). Keep the verifier's epoch send
   only if the repair doesn't subsume it (it should). Drop the handler's now-subsumed symlink-only ancestor walk;
   make the `scan_subtree` cancel arm still send `ComputeSubtreeAggregates` (destructive-cancel insurance).
4. Optional hardening while touching the message: carry the subtree root's entry ID on `ComputeSubtreeAggregates`
   instead of re-resolving the PATH at process time — a rename/delete landing between send and process currently
   no-ops the aggregate (and would no-op the repair) after the destructive messages already ran. Requires extending
   `run_scan`'s return tuple to expose `root_id` to the send site in `scan_subtree` (it resolves the id internally
   today and doesn't return it) — do NOT re-resolve the path at the send site instead; that reintroduces the same
   race. Cheap now, fiddly later.

### M3 — Leak C + Leak D: backfill repairs ancestors; maps can't poison full aggregates

1. RED (Leak C): entries exist for a dir chain with no `dir_stats` rows and stale ancestors above; run backfill;
   assert the full chain AND ancestors end exact. Fails today.
2. RED (Leak D): `InsertEntriesV2` a subtree batch (maps now polluted), then a `finish_reconcile`-shaped
   `ComputeAllAggregates`; assert dirs OUTSIDE the subtree keep correct stats. Fails today (maps path zeroes them).
   Include the interleaved variant: the full aggregate landing BETWEEN the subtree batches and their
   `ComputeSubtreeAggregates` — this is why the source parameter, not the clear, is the real fix.
3. GREEN: missing-roots computation + repair calls in `backfill_missing_dir_stats`; `source: Maps|Sql` parameter on
   `ComputeAllAggregates` — `Maps` ONLY from the fresh full-scan completions (`scanner/mod.rs` + `volume_scanner`'s
   `finish_partial_scan`), `Sql` from `finish_reconcile` and the M5 heal; empty-Maps falls back to SQL; NO
   accumulator clear in the subtree handler (see the Leak D design bullets for why); bench/stress senders pick
   sources deliberately (`reconcile_bench.rs` baselines Maps).
4. Fix the false comment in `local_reconcile.rs`'s cancel arm (claims partial writes "stay size-consistent" — they
   don't under `SetDeltaPropagation(false)`); document the accepted drift window there in one line.

M2 and M3 both touch `handle_compute_subtree_aggregates`, so run them sequentially (no parallel agents).

### M4a — live-path escalation (Leak B) + removal-storm coalescing + the ledger stress invariant

Tests are integration-style (reconciler/event_loop test files); TDD where the assertion is cheap to write first:

1. Missing-parent escalation: RED test — feed a creation event whose parent chain is absent; assert a rescan gets
   queued for the highest missing dir (not dropped). GREEN: deepest-existing-DIR-ancestor resolve (via
   `space.resolve_abs`) + return-signal from `handle_creation_or_modification` to `process_live_event` +
   `queue_must_scan_sub_dirs`. Same treatment for `reconcile_subtree`'s skip branch. Replay-mode events defer into
   the existing deferred-rescan machinery (assert no live queueing during replay). Rework the skip-aggregator wording.
1b. Removal-storm coalescing (design § "Removal-storm coalescing"). Test-observability first: the pre- and post-fix
   DB end states are IDENTICAL for these behaviors (rows end up gone either way), so the assertions need message- or
   counter-level probes — add writer delete/subtree-delete counters to `DEBUG_STATS` (or a test-only writer message
   probe) and SEED the DB with the storm's rows (unseeded, per-file removals already no-op as unknown-path skips and
   the test is green pre-fix for the wrong reason). The deterministic RED: pre-set `rescan_active = true` (the
   existing `must_scan_sub_dirs_deduplication` trick) so the queued anchor stays visible in `pending_rescans`, feed
   `REMOVAL_STORM_THRESHOLD`+1 removals under one prefix, assert ONE rescan queued at the deepest common ancestor
   and zero per-file delete messages for the storm's events. Twin below-threshold test: removals process per-file,
   no rescan queued. The batch-sort test needs the counters too (and a seeded dir + children in ONE batch): assert
   one `DeleteSubtreeById` + zero `DeleteEntryById`, not N — and note the pre-fix run is order-flaky
   (`process_live_batch` drains a `HashMap`), which the counter assertion tolerates by asserting the POST-fix
   deterministic shape, with the RED run accepted as failing-or-flaky. Also assert the scope's OWN removal event is
   NOT dropped (strict-descendants rule), and that a dropped event re-inserts the anchor into `pending_rescans`.
   GREEN: detector in `process_live_batch` + active-path slot + strict-descendant drop with re-queue + the
   parent-first batch sort. Storm-mode end state is covered by the stress oracle (3.): add a bulk-delete storm to
   the mix and assert the invariant holds after quiescence.
2. Ancestor-collapse in `start_next_rescan`: unit test — queue `/a/b/c` and `/a/b`; assert one reconcile at `/a/b`
   covers both. While in this function, fix the pre-existing connection-kind bug: it opens a WRITE connection for the
   reconcile's reads (`open_write_connection`, `reconciler.rs` ~442, with a log line that even says "read
   connection") — the CLAUDE.md invariant is reconciler-holds-READ; switch it and fix the log wording.
3. Stress: extend one existing stress test (`stress_tests_concurrency.rs` family) with a mixed storm — live events +
   a concurrent subtree rescan + deletes — ending in a `check_db_consistency` oracle pass. Quiescence needs a
   fixed-point loop, not a single await (a rescan finishing after a writer-drain check enqueues more messages, and
   `start_next_rescan` chains): stop event generation → drain the event loop → await `rescan_active == false` AND
   `pending_rescans` empty → final writer drain → re-check the rescan set is still empty; loop until stable.
   Termination argument (why the loop converges): with no new events, escalations re-fire only from
   `reconcile_subtree`'s skip branch, each anchoring strictly closer to the volume root (bounded by depth);
   successful rescans strictly shrink the missing set; happy-path rescans queue nothing. This is the whole-effort
   invariant: **after the writer drains and rescans quiesce, `dir_stats` ≡ recompute-from-`entries`** (carve-out:
   the documented cancelled-full-reconcile window).

### M4b — hourglass visibility (held tier + emit)

1. Held-roots tier: unit tests in `pending_sizes.rs` — hold/release/clear interplay (writer-drain clear keeps holds;
   release on failure path; overlapping rescans `/a/b` + `/a/c` release independently without stripping `/a`'s
   pendingness while one is in flight; `is_pending` matches BOTH ancestors and descendants of a held root; release
   skips when the root is re-queued). Wire `queue_must_scan_sub_dirs` + every release site enumerated in the design
   (including the connection-open-failure arm and ancestor-collapse drops), and route the writer-drain clear
   per-volume (`get_pending_sizes_for(volume_id)`) — add a test pinning that a non-root writer drain no longer
   clears root's tracker.
2. Completion emit: assert `EmitDirUpdated` (root + ancestor chain) rides the writer after the rescan's writes, and
   that `release` precedes it.

### M5 — one-shot heal, docs, wrap-up

1. The heal per the design section: key written from inside the aggregate handler on `Ok` only
   (`set_heal_key_on_success`), heal decision in `resume_or_scan` / `resume_or_scan_network` (scan branches arm
   their own flow's aggregate; replay + SMB-completed branches enqueue the heal's own Sql aggregate). Tests:
   drifted stats + absent key ⇒ aggregate runs, key set after; a FAILED aggregate leaves the key unset (re-heals
   next launch); second clean startup does nothing; a launch that takes the rescan funnel gets NO second aggregate
   but still sets the key on that flow's completion; the SMB completed-index arm (no rescan on connect) heals via
   its own aggregate.
2. Docs (per `.claude/rules/docs.md`, single-sourced in the indexing docs):
   - `indexing/DETAILS.md`: new section "The dir_stats ledger" — the three principles, the repair primitive's
     contract (when delta vs repair, missing-child semantics), the escalation sites, the negative-delta warn as
     drift telemetry, the `source: Maps|Sql` sender contract (and why the subtree handler must NOT clear the
     accumulator), the held-roots pending tier, removal-storm coalescing (threshold/depth-cap rationale), the
     accepted cancel-drift window, and the healing latch. Update the now-stale "after a reconcile the maps are
     empty" claim.
   - `indexing/CLAUDE.md` must-knows (concise): "Never clamp `dir_stats` arithmetic — a negative delta is drift
     evidence; escalate to `repair_dir_stats_upward` (DETAILS § ledger)." Plus one line each: structural rewrites
     repair ancestors on the writer (never off-writer read-then-credit), and full-aggregate senders declare
     `source: Maps|Sql` (never clear the accumulator in the subtree handler). Mind the 600-word ceiling — trim
     elsewhere if needed, don't bump the allowlist.
   - `pending_sizes.rs` module doc: the held-roots tier and per-volume clear routing.
   - `docs/specs/index.md`: already registered in M1; update the entry's status line.
3. `CHANGELOG.md` entry (impact-first: folder sizes no longer drift or stick at a wrong 0 after large deletes;
   hourglass now shows during coalesced rescans).
4. `pnpm check -q --include-slow`, fix fallout, FF-merge readiness (rebase onto current local `main` first — it
   advances often).

## Verification beyond tests

Manual QA on the dev instance (David's data is the perfect fixture): after M5, relaunch → the one-shot heal should fix
`e2e-budget` (1.21 GB) and `worktrees` (consistent with children) without a disk rescan; then repeat the incident
(`rm -rf` a large built worktree `target`) and watch: hourglass appears on ancestors within ~2 s, sizes update in
place without navigation, the settled numbers match `du`-style truth, and the whole thing lands ~15–30 s after the
`rm` finishes (not minutes). Check the log for: the storm detector firing (one rescan queued, not thousands of
per-file deletes), and the negative-delta warn staying silent during the exercise.

## Risks and rabbit holes

- **Repair perf on hot huge dirs**: a level with ~100k direct children makes the per-level SUM non-trivial. It only
  runs on escalation, and the short-circuit bounds repeat costs, but if the negative-delta warn ever fires in a loop
  (a persistent below-zero source), repair could churn. The warn is the tripwire; if it shows up in normal use, stop
  and find the leak rather than tuning repair.
- **Escalation storms (Leak B)**: ancestor-collapse + 1-concurrent rescans + set-dedup should bound it; replay-mode
  deferral reuses the existing capped list + full-rescan backstop. If cargo churn still queues excessive rescans, add
  a small debounce before `start_next_rescan` picks work — but measure first; `reconcile_subtree` on build-output
  dirs is fast and the 60 s file throttle already suppresses most rewrite noise.
- **Double-application during concurrent repair + delta**: both run on the single writer thread, serialized — no true
  concurrency. A delta queued behind a repair applies to the repaired row, which is correct by construction: the
  delta describes an entries-change whose row landed before it.
- **Deleting the manual compensation blocks (M2) changes verification behavior**: the flow-level test is the guard.
  If some caller depends on the compensation's `PropagateMinSubtreeEpoch` for coverage (not sizes), keep exactly that
  send — the repair's epoch recompute should subsume it, but verify rather than assume.
- **Don't grow `reconciler.rs`** (1,337 lines, allowlisted): new escalation logic that doesn't fit a small diff goes
  in a sibling module (e.g. `reconciler/escalation.rs`), matching the existing `reconciler/throttle.rs` pattern.
