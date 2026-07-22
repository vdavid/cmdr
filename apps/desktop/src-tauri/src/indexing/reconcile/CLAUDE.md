# Reconcile (keep the index matching disk)

Three mechanisms resync the index after the initial scan: the event-triggered `reconciler`, the full `local_reconcile`
(rescan-in-place), and the per-navigation `verifier`, all sharing the honest-stale / skip / cost-budget discipline.
Canonical elsewhere: honest sizes + ledger → [`../writer/DETAILS.md`](../writer/DETAILS.md); the guarded reader →
[`../scanner/DETAILS.md`](../scanner/DETAILS.md); the event loop + `verify_affected_dirs` code →
[`../watch/DETAILS.md`](../watch/DETAILS.md) (its teeth are documented here).

## Module map

- **reconciler.rs** + **reconciler/**: the event path — `diff_dir_against_db`, `reconcile_subtree`, `BulkReconcileGuard`,
  and `rescan*` / `throttle` / `escalation` (depth-split, sweep, per-file throttle).
- **local_reconcile.rs** + **local_reconcile/**: serial full-tree rescan-in-place (`cost_budget`, `latency_probe`).
- **verifier.rs**: per-navigation `read_dir` diff. **reconcile_bench** / **reconcile_correctness**: perf + regressions.

## Must-knows

- **A rescan of a populated+completed index RECONCILES in place, never truncates** (stays visible-stale; a mid-rescan
  disconnect keeps prior data). LOCAL reconciles when `entry_count > 1 && prior_scan_completed`, NETWORK when
  `entry_count > 1`; a never-completed partial takes the fast parallel rebuild. Keep the two predicates in lock-step.
- **Recursion is decoupled from the write decision.** The BFS recurses into EVERY matched child dir; writes stay
  change-gated. Gating recursion on `changed` "completed" an unscanned share in 0.0s.
- **A new child dir resolves by `(parent_id, name)` (`resolve_component`), NEVER by absolute path** — an absolute walk
  from `ROOT_ID` fails at the first component on a network index (root is the mount) and falsely "completes".
- **A ROOT listing ZERO children does NOT mark complete** (typed `EmptyRoot`); reconcile bails BEFORE diffing the root,
  else the diff blanks the index and the false "complete" strands it forever.
- **Full-reconcile propagation suppression lives ONLY in `BulkReconcileGuard`**: `MarkLedgerUnpaid` +
  `SetDeltaPropagation(false)` before the walk, restore + `PayLedgerIfUnpaid` on EVERY exit (`Drop`); finish stamps
  marks + runs ONE `ComputeAllAggregates { source: Sql }`. ❌ Never per-dir `PropagateMinSubtreeEpoch` on the bulk path
  (2× slower than truncate) nor per-entry propagation (hours-long writer wedge). The LIVE path keeps propagating. Bare
  `SetDeltaPropagation(false)` left 249 dirs claiming exact sizes.
- **`local_reconcile` stays SERIAL** (hang-tolerance, not parallelism): each `read_fs_children` goes through a
  `GuardedReader` (15 s cap), reuses `start_scan`'s completion handler, catches panics into `ScanError::Panicked`.
  Hardlink dedup: dedup only the summary byte total, leave the per-entry snapshot RAW (the writer dedups).
- **Cost budget: score read LATENCY as a FRACTION of slow reads, NEVER a total.** An anchor (depth 5) is refused past 5%
  slow reads, floored at ≥10 slow reads and >5 s wasted. A skipped dir is one we NEVER listed: ❌ never diff it with an
  empty listing (it reaps the subtree), ❌ never stamp its `listed_epoch` (least of all `0` — it absorbs up to `~`/`/`).
- **Verification's two teeth (`verify_affected_dirs`, code in `../watch/`), one constant `HUGE_DIR_CHILDREN`
  (200,000):** a DB-side `count_children_capped` probe BEFORE the snapshot, and a `read_dir` ITERATION cap (not an upsert
  cap). ❌ A declined dir must NOT be marked `listed_epoch = 0` (same absorb trap); it keeps claiming exact (owned debt).
- **Per-subtree rescan throttle: each anchor ≤ 1 reconcile per `RESCAN_THROTTLE_WINDOW` (60 s), leading + trailing**, on
  a `Utility`-QoS thread so background walks never outrank the webview.
- **Depth-split `MustScanSubDirs`:** SHALLOW (`depth ≤ 2`) → VISIBLE scanner (`start_scan`), NO hourglass hold, NEVER
  `pending_rescans` (holding it is the stuck-hourglass bug); DEEP (`≥ 3`) → throttled drain. Only the live path and
  post-replay handoff route by depth.
- **A shallow anchor sweeps at most ONCE A DAY, BOOT DISK ONLY** (`SHALLOW_RESCAN_MIN_INTERVAL = 24 h`); a mount-rooted
  volume keeps the 45 s cooldown (the verifier is root-scoped, so an external drive has no cover between sweeps).
  Coalesced anchors are COUNTED (since the last COMPLETED sweep); the badge stays GREEN by design. The window is
  wall-clock and persisted, seeded from `max(shallow_sweep_at, scan_completed_at)`; a triggered sweep stamps
  `shallow_sweep_at` at once (else an interrupted sweep looks never-swept and rescans every launch).

Full depth on all of the above: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
