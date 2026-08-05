# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Every invariant here holds PER volume id.

## Module map

- **state.rs** (+ `state/tests.rs`) — `INDEX_REGISTRY`, `IndexInstance`, the `IndexPhase` machine, every transition, the
  failure supervisor, the `IndexManager` + `ReadPool` bootstrap.
- **manager.rs** the per-volume coordinator (+ **manager/start.rs**, LOCAL scan and journal-replay starts);
  **network_scan.rs** the SMB/MTP trait scan; **scan_completion.rs** post-scan; **progress_reporter.rs** +
  **partial_agg.rs** the 500 ms progress pump.
- **cover.rs** (+ **cover/bootstrap.rs**, what has to exist before a walk can run; **cover/live.rs**, which frontier
  roots are walked right now) — the search-driven walk over a coverage frontier (`Index::cover`), on any ground.

- **freshness.rs**, **failure.rs**, **master.rs** (the master switch), **lifecycle_bus.rs** (the neutral scan-completed
  / registration / dirs-changed bus).

## Must-knows

- **`INDEX_REGISTRY` is the authority** on which volumes have a live index, and guards lifecycle ONLY. **Disabled = the
  ABSENCE of a key** (no `IndexPhase::Disabled`); `get_status`/`is_active` read absent as disabled. But **present ≠
  indexed**: a walk-built index is `is_active` with nothing ever scanned, so an enable asks `awaits_its_first_scan`
  (`Index::start_volume`), else the drive never indexes.
- **Handles are PUSHED down, never pulled up.** A volume's `ReadPool`/`PendingSizes` live in `../read/handles.rs`; its
  stop token reaches a walk from whoever holds the instance. ❌ Nothing below `lifecycle` may import `lifecycle::state`:
  reads would wait on the lock teardown holds, and a late lookup hands a doomed walk a token that never fires.
- **Withdraw the read handles BEFORE the drain, and before any DB file goes.** Withdrawal IS what makes reads skip, so
  `Failed` (kept registered for an honest badge) needs no read-path special case.
- **The phase MACHINE (`IndexPhase`) is here; the phase EVENT is not.** Fire it via `events::set_phase_for`, never
  `DEBUG_STATS.set_phase`.
- **`start_indexing` is lock-first**: reserve the slot (`try_reserve_initializing_phase`) BEFORE building
  `IndexManager`, else two starts race two writer threads on one DB. A second no-ops.
- **Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call.** Drop the guard before the shutdown drain
  AND the blocking scan-start: holding froze the UI; re-locking self-deadlocked on real hardware.
- **A manual rescan routes by the TYPED kind** (`rescan_scanner_for_kind`): SMB/MTP → `start_volume_scan`, local →
  `start_scan`. ❌ Never `start_scan` a trait-scanned volume: it walks nothing and falsely completes.
- **A cover walk reuses the RUNNING writer; no index ⇒ it stands one up** (`Activation::WriterOnly`: DB, epoch, writer,
  read handles; ❌ no scan or watcher, `../store/` owns the `EXCLUSION_POLICY_KEY` stamp). EVERY kind is walkable —
  `Ground` picks the guarded walker or the volume's own `Volume` — and only an unmounted id is refused. ⚠️ A volume
  mid-SCAN isn't walked, and two walks over overlapping frontiers don't both take the ground (`cover/live.rs` claims
  roots): one writer doesn't stop `INSERT OR IGNORE` dropping a collider and orphaning its subtree.
- **`CoverOutcome::abandoned_ground` is independent of every other field**: every root covered, uncancelled, and still
  short. ❌ A caller reporting completeness must consult it. Live progress rides the same `WalkHeartbeat`
  (`CoverWalk::dirs_scanned_counter` / `current_dir_slot`).
- **A walk stops through the CALLER's `CancellationToken`** — `CoverWalk` holds a `Receiver`, so it can't reach the
  thread that decides to stop it. It **flushes its writer before reporting** (cancel path too), so "what's still
  uncovered" is true the moment it ends, not one search later.
- **`IndexVolumeKind` is a capability model**; branch on the axis, not the variant. `has_event_journal()` (only `Local`)
  gates journal replay, NOT `last_event_id.is_some()` (`LocalExternal` persists an id, no journal).
- **Freshness has ONE total transition table** (`Freshness::on`); no journal ⇒ load Stale on launch. `..._on` (fires on
  the `Arc`) vs `apply_freshness_event` (looks up under the lock) is LOCK DISCIPLINE, not style.

- **A fatal storage error STOPS + FAILS the index, never retries** (one incident logged 12,700 warnings in 8 min). Typed
  and terminal; recovery is rebuild.
- **TWO switches, master wins, and both gate BACKGROUND work only.** `indexing.enabled` hard-gates
  `Activation::IndexTheVolume` in `start_indexing_for`, the choke point all four transports share. Master-off stops via
  `stop_indexing`, which must ❌ never write per-drive intent. ⚠️ A search walk (`WriterOnly`) is carved out of both
  switches AND `user_disabled`: a read someone asked for, starting nothing autonomous, so refusing only makes their
  search wrong. ❌ Don't "fix" that into a refusal.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups, so FDA gates ONLY `root`.
  A narrow deferral, NOT the master switch; ❌ never feed it to `set_master_enabled`.
- **The lifecycle bus is neutral and one-way** (consumer → indexing): `watch` not `broadcast`, `send_replace` so a
  pre-subscribe `ScanCompleted` survives.
- **`publish_dirs_changed` takes ORIGIN dirs, ❌ never their ancestor closure.** Consumers expand DOWNWARD, so one
  ancestor rescores its whole subtree (`/Users` in every batch cost ~90 k folders a minute).

Owned elsewhere, point don't restate: `../writer/` (`dir_stats`, epochs), `../events/` (sink seam + phase EVENT),
`../paths/`, `../store/`, `../scanner/`, `../network_scanner/`, `../transports/`, `../watch/`, `../resources/`.

Depth on every bullet above: `DETAILS.md`. Read it before any non-trivial work here.
