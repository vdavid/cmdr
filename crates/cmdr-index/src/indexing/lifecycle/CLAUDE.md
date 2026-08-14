# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Every invariant here holds PER volume id.

`state.rs` the registry + `IndexPhase` machine, with a job per file under `state/`, re-exported so `state::*` stays the
one path; `manager.rs` (+ `manager/start.rs`) the per-volume coordinator; `network_scan.rs` the SMB/MTP trait scan;
`scan_completion.rs`; `progress_reporter.rs` + `partial_agg.rs` the 500 ms progress pump; `cover.rs` the search-driven
walk (bootstrap + ground-claiming rules in `cover/CLAUDE.md`); `rescan_request.rs` the typed scan-start
refusal + the owed walk; `freshness.rs`, `failure.rs`, `master.rs`, `lifecycle_bus.rs`.

## Must-knows

- **`INDEX_REGISTRY` guards lifecycle ONLY, and disabled = the ABSENCE of a key.** Present ≠ indexed: an enable asks
  `awaits_its_first_scan`, else a walk-built index never scans.
- **Handles are PUSHED down, never pulled up** (`../read/handles.rs`). ❌ Nothing below `lifecycle` may import
  `lifecycle::state`: reads would wait on the lock teardown holds.
- **Withdraw the read handles BEFORE the drain, and before any DB file goes.** Withdrawal IS the read-skip, so `Failed`
  needs no read-path case.
- **The phase MACHINE is here; the phase EVENT is not**: fire it via `events::set_phase_for`.
- **`start_indexing` is lock-first**: reserve the slot before building `IndexManager`, or two starts race two writers on
  one DB. ❌ Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call.
- **A manual rescan routes by the TYPED kind** (`rescan_scanner_for_kind`). ❌ Never `start_scan` a trait-scanned
  volume: it walks nothing and falsely completes.

- **A cover walk reuses the RUNNING writer, or stands one up** (`Activation::WriterOnly`, ❌ no scan or watcher), and
  EVICTS an index whose coverage this build refuses. ⚠️ A volume mid-SCAN isn't walked.
- **`CoverOutcome::abandoned_ground` is independent of every other field**, so ❌ any caller reporting completeness must
  consult it.
- **Every scan entry asks TWO single-flight questions** (`start_scan`, `start_volume_scan`): `mgr.scanning` AND
  `cover::ground_being_walked`. A search walk sets no flag, and truncating under one blanks rows it's still writing. ❌
  Don't collapse them or classify them by text (both are `ScanStartError`). A MANUAL rescan they refuse is REMEMBERED
  (`rescan_request`) and run by the walk from `cover::release_ground` (claim first, THEN the scan) via `force_scan`,
  re-asking both.
- **A walk RELEASES its branch whatever the registry phase** (`finish_branch_coverage` reaches the set directly). ❌
  Never behind `with_running_manager`: a walk ending inside a rescan's `ShuttingDown` window would hold that ground
  forever.
- **A walk stops through the CALLER's token and flushes its writer before reporting**, cancel included.
- **`IndexVolumeKind` is a capability model**: branch on the axis, not the variant. `has_event_journal()` gates journal
  replay, ❌ not `last_event_id.is_some()`.
- **Freshness has ONE total transition table** (`Freshness::on`); no journal ⇒ Stale on launch. `..._on` vs
  `apply_freshness_event` is LOCK DISCIPLINE, not style.
- **A fatal storage error STOPS + FAILS the index, never retries** (one incident logged 12,700 warnings in 8 min);
  recovery is a rebuild.
- **TWO switches, master wins, and both gate BACKGROUND work only.** `indexing.enabled` hard-gates
  `Activation::IndexTheVolume`, the choke point all four transports share; master-off goes through `stop_indexing`,
  which ❌ must never write per-drive intent. ⚠️ A search walk is carved out of both switches AND `user_disabled`; ❌
  don't "fix" that into a refusal.
- **Defer `root` auto-start**: scanning `/` stacks TCC popups, so FDA gates ONLY `root`, ❌ never `set_master_enabled`.
- **The lifecycle bus is neutral and one-way** (`watch` + `send_replace`, so a pre-subscribe `ScanCompleted` survives).
  `publish_dirs_changed` takes ORIGIN dirs, ❌ never their ancestor closure: consumers expand DOWNWARD, and one ancestor
  cost ~90 k folders a minute.

Owned elsewhere, point don't restate: `../writer/`, `../events/`, `../paths/`, `../store/`, `../scanner/`,
`../network_scanner/`, `../transports/`, `../watch/`, `../resources/`.

Depth on every bullet: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
