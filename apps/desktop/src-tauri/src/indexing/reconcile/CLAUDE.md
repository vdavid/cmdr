# Reconcile (keep the index matching disk)

Three mechanisms resync the index after the initial scan: the event-triggered `reconciler`, the full `local_reconcile`
(rescan-in-place), and the per-navigation `verifier`. Rationale, evidence, and the full mechanics are in
[DETAILS.md](DETAILS.md); the bullets below are only the guardrails.

## Module map

- **reconciler.rs** + **reconciler/**: the event path — `diff_dir_against_db`, `reconcile_subtree`, `BulkReconcileGuard`,
  and `rescan*` / `throttle` / `escalation` (depth-split, sweep, per-file throttle).
- **local_reconcile.rs** + **local_reconcile/**: serial full-tree rescan-in-place (`cost_budget`, `latency_probe`).
- **verifier.rs**: per-navigation `read_dir` diff. **reconcile_bench** / **reconcile_correctness**: perf + regressions.

## Must-knows

- **A rescan of a populated+completed index RECONCILES in place, never truncates.** LOCAL: `entry_count > 1 &&
  prior_scan_completed`; NETWORK: `entry_count > 1`. Keep the two predicates in lock-step.
- **Recursion is decoupled from the write decision:** recurse into EVERY matched child dir, gate only writes. Gating
  recursion on `changed` "completed" an unscanned share.
- **New child dirs resolve by `(parent_id, name)`, never absolute path** (an absolute walk from `ROOT_ID` false-completes
  a network index).
- **A root listing ZERO children does NOT mark complete** (typed `EmptyRoot`); bail before diffing, else the diff blanks
  the index and the false "complete" strands it.
- **Suppress full-reconcile propagation ONLY in `BulkReconcileGuard`** (`MarkLedgerUnpaid`/`PayLedgerIfUnpaid` on Drop;
  finish stamps marks + ONE `ComputeAllAggregates{source: Sql}`). ❌ No per-dir/per-entry propagation on the bulk path;
  the LIVE path keeps propagating.
- **`local_reconcile` stays SERIAL** (hang-tolerance via `GuardedReader`, 15 s cap). Hardlink dedup: dedup the summary
  total only, leave the per-entry snapshot RAW (the writer dedups).
- **Cost budget scores read latency as a FRACTION of slow reads, never a total.** A skipped dir is one we NEVER listed:
  ❌ never diff it with an empty listing, ❌ never stamp its `listed_epoch` (`0` absorbs up to `~`/`/`).
- **Verification's two teeth** (`verify_affected_dirs`, code in `../watch/`): a `count_children_capped` probe before the
  snapshot + a `read_dir` iteration cap. ❌ A declined dir keeps claiming exact (owned debt), never `listed_epoch = 0`.
- **Per-subtree rescan throttle:** ≤ 1 reconcile per `RESCAN_THROTTLE_WINDOW` (60 s), leading + trailing, on a
  `Utility`-QoS thread.
- **Depth-split `MustScanSubDirs`:** SHALLOW (`depth ≤ 2`) → visible scanner, NO hourglass hold, never `pending_rescans`;
  DEEP (`≥ 3`) → throttled drain.
- **A shallow anchor sweeps at most ONCE A DAY, boot disk only** (24 h; mount-rooted keeps 45 s). Coalesced anchors are
  counted; the badge stays GREEN by design; the window is wall-clock, persisted, seeded from
  `max(shallow_sweep_at, scan_completed_at)`.

Full depth: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
