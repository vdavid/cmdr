# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Every invariant below holds PER volume id.

## Module map

- **state.rs** (+ `state/tests.rs`) — `INDEX_REGISTRY`, `IndexInstance`, the `IndexPhase` machine, every transition, the
  failure supervisor, and the `IndexManager` + `ReadPool` bootstrap.
- **manager.rs** the per-volume coordinator (+ **manager/start.rs**, the LOCAL scan and journal-replay starts);
  **network_scan.rs** the SMB/MTP trait scan; **scan_completion.rs** post-scan; **progress_reporter.rs** +
  **partial_agg.rs** the 500 ms progress pump.
- **freshness.rs**, **failure.rs**, **master.rs** (the master switch), and **lifecycle_bus.rs** (the neutral
  scan-completed / registration / dirs-changed bus).

## Must-knows

- **`INDEX_REGISTRY` is the authority** on which volumes are indexed, and it guards lifecycle ONLY. **Disabled = the
  ABSENCE of a key** (no `IndexPhase::Disabled`); `get_status`/`is_active` read absent as disabled.
- **Handles are PUSHED down, never pulled up.** A volume's `ReadPool`/`PendingSizes` live in `../read/handles.rs`; its
  stop token reaches a walk from whoever holds the instance. ❌ Nothing below `lifecycle` may import `lifecycle::state`:
  reads would wait on the lock teardown holds, and a late token lookup hands a doomed walk a token that never fires.
- **Withdraw the read handles BEFORE the drain, and before any DB file goes.** Withdrawal IS what makes reads skip,
  which is why `Failed` (kept registered so the badge is honest) needs no read-path special case.
- **The phase MACHINE (`IndexPhase`) is here; the phase EVENT is not.** Fire it through `events::set_phase_for`, never
  `DEBUG_STATS.set_phase`.
- **`start_indexing` is lock-first**: reserve the slot (`try_reserve_initializing_phase`) BEFORE building
  `IndexManager`, else two starts race two writer threads on one DB. A second start for a volume no-ops.
- **Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call.** Drop the guard before the shutdown drain
  AND the blocking scan-start: holding froze the UI, and re-locking under it self-deadlocked on real hardware.
- **A manual rescan routes by the TYPED kind** (`rescan_scanner_for_kind`): SMB/MTP → `start_volume_scan`, local →
  `start_scan`. ❌ Never `start_scan` a trait-scanned volume: it walks nothing and falsely completes with 0 entries.
- **`IndexVolumeKind` is a capability model**; branch on the axis, not the variant. `has_event_journal()` (only `Local`)
  gates journal replay, NOT `last_event_id.is_some()` (`LocalExternal` persists an id with no journal).
- **Freshness has ONE total transition table** (`Freshness::on`); no journal ⇒ load Stale on launch. `..._on` (fires on
  the `Arc`) vs `apply_freshness_event` (looks up under the lock) is LOCK DISCIPLINE, not style.
- **A fatal storage error STOPS + FAILS the index, never retries** (one incident logged 12,700 warnings in 8 min).
  Typed, terminal; recovery is rebuild.
- **TWO switches, master wins.** `indexing.enabled` is a HARD gate (autonomous resumes included), enforced in
  `start_indexing_for`, the choke point all four transports share — ❌ no start path around it. Master-off stops via
  `stop_indexing`, which must ❌ never write per-drive intent, or the user's choices couldn't be restored.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups, so FDA gates ONLY `root`.
  A narrow deferral, NOT the master switch; ❌ never feed it into `set_master_enabled`.
- **The lifecycle bus is neutral and one-way** (consumer → indexing): `watch`, not `broadcast`; `send_replace`, so a
  pre-subscribe `ScanCompleted` isn't lost.
- **`publish_dirs_changed` takes ORIGIN dirs, ❌ never their ancestor closure.** Consumers expand DOWNWARD, so one
  ancestor rescores its whole subtree (`/Users` in every batch cost ~90 k rescored folders a minute).

Owned elsewhere, point don't restate: `../writer/` (`dir_stats`, epochs), `../events/` (the sink seam + phase EVENT),
`../paths/`, `../store/`, `../scanner/`, `../network_scanner/`, `../transports/`, `../watch/`, `../resources/`.

Depth on every bullet above: `DETAILS.md`. Read it before any non-trivial work here.
