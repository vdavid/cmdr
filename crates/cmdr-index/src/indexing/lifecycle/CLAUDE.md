# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. All invariants below hold PER volume id.

## Module map

- **state.rs** (+ `state/tests.rs`) — `INDEX_REGISTRY`, `IndexInstance`, the `IndexPhase` machine, reservation / start /
  stop / clear / `force_scan`, the failure supervisor, `IndexManager`/`ReadPool` bootstrap.
- **manager.rs** — the per-volume coordinator + LOCAL scan dispatch; **network_scan.rs** its SMB/MTP trait-scan path,
  **scan_completion.rs** the post-scan handler.
- **freshness.rs** — the Fresh/Stale/Scanning/Failed table. **failure.rs** — the fatal-storage signal.
  **lifecycle_bus.rs** — the neutral scan-completed / registration / dirs-changed bus. **master.rs** — the master
  switch + the composed per-drive gate + `drives_to_resume`.

## Must-knows

- **`INDEX_REGISTRY` is the authority** for which volumes are indexed. It guards lifecycle ONLY; reads route through the
  per-volume `ReadPool`, never under the lock. **Disabled = the ABSENCE of a key** (no `IndexPhase::Disabled`);
  `get_status`/`is_active` treat absent as disabled.
- **Root is special-cased to module globals.** Its `ReadPool`/`PendingSizes` live in `READ_POOL`/`PENDING_SIZES` (same
  `Arc`s as the instance); non-root handles live only in the instance.
- **The phase MACHINE (`IndexPhase`) is here; the phase EVENT is not.** Fire it through `events::set_phase_for`, never
  `DEBUG_STATS.set_phase` directly.
- **`start_indexing` is lock-first**: reserve the slot (`try_reserve_initializing_phase`) BEFORE building
  `IndexManager`, else two starts race two writer threads on one DB. A second start for a volume no-ops.
- **Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call.** Drop the guard before the shutdown drain
  (`stop_indexing`/`clear_index`) AND the blocking scan-start (`force_scan` / the journal-gap fallback): holding froze
  the UI, and re-locking under it self-deadlocked on real hardware.
- **A manual rescan routes by the TYPED kind** (`force_scan` → `force_rescan` → `rescan_scanner_for_kind`): SMB/MTP →
  `start_volume_scan`, local → `start_scan`. Never `start_scan` a trait-scanned volume: it walks nothing in ~2 ms and
  falsely marks the index complete with 0 entries.
- **`IndexVolumeKind` is a capability model**; branch on the axis, not the variant. `has_event_journal()` (only `Local`)
  gates journal replay, NOT `last_event_id.is_some()` (`LocalExternal` persists an id with no journal).
- **Freshness has ONE total transition table** (`Freshness::on`). No journal ⇒ load Stale on launch; journaled ⇒ Fresh.
  `apply_freshness_event_on` (on the `Arc`) vs `apply_freshness_event` (under the lock) is LOCK DISCIPLINE, not style.
- **The Failed state** (`failure.rs`): a fatal storage error STOPS + FAILS the index, never retries (one incident logged
  12,700 warnings in 8 min). Typed, terminal; recovery is rebuild.
- **TWO switches, master wins.** `indexing.enabled` (`master.rs`) is a HARD gate: off ⇒ nothing indexes, autonomous
  resumes included; per-drive intent only picks WHICH drives run while it's on. Enforced in `start_indexing_for`, the
  choke point all four transports share — ❌ no start path around it. Master-off stops via `stop_indexing`, which must
  never write per-drive intent, or the user's choices couldn't be restored.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups, so FDA gates ONLY `root`.
  A narrow deferral, NOT the master switch; never feed it into `set_master_enabled`.
- **The lifecycle bus is neutral and one-way** (consumer → indexing): `watch`, not `broadcast`, `send_replace` so a
  pre-subscribe `ScanCompleted` isn't lost.
- **`publish_dirs_changed` takes ORIGIN dirs (whose own listings changed), ❌ never their ancestor closure.** Consumers
  expand DOWNWARD, so one ancestor rescores its whole subtree: `/Users` in every batch cost ~90 k rescored folders a
  minute. `reconciler::with_ancestor_closure` rebuilds the size-refresh set for the FE emit + hourglass.

Owned elsewhere (each has its own `CLAUDE.md`), point don't restate: writer / `dir_stats` / epochs (`../writer/`); phase
EVENT + progress (`../events/`); `IndexPathSpace` (`../paths/`); schema (`../store/`); walker (`../scanner/`); trait BFS
(`../network_scanner/`); per-transport enable + watch (`../transports/`); event loop (`../watch/`); memory + retention
(`../resources/`).

Depth on every bullet above: `DETAILS.md`. Read it before any non-trivial work here.
