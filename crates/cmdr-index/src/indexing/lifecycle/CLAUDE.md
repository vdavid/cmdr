# Indexing lifecycle (the per-volume registry + state machine)

How a per-volume index is born, lives, transitions, and dies. Every invariant here holds PER volume id.

## Module map

- **state.rs** — the registry, the `IndexPhase` machine and its transitions, the failure supervisor, the
  `IndexManager` + `ReadPool` bootstrap.
- **manager.rs** the per-volume coordinator (+ **manager/start.rs**); **network_scan.rs** the SMB/MTP trait scan;
  **scan_completion.rs** post-scan; **progress_reporter.rs** + **partial_agg.rs** the 500 ms progress pump.
- **cover.rs** (+ **cover/bootstrap.rs**, **cover/live.rs**) — the search-driven walk over a coverage frontier.
- **freshness.rs**, **failure.rs**, **master.rs**, **lifecycle_bus.rs**.

## Must-knows

- **`INDEX_REGISTRY` is the authority** on which volumes have a live index, and guards lifecycle ONLY. **Disabled = the
  ABSENCE of a key.** But **present ≠ indexed**: a walk-built index is `is_active` with nothing ever scanned, so an
  enable asks `awaits_its_first_scan`, else the drive never indexes.
- **Handles are PUSHED down, never pulled up**: a volume's `ReadPool`/`PendingSizes` live in `../read/handles.rs`, and
  its stop token reaches a walk from whoever holds the instance. ❌ Nothing below `lifecycle` may import
  `lifecycle::state`: reads would wait on the lock teardown holds.
- **Withdraw the read handles BEFORE the drain, and before any DB file goes**: withdrawal IS what makes reads skip, so
  `Failed` (kept registered for an honest badge) needs no read-path case.
- **The phase MACHINE is here; the phase EVENT is not.** Fire it via `events::set_phase_for`.
- **`start_indexing` is lock-first**: reserve the slot BEFORE building `IndexManager`, else two starts race two writer
  threads on one DB. ❌ Never hold `INDEX_REGISTRY` across a blocking or re-entrant manager call: it froze the UI, and
  self-deadlocked on real hardware.
- **A manual rescan routes by the TYPED kind** (`rescan_scanner_for_kind`). ❌ Never `start_scan` a trait-scanned
  volume: it walks nothing and falsely completes.
- **A cover walk reuses the RUNNING writer; no index ⇒ it stands one up** (`Activation::WriterOnly`: DB, epoch, writer,
  read handles, ❌ no scan or watcher), and EVICTS one whose coverage this build refuses (`../resources/DETAILS.md` §
  "Rebuilt-from-scratch coverage is EVICTED"). EVERY kind is walkable; only an unmounted id is refused. ⚠️ A volume
  mid-SCAN isn't walked, and two overlapping walks don't both take the ground (`cover/live.rs` claims roots): one writer
  doesn't stop `INSERT OR IGNORE` orphaning a collider's subtree.
- **`CoverOutcome::abandoned_ground` is independent of every other field**: every root covered, uncancelled, and still
  short, so ❌ a caller reporting completeness must consult it.
- **A walk stops through the CALLER's `CancellationToken`** (`CoverWalk` holds a `Receiver`) and **flushes its writer
  before reporting**, cancel path included, so "what's still uncovered" is true the moment it ends.
- **`IndexVolumeKind` is a capability model**: branch on the axis, not the variant. `has_event_journal()` (only `Local`)
  gates journal replay, ❌ not `last_event_id.is_some()`.
- **Freshness has ONE total transition table** (`Freshness::on`); no journal ⇒ load Stale on launch. `..._on` (on the
  `Arc`) vs `apply_freshness_event` (under the lock) is LOCK DISCIPLINE, not style.
- **A fatal storage error STOPS + FAILS the index, never retries** (one incident logged 12,700 warnings in 8 min). Typed
  and terminal; recovery is a rebuild.
- **TWO switches, master wins, and both gate BACKGROUND work only.** `indexing.enabled` hard-gates
  `Activation::IndexTheVolume` in `start_indexing_for`, the choke point all four transports share; master-off stops via
  `stop_indexing`, which ❌ must never write per-drive intent. ⚠️ A search walk (`WriterOnly`) is carved out of both
  switches AND `user_disabled`. ❌ Don't "fix" that into a refusal.
- **Defer `root` auto-start**: scanning `/` stacks TCC popups, so FDA gates ONLY `root`. A narrow deferral, ❌ never fed
  to `set_master_enabled`.
- **The lifecycle bus is neutral and one-way**: `watch` not `broadcast`, `send_replace` so a pre-subscribe
  `ScanCompleted` survives. `publish_dirs_changed` takes ORIGIN dirs, ❌ never their ancestor closure — consumers expand
  DOWNWARD, so one ancestor rescores its subtree (`/Users` cost ~90 k folders a minute).

Owned elsewhere, point don't restate: `../writer/` (`dir_stats`, epochs), `../events/`, `../paths/`, `../store/`,
`../scanner/`, `../network_scanner/`, `../transports/`, `../watch/`, `../resources/`.

Depth on every bullet: `DETAILS.md`. Read it before any non-trivial work here.
