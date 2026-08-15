# Reconcile (keep the index matching disk)

Three mechanisms resync the index after the initial scan: the event-triggered `reconciler`, the full `local_reconcile`
(rescan-in-place), and the per-navigation `verifier`. Guardrails only below; `DETAILS.md` holds the measurements,
incidents, and constants behind every one.

`reconciler.rs` + `reconciler/` the event path (`diff_dir_against_db`, `reconcile_subtree`, `BulkReconcileGuard`, and
the `rescan*` family: route, throttle, settle, hold, churn); `local_reconcile.rs` + `local_reconcile/` the serial
full-tree rescan-in-place; `verifier.rs` the per-navigation `read_dir` diff.

## Must-knows

- **A rescan of a populated+completed index RECONCILES in place, ❌ never truncates.** LOCAL:
  `entry_count > 1 && prior_scan_completed`; NETWORK: `entry_count > 1`. Keep the two predicates in lock-step.
- **Recursion is decoupled from the write decision**: recurse into EVERY matched child dir, gate only writes. Gating on
  `changed` "completed" an unscanned share.
- **New child dirs resolve by `(parent_id, name)`, ❌ never absolute path** (an absolute walk from `ROOT_ID`
  false-completes a network index).
- **A root listing ZERO children does NOT mark complete** (typed `EmptyRoot`): bail before diffing, else the diff blanks
  the index and the false "complete" strands it. A SHORT `getattrlistbulk` read is re-read via `read_dir`, ❌ never
  diffed.
- **Suppress full-reconcile propagation ONLY in `BulkReconcileGuard`** (`MarkLedgerUnpaid`/`PayLedgerIfUnpaid` on Drop;
  finish stamps marks + ONE `ComputeAllAggregates{source: Sql}`). ❌ No per-dir or per-entry propagation on the bulk
  path; the LIVE path keeps propagating.
- **`local_reconcile` stays SERIAL** (hang-tolerance via `GuardedReader`). Dedup hardlinks in the summary total only,
  leaving the per-entry snapshot RAW — the writer dedups.
- **Every size diff skips a deduped hardlink** (`db.logical_size.is_none() && snap.nlink > 1` → mtime only, in BOTH
  `diff_dir_against_db` and `verifier.rs`): the writer's NULL is the converged state, so comparing it re-upserts the row
  forever. ❌ Not the NULL alone; `nlink == 1` restores a real size.
- **Cost budget scores read latency as a FRACTION of slow reads, ❌ never a total.** A skipped dir is one we NEVER
  listed: ❌ never diff it with an empty listing, ❌ never stamp its `listed_epoch`.
- **The verifier BAILS on `listed_epoch == 0` while the volume has no `scan_completed_at`** (a walk owns that ground).
  ⚠️ Both halves: on a COMPLETED volume a SKIPPED dir still heals here.
- **The verifier's pool, writer, and space must name ONE volume** (all off the instance). ❌ Never read via root's
  `get_read_pool()`: the pass goes inert on SMB/MTP/external.
- **Verification's two teeth** (`verify_affected_dirs`, in `../watch/`): a `count_children_capped` probe + a `read_dir`
  iteration cap. ❌ A declined dir keeps claiming exact (owned debt), never `listed_epoch = 0`.
- **Per-subtree rescan throttle is COST-PROPORTIONAL** (window scaled off `walk_cost`, clamped), and cost is duration
  MINUS writer wait, else one saturated writer over-throttles every anchor. `gc` measures each record against its OWN
  window.
- **A brand-new anchor SETTLES before it walks** (`rescan_settle.rs`), from BIRTHTIME, reading INELIGIBLE while queued
  and holding nothing. ❌ No stat inside the pure throttle (the call site passes a deadline in), ❌ no mtime; a missing
  birthtime FAILS OPEN.
- **A rescan anchor holds the hourglass ONLY while walking or queued-AND-eligible** (`rescan_hold.rs`). ❌ No
  unconditional hold at enqueue: a resting anchor puts "size updating" on `~` and `/`. ❌ Don't drop the pick-time hold
  either; it leaves no unheld-write window.
- **Depth-split `MustScanSubDirs`**: SHALLOW (`depth ≤ 2`) → visible scanner, NO hourglass hold, never `pending_rescans`;
  DEEP (`≥ 3`) → throttled drain. A shallow anchor sweeps at most ONCE A DAY, boot disk only; coalesced anchors are
  counted, the badge stays GREEN, and the window is wall-clock and persisted.

Full depth: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
