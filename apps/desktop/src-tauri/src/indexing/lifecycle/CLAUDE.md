# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Owns the registry, the `IndexPhase` machine, lock
discipline, freshness, the Failed state, the `IndexManager` coordinator, and the lifecycle bus.

All invariants below hold PER volume id.

## Module map

- **state.rs** (+ `state/tests.rs`) — the `INDEX_REGISTRY` + `IndexInstance` + `IndexPhase` machine + `IndexVolumeKind`
  + the reservation / start / stop / clear / `force_scan` + the failure supervisor + `IndexManager`/`ReadPool` bootstrap.
- **manager.rs** — `IndexManager`, the per-volume coordinator + the LOCAL scan dispatch. **network_scan.rs** — its
  SMB/MTP `Volume`-trait scan path (a sibling `impl IndexManager`). **scan_completion.rs** — the post-scan handler.
- **freshness.rs** — the Fresh/Stale/Scanning/Failed transition table. **failure.rs** — the fatal-storage-error signal.
  **lifecycle_bus.rs** — the neutral scan-completed / registration / dirs-changed bus.

## Must-knows

- **`INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority** for WHICH volumes are indexed and
  each volume's lifecycle. It guards lifecycle ONLY; reads route through the per-volume `ReadPool`, never under the lock.
  **Disabled = the ABSENCE of a key** (no `IndexPhase::Disabled`); `get_status`/`is_active` treat absent as disabled.
- **Root is special-cased to module globals.** Root's `ReadPool`/`PendingSizes` live in `READ_POOL`/`PENDING_SIZES`
  (the instance holds the SAME `Arc`s); non-root handles live only in the instance.
- **The phase MACHINE (`IndexPhase`: Initializing/Running/ShuttingDown/Failed) is here; the phase EVENT is not.** Fire
  the pipeline-phase transition through `events::set_phase_for` (owned by [`../events`](../events/CLAUDE.md)); never
  `DEBUG_STATS.set_phase` directly.
- **`start_indexing` is lock-first**: reserve the registry slot (`try_reserve_initializing_phase`) BEFORE building
  `IndexManager` (else two starts spawn two writer threads racing one DB). A second start for the same volume no-ops.
- **Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call.** Drop the guard before the shutdown drain
  (`stop_indexing`/`clear_index`) AND before the blocking scan-start (`force_scan` / the journal-gap fallback). Holding
  froze the UI once, and re-locking under it self-deadlocked on real hardware. Scan-start freshness fires through the
  manager's own freshness `Arc`, not a registry re-lock.
- **A manual rescan routes by the TYPED kind.** `force_scan` → `force_rescan` → `rescan_scanner_for_kind`: SMB/MTP →
  `start_volume_scan` (trait walk), local → `start_scan` (guarded walker). Never `start_scan` a trait-scanned volume; it
  walks nothing in ~2 ms and falsely marks the index complete with 0 entries. Classify by `IndexVolumeKind`, never an id
  substring.
- **`IndexVolumeKind` is a capability model** (`uses_local_scanner`/`is_trait_scanned`, `has_event_journal`,
  `mount_rooted`, `feeds_search`); branch on the axis, not the variant. `has_event_journal()` (only `Local`) gates
  journal replay, NOT `last_event_id.is_some()` (a `LocalExternal` index persists an id with no journal to replay).
- **Freshness has ONE total transition table** (`Freshness::on`, `freshness.rs`). No journal ⇒ load Stale on launch
  (`initial_freshness_on_launch`); journaled (local) ⇒ Fresh. `apply_freshness_event_on` (on the `Arc`, no registry
  lock) vs `apply_freshness_event` (looks up under the lock) is a LOCK-DISCIPLINE choice, not style.
- **The Failed state** (`failure.rs` + `IndexPhase::Failed` + `Freshness::Failed`): a fatal storage error STOPS + FAILS
  the index, never retries (one incident logged 12,700 warnings in 8 min). Typed classification, never a message
  substring; `Freshness::Failed` is terminal; recovery is rebuild.
- **Defer `root` auto-start** (`should_auto_start_indexing`): scanning `/` stacks TCC popups, so FDA gates ONLY `root`.
- **The lifecycle bus is neutral and one-way** (consumer → indexing): `watch`, not `broadcast`, and `send_replace` so a
  pre-subscribe `ScanCompleted` isn't lost.

Owned elsewhere, point don't restate: writer / `dir_stats` / epochs / `WRITER_GENERATION`
([`../writer`](../writer/CLAUDE.md)); phase EVENT + scan progress ([`../events`](../events/CLAUDE.md)); `IndexPathSpace`
+ firmlinks ([`../paths`](../paths/CLAUDE.md)); SQLite schema ([`../store`](../store/CLAUDE.md)); walker + exclusions
([`../scanner`](../scanner/CLAUDE.md)); trait BFS ([`../network_scanner`](../network_scanner/CLAUDE.md)); per-transport
enable + live watch, the direct-smb2 gate, local-external classify, unmount/eject
([`../transports`](../transports/CLAUDE.md)); event loop / watcher ([`../watch`](../watch/CLAUDE.md)); memory + retention
caps ([`../resources`](../resources/CLAUDE.md)).

The registry, phase and freshness machines, the Failed state, lock discipline, and the bus: [DETAILS.md](DETAILS.md).
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
