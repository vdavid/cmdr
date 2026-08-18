# Write operations

Copy, move, delete, trash, and zip edits as managed background ops: progress, cancellation, conflicts, rollback.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, `CopyTransaction`, cancel/abort),
  `status_cache.rs` (status + the busy-volume set that disables Eject; reach it through `state::`), `mod.rs` (public
  API). Scan preview: `scan_{preview,cache,bridge,watchdog}.rs`. Cross-volume routing: `routing.rs`. Source identity:
  `source_binding.rs`. Own docs: `transfer/`, `delete/`, `archive_edit/`, and the frontend counterpart
  `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**; blocking work inside
  `spawn_blocking`, so `*_files_start` returns an id before any I/O. Zip edits: `archive_edit/`.
- **A spawned op reserves every lane it touches or waits Queued**; the next admits on `on_settled`, ❌ never in
  `Drop`.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it. Cancel keeps copied files, Rollback deletes them
  in reverse. `PauseGate` is orthogonal; cancel wins.
- **Parking on a PERSON owes two calls**: the `human_wait.rs` clock AND `state.announce_human_wait(sink)` on both
  edges. Miss one and the ETA collapses on resume; miss the other and surfaces show a speed over a stopped op.
- **Stopping has two tiers**: clicks use `backend_cancel`; `backend_abort` and `cancel_all_write_operations` belong
  to the quit deadline alone (`transfer/DETAILS.md`).
- **Arm `state.conflict_slot` with the QUESTION before emitting `write-conflict`** (emit-first hangs the recv); the
  dispatch mutex ❌ never spans a file write.
- **An answer NAMES its clash** (`ConflictId`); `resolve_write_conflict` REPORTS a `ConflictResolutionOutcome`, from
  where it LANDS. ❌ Never collapse `AlreadyResolved` / `StaleAnswer` / `NoPendingConflict`, or leave a prompt up for a
  settled question — a modal blocks every new op.
- **Emit through `OperationEventSink`, ❌ never `AppHandle`** — built at the IPC edge, injected down.
  `write-settled` fires once, AFTER the terminal event.
- **`write-source-item-done` is the per-source verdict** (`Done` / `Skipped` / `Failed`). A cross-FS move speaks
  twice for one source, so the LAST event wins; `source_removed` is a separate question, driving snapshot purge.
- **A caller may BIND the sources an op may touch** (`source_binding.rs`, an `Option` on all four starters): one no
  longer matching its promised identity is dropped before any I/O and reported `Skipped`. ❌ Nothing here asks WHO
  started an op; approved work is ordinary work (`approved_op_parity_tests.rs`).
- **One routing for every cross-volume transfer** (`routing.rs`: `start_volume_{copy,move,compress}`); the IPC
  command only builds the sink. Extract IS a copy with an `ArchiveVolume` source, ❌ no `Extract` op type.
- **Register a destination with `downloads::note_pending_write_for_cmdr` BEFORE the syscall** (renames both).
- **EVERY local write lands via temp + rename** (`overwrite::stage_and_land_file`), rename-aside when replacing.
  Temps carry the recoverable `.cmdr-` marker and register via `in_flight_temps`. Symlinks are never followed.
- **❌ Never `statvfs` for macOS disk space** (it rejects copies APFS purgeable space allows); use
  `crate::volumes::get_volume_space()`.
- **Scans report `total_bytes` (copy/move) and `dedup_bytes` (delete)**: ❌ never point copy at the dedup'd one, it
  under-reserves disk.
- **Every managed mutation journals by `op_id`**, and a VOLUME op passes the REAL volume id, never `"root"`
  (`../../operation_log/DETAILS.md`).
- **Bulk rename journals every hop as it lands.** ❌ Never batch to the end, and never put a rotation temp in
  `in_flight_temps`, whose sweep DELETES.
- **`LifecycleStatus` is the ONE lifecycle answer**, carried as `OperationStatus.lifecycle`. ❌ Never re-derive one
  from a presence test, and no new variant.
- **Every preview runs under a `ScanWatchdog`, and whoever settles it CLAIMS the outcome first**
  (`watchdog.claim_outcome()`). It bounds by INACTIVITY (60 s), so feed the progress callback.
- **A FAILED op is retained out-of-band**, the one exception to removal-on-terminal; `record_failure` emits only
  once the record is GONE.
- **Op state in a test hangs off a guard, not a `static`** (`test_support::TestOperationGuard`,
  `operation_log::TestJournalGuard`); ❌ never `cancel_all_write_operations()` there.

Architecture, flows, and decisions: `DETAILS.md`. Read it before non-trivial work here.
