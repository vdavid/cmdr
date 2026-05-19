# Write operations

Copy, move, delete, and trash with streaming progress, cancellation, conflict resolution, and rollback. macOS and Linux.

## Purpose

Implements the four destructive file operations as background tasks that stream Tauri events to the frontend. Every
operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites,
network mounts, cross-filesystem moves, and name/path length limits.

Pre-flight scans reuse cached listings when the source volume reports an active watcher, avoiding redundant
`list_directory` calls. The freshness contract and per-backend debounce windows are documented in
`../volume/CLAUDE.md` and `../listing/caching.rs::try_get_watched_listing`.

## Files

| File | Responsibility |
|------|----------------|
| `mod.rs` | Public API: `copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start`. Each delegates to `start_write_operation` which handles state creation, spawn lifecycle, cleanup, and error/panic recovery. Validation runs inside the handler closure on the blocking thread pool, never on the async executor. |
| `types.rs` | All serializable types: events, config, errors, results. `WriteOperationConfig`, `ConflictResolution`, `WriteOperationError`, `DryRunResult`, scan preview events. Also: `OperationEventSink` trait (decouples event emission from `tauri::AppHandle`), `TauriEventSink` (production), `CollectorEventSink` (test-only). |
| `state.rs` | Two `LazyLock<RwLock<HashMap>>` caches (`WRITE_OPERATION_STATE`, `OPERATION_STATUS_CACHE`). `WriteOperationState`, `CopyTransaction`, `ScanResult`, `FileInfo`. |
| `helpers.rs` | Validation (`validate_sources`, `validate_destination_writable` via `libc::access`, `validate_disk_space` via `statvfs`). Conflict resolution (`tokio::sync::oneshot` channel wait for Stop mode). `safe_overwrite_file`/`safe_overwrite_dir` (temp+rename). `find_unique_name`. `run_cancellable`. `is_same_filesystem` (device IDs). Background cleanup helpers: `remove_file_in_background`, `remove_dir_all_in_background`. |
| `scan.rs` | `scan_sources` (recursive walk, emits progress), `dry_run_scan`, shared `walk_dir_recursive` walker. The `on_progress` callback receives `(files, dirs, bytes, current_file, current_dir)`; the walker reads `current_dir` from `path.parent()` so the UI can show "in directory: …" alongside the filename. Scan emit sites populate `WriteProgressEvent.current_dir` plus index-derived `expected_files_total` / `expected_bytes_total` (via `WriteProgressEvent::with_scan_meta`) so the frontend renders a real progress bar during the foolproof re-scan. Expected totals come from `crate::indexing::expected_totals::expected_totals_for_sources` (`None` when the index doesn't cover all sources; the FE falls back to a tally-only display). |
| `scan_preview.rs` | Scan preview subsystem for Copy dialog live stats: `start_scan_preview`, `cancel_scan_preview`, `is_scan_preview_complete`. Background scans (local and volume-based) with result caching. Emits `expected_files_total` / `expected_bytes_total` (sampled once at scan start from the drive index) on every `scan-preview-progress` event, alongside the running tallies and `current_dir`. |
| `copy.rs` | `copy_files_with_progress`: scan → disk space check → per-file copy via `copy_single_item`. `CopyTransaction` for rollback. The per-source execute loop runs through `drive_transfer_serial_sync` (`transfer_driver.rs`); the closure captures `&mut transaction` / `&mut created_dirs` / `&mut tracker` / `&mut apply_to_all_resolution` and threads them into `copy_single_item`. Pre-flight scan / dry-run / disk-space / bulk-skip filtering stay outside the driver. Post-loop dispatch matches on `PostLoopIntent` (Completed / Cancelled / Failed) and reproduces the historic three-arm shape — including the post-completion `RollingBack` recheck for the click-during-the-last-millisecond race (commit `1de4255d`). |
| `move_op.rs` | Same-fs: `fs::rename`. Cross-fs: copy to `.cmdr-staging-<uuid>`, atomic rename, delete sources. |
| `delete.rs` | Scan, delete files first, then directories in reverse/deepest-first order. Not rollbackable. Also contains `delete_volume_files_with_progress` for non-local volumes (MTP): consumes the scan preview via `take_cached_scan_result(preview_id)` first (top-level files come straight from `CopyScanResult` with no `is_directory` probe, top-level dirs recurse via the oracle-aware walker); on no-preview paths (MCP, programmatic) the top-level `is_directory(source)` probe stays unless the source's parent is watcher-fresh in `LISTING_CACHE`, in which case the type comes from the cached entry. The walker (`scan_volume_recursive`) consults `try_get_watched_listing(volume_id, path)` before every `list_directory`, so any subtree open in another pane is cache-fed. Scans via `volume.list_directory(path, Some(&cb))` (per-entry throttled progress so the FE tally climbs mid-listing on slow MTP roundtrips), deletes via `volume.delete()` per item. Shared cumulative tally lives in an `Arc<VolumeScanTracker>` (atomics for files/dirs/bytes + `Mutex<Instant>` throttle) so the per-entry callback and the post-subtree snapshot agree across recursion levels. Both emit paths use `with_scan_meta(current_dir, dirs_done, None)` so the scanning UI shows the dir count and the directory the walker is currently in. |
| `eta.rs` | `EtaEstimator`: time-weighted EWMA per axis (bytes, files), τ ≈ 3 s. Combines via `max(ETA_bytes, ETA_files)`. One per `WriteOperationState`, fed by `state.enrich_progress` at every `write-progress` emit site. See [ETA + throughput](#eta--throughput) below. |
| `trash.rs` | `move_to_trash_sync()` (macOS: ObjC `trashItemAtURL`; Linux: `trash` crate; reused by `commands/rename.rs`) and `trash_files_with_progress()` (batch trash with per-item progress, cancellation, partial failure). Uses `symlink_metadata()` for existence checks (handles dangling symlinks). |
| `copy_strategy.rs` | Strategy selection per file: network FS → chunked copy; overwrite → temp+rename; macOS → `copyfile(3)`; Linux → `copy_file_range(2)`. |
| `macos_copy.rs` | FFI to macOS `copyfile(3)`. Preserves xattrs, ACLs, resource forks, Finder metadata. Supports APFS `clonefile`. |
| `linux_copy.rs` | Linux `copy_file_range(2)` with reflink support on btrfs/XFS. 4 MB chunks, cancellation between iterations. |
| `chunked_copy.rs` | 1 MB chunked read/write, the default copy method for all non-APFS-clonefile copies on macOS and network copies on Linux. Checks cancellation between chunks. Copies xattrs, ACLs, timestamps. |
| `volume_copy.rs` | Volume-to-volume copy (Local↔MTP↔SMB): `copy_between_volumes`, `scan_for_volume_copy`. Uses `OperationEventSink` (not `AppHandle` directly) for event emission. Handles conflict detection, resolution, progress, rollback (delete all copied files in reverse with progress), and partial-file cleanup on cancel. Shared `map_volume_error` helper. |
| `volume_move.rs` | Volume-to-volume move: `move_between_volumes`, `move_within_same_volume`. Same-volume uses `Volume::rename`; cross-volume does copy+delete. |
| `volume_preflight.rs` | Shared preflight scan for both volume copy and move: `scan_volume_sources` returns a `VolumePreflight { total_files, total_bytes, source_hints }`. Reuses a cached preview from `TransferDialog` when one is available; otherwise dispatches `volume.scan_for_copy_batch` (so MTP's group-by-parent and SMB's pipelined-stat optimizations still kick in). Emits one `WriteProgressEvent { phase: Scanning, … }` so the FE sees the scan stage. On pre-scan cancel, emits `write-cancelled` and returns `Err(Cancelled)` so the FE dialog closes cleanly. |
| `volume_conflict.rs`, `volume_strategy.rs` | Conflict resolution (Stop/Skip/Overwrite/Rename/OverwriteSmaller/OverwriteOlder) and copy strategy selection for volume operations. `volume_conflict.rs` mirrors the local-FS `reduce_conditional_resolution` from `helpers.rs` with its own `reduce_volume_conditional_resolution` (async, uses size hints + `get_metadata` for mtime). |
| `transfer_driver.rs` | Shared per-source transfer driver for copy/move ops. Owns the bulk-skip prelude, per-iter cancellation check, conflict-resolve dispatch (async path), per-iter skip accounting, and paired progress/status emit. Two sibling entry points: `drive_transfer_serial_sync` (used by `copy.rs::copy_files_with_progress_inner`; closure captures `&mut CopyTransaction` / `&mut created_dirs` / `&mut SourceItemTracker`) and `drive_transfer_serial_async` (used by `volume_copy::copy_volumes_with_progress`'s serial path and both `volume_move` paths; driver owns top-level conflict detection + dispatch). The `FuturesUnordered` concurrent path in `volume_copy.rs` stays inline (1-of-4 abstraction; see plan § "Concurrent driver scope"). |
| `tests.rs` | Unit tests. |
| `copy_integration_test.rs` | Copy operation integration tests (permissions, symlinks, xattrs, edge cases). |
| `delete_integration_test.rs` | Delete operation integration tests. |
| `delete_volume_reuse_tests.rs` | Volume-delete tests for scan-preview reuse and oracle fast paths (M3 of `fresh-listing-reuse-plan.md`). |
| `move_integration_test.rs` | Same-fs and cross-fs move integration tests. |
| `transaction_integration_test.rs` | CopyTransaction record/rollback/commit tests. |
| `validation_integration_test.rs` | Validation functions, safety checks, path length, disk space tests. |

## Architecture / data flow

```
Frontend
  → WriteOperationState created (AtomicU8 intent, oneshot channel for Stop conflicts)
  → stored in WRITE_OPERATION_STATE + OPERATION_STATUS_CACHE
  → operationId returned to frontend immediately (dialog opens, cancel is possible)
  → tokio::spawn (async wrapper)
      → tokio::task::spawn_blocking (local I/O) or direct async (volume ops)
          → validate (sources exist, dest writable, not same location, dest not inside source)
          → scan phase: walk_dir_recursive, emit scan-progress events
              (delete on a volume also: `take_cached_scan_result(preview_id)` first;
               on hit, build the entry list from `per_path` — top-level files come
               straight from the cache, top-level dirs recurse via the oracle-aware
               walker; on miss, fall through to `scan_volume_recursive`)
          → disk space check (statvfs)
          → execute phase: per-file copy/delete
              → throttled write-progress events (200ms default)
          → success: CopyTransaction::commit(), emit write-complete
          → cancel (Stopped): CopyTransaction::commit(), emit write-cancelled (rolled_back: false)
          → cancel (RollingBack): rollback_with_progress() → emit write-progress (phase: rolling_back) → emit write-cancelled
          → error: CopyTransaction::rollback(), emit write-error
      → safety net: start_write_operation emits write-error for unhandled handler errors
  → state removed from both caches
```

## ETA + throughput

Rates and ETA are computed in the backend (`eta.rs`) and shipped on every `WriteProgressEvent` as `bytes_per_second`, `files_per_second`, and `eta_seconds`. The frontend renders these directly, with no client-side math or sample buffer.

**Why backend, not frontend:** one place to test, one set of fields exposed on the wire, identical behavior across copy/move/delete/MTP/SMB/local. Putting the math in Svelte couples the estimator to dialog lifecycle and makes any future client (CLI, menu bar app) reinvent it.

**Why two axes, not one:** the bug we hit in May 2026 was a delete of 5.4 GB / 174k files where the size bar saturated in the first second (a few large files) and the byte-based ETA collapsed to ~0 s while 165k small files were still streaming through. The estimator now tracks bytes/sec and files/sec independently and reports `eta = max(ETA_bytes, ETA_files)`. The operation can't finish before either axis is done, so the larger one is reality. When one axis has zero remaining work, its ETA is `0` and the other axis dominates naturally, with no branching needed.

**EWMA, not blended overall:** `α = 1 - exp(-Δt / τ)` with `τ = 3 s` (see `EWMA_TAU_SECS`). Pure exponential decay, no "overall average" anchor. If the network drops mid-operation, the EWMA converges to the new rate within a few τ instead of being pulled back toward historical numbers. Time-weighted means the response is the same whether progress events arrive every 50 ms or every 500 ms.

**Warm-up:** the estimator returns `None` for ETA until it has ≥ 2 samples in the current phase AND ≥ 800 ms elapsed (`MIN_SAMPLES_FOR_ETA`, `MIN_ELAPSED_FOR_ETA`). This kills the early "200 ms in, rate = 50 MB/s → ETA = 0 s" footgun. Rates are populated as soon as we have the first delta; only the ETA is gated.

**Phase transitions reset:** `update()` reseeds on every `phase` change. Without this, the counters' reset (scanning → copying both restart from 0) would feed a negative delta into the EWMA. Rollback is treated as a forward phase toward target `(0, 0)`: the estimator subtracts the new counters from the previous ones and ETA = current value / decay rate.

**Wiring:** every `write-progress` emit site calls `state.emit_progress_via_app(app, event)` (for the AppHandle-direct path: copy/delete/trash/scan/move) or `state.emit_progress_via_sink(events, event)` (for the `OperationEventSink` path: volume copy/move). Both methods call `enrich_progress` internally, so no caller has to remember. The `bytes_per_second: None, files_per_second: None, eta_seconds: None` placeholders in the struct literals get overwritten before the event reaches the FE.

**Frontend display:** `TransferProgressDialog.svelte` stores the three fields in local `$state` and renders both speeds side by side ("27.7 MB/s · 1,234 files/s"). A tiny low-pass on the displayed ETA (25% gap-closure per tick) prevents flicker without dampening real changes. The display ETA also resets to `null` on phase transitions to re-warm with the backend.

## Key patterns and gotchas

**All blocking work in `spawn_blocking`.** Never call blocking I/O on the async executor.

**`OperationIntent` state machine.** Replaces the old `cancelled: AtomicBool` + `skip_rollback: AtomicBool` pair with a
single `AtomicU8`-backed enum: `Running → RollingBack` (user clicks Rollback), `Running → Stopped` (user clicks Cancel
or teardown), `RollingBack → Stopped` (user cancels the rollback). `Stopped` is terminal. The `is_cancelled()` helper
returns true for both `RollingBack` and `Stopped`, so the 40+ cancellation check sites just call `is_cancelled(&state.intent)`.

**Cancel vs Rollback: distinct behaviors:**
- **Cancel (`Stopped`)**: Stop immediately. Keep all fully-copied files. Delete only the last *partial* file (a
  half-written file is corrupted data, not useful to keep). `rolled_back: false`.
- **Rollback (`RollingBack`)**: Stop copying, then delete ALL files copied so far in reverse order with progress
  events (`phase: RollingBack`). The progress bars go backwards. User can cancel the rollback (→ `Stopped`), which
  keeps whatever hasn't been deleted yet. `rolled_back: true`.
- Both are triggered from the same `cancel_write_operation` IPC call, distinguished by the `rollback` parameter.

**Two-layer cancellation.** `AtomicU8` (`OperationIntent`) for fast in-loop checks in local file operations. Volume operations (MTP, SMB) use the same `AtomicU8` checks but run on the async executor (no `spawn_blocking`). `run_cancellable` wraps blocking local operations (e.g., network-mount copies that may block indefinitely) in a separate thread, polling the flag every 100ms via `mpsc::channel`.

**`CopyTransaction` rollback: sync with progress.** `rollback()` (synchronous, for error paths) and tracked
`rollback_with_progress()` in `copy.rs` (for user-initiated rollback, emits `write-progress` events with
`phase: RollingBack`, checks for `Stopped` between file deletions so the user can cancel the rollback). Auto-rollback
via `Drop` remains as a panic safety net. Delete operations are not rollbackable.

**Symlinks never dereferenced.** All stat calls use `symlink_metadata`. Symlink loop detection uses a `HashSet<PathBuf>`
of canonicalized paths.

**Safe overwrite: temp + backup + rename.** Steps: copy source → `dest.cmdr-tmp-<uuid>`, rename dest → `dest.cmdr-backup-<uuid>`,
rename temp → dest, delete backup. The original is intact until step 3 completes.

**Conditional conflict policies (`OverwriteSmaller` / `OverwriteOlder`)** reduce per-file. The user picks "Overwrite all smaller" / "Overwrite all older" either upfront (TransferDialog radios) or via the per-file conflict dialog's apply-to-all buttons. Each conflict re-evaluates against its own source/dest metadata: `OverwriteSmaller` overwrites only when `dst.len() < src.len()`, `OverwriteOlder` overwrites only when `dst.modified() < src.modified()`. Equal sizes / equal mtimes / unknown metadata all reduce to `Skip` — strict comparison so a borderline file is never silently overwritten. Implemented by `helpers::reduce_conditional_resolution` (sync, local FS) and `volume_conflict::reduce_volume_conditional_resolution` (async, volume backends). Both log a `target: "conflict_resolution"` info line on every Skip with the reason (not-strictly-smaller, not-strictly-older, missing metadata), so users running an MTP/SMB copy who picked one of these can see in the operation log why their conflicts got skipped instead of being puzzled by silence. **The apply-to-all storage saves the *original* conditional variant**, not the reduced one — subsequent conflicts re-run the comparison against their own files. Tested exhaustively across the comparison axes in `helpers::conditional_resolution_tests` and `volume_conflict::tests::volume_*`.

**Stop-mode conflict resolution.** Emits `write-conflict` event, then blocks on a `tokio::sync::oneshot` channel
(`blocking_recv()` inside `spawn_blocking`). A new oneshot channel is created per conflict. Frontend calls
`resolve_write_conflict(operation_id, resolution, apply_to_all)` which takes the stored `Sender` and sends the
`ConflictResolutionResponse`. `cancel_write_operation` drops the sender, causing the receiver to return `Err` (interpreted
as cancellation). This is strictly better than the old Condvar+timeout approach: no polling, no 30s safety timeout needed,
immediate unblock on cancel.

**`cancel_write_operation` does state transitions.** `rollback=true` → `Running → RollingBack`, `rollback=false` →
`Running → Stopped` or `RollingBack → Stopped`. First caller's decision wins; subsequent calls with different intent
are no-ops (unless transitioning from `RollingBack → Stopped`). `cancel_all_write_operations` always transitions to
`Stopped` (teardown should never silently roll back without visual feedback).

**Scan preview caching.** `start_scan_preview` runs a background scan, caches the result in `SCAN_PREVIEW_RESULTS`. The
actual `copy_files_start` can consume the cache via `preview_id` in `WriteOperationConfig`, skipping a redundant scan.

**Progress throttled to 200ms.** Each operation tracks `last_progress_time` and skips emitting if under the interval.

**Temp files use `.cmdr-` prefix.** Enables recoverability (recognizable leftover files after a crash).

**Move strategy.** Same filesystem detected via device ID comparison (`MetadataExt::dev`). Cross-filesystem move uses a
`.cmdr-staging-<uuid>` dir at the destination root, then atomic `rename` into place, then source deletion.

**Dir-vs-dir conflicts route through `resolve_volume_conflict` like every other shape.** The `volume_move` and `volume_copy` loops used to short-circuit dir-into-dir as a silent merge, bypassing the user's `conflict_resolution`. That made the merge invisible: even when the user picked "Stop" (= ask), nothing prompted. Now every conflict (file-vs-file, dir-vs-dir, file-vs-dir, dir-vs-file) goes through the resolver.

**Overwrite means merge for dirs, replace for files, enforced architecturally, not by trait contract.** `apply_volume_conflict_resolution` stats the dest first; for files it deletes (so the streaming writer lands a fresh copy), for directories it skips the delete entirely (the recursive copy merges into the existing tree). This is enforced at the call site rather than relying on `Volume::delete`'s "file or empty directory only" contract. A future backend with recursive delete semantics, or a refactor that consolidates `delete` + `delete_recursive`, would otherwise silently flip the UX from merge to wholesale replace and delete files unique to dest. The `dir_overwrite_must_merge_not_replace_even_with_recursive_delete` test in `volume_conflict.rs` pins this with a wrapper Volume that violates the trait contract.

**Cross-volume move source-delete is recursive.** `move_between_volumes` in `volume_move.rs` deletes the source via
`delete_volume_path_recursive` (re-exported from `volume_copy.rs` for this purpose) when the source is a directory.
The `Volume::delete` contract is "file or *empty* directory": `LocalPosixVolume::delete` calls `std::fs::remove_dir`
which fails ENOTEMPTY, so deleting a populated source directory after a cross-volume copy must walk the tree.
Regression coverage: `delete_volume_path_recursive_*` tests in `volume_copy.rs`. The original failure mode (data at
both source and dest, FE shows generic `io_error`) traced back to this; the SMB collision that surfaced on retry was
just the second-order symptom.

**Move rollback (same-FS).** `MoveTransaction` in `move_op.rs` tracks `(source, dest)` pairs for each rename. On
cancellation, renames are reversed in reverse order. Same-FS rename rollback is instant (just another rename), so it
runs synchronously. Cross-FS move rollback is handled by `CopyTransaction` (deletes the staging directory).

**Intentional duplication: `merge_move_directory` vs `copy_single_item`.** Both implement recursive merge with conflict
resolution, but differ in every detail: copy has progress tracking, symlink handling, byte counting, strategy selection,
and `CopyTransaction` recording. Move uses simple `fs::rename`. A shared abstraction would be forced and fragile.
Cross-references are in the doc comments of both functions.

**Copy strategy selection** (`copy_strategy.rs`):
- macOS, same APFS volume → `copyfile(3)` with `COPYFILE_CLONE` for instant clonefile
- macOS, everything else → `chunked_copy_with_metadata` (1 MB chunks, cancellation between chunks)
- Linux, network → `chunked_copy_with_metadata`
- Linux, local → `copy_single_file_linux` (`copy_file_range(2)`, supports reflink on btrfs/XFS)
- Other platforms → `std::fs::copy` fallback

**Trash has no scan phase.** `trashItemAtURL` is atomic per top-level item (the OS moves the entire tree), so trash
doesn't need the recursive scan that delete/copy use. Progress tracks top-level items, with optional byte-level progress
from pre-computed item sizes. Partial failure is supported: if some items fail, others still succeed. The core
`move_to_trash_sync()` is extracted to `trash.rs` and reused by `commands/rename.rs`.

**`cancel_all_write_operations` for teardown safety.** A `beforeunload` listener calls this to cancel all active
operations (with rollback) on hot-reload, tab close, window close, or navigation. Prevents orphaned background
operations when the frontend is destroyed.

**Special files skipped.** Sockets, FIFOs, and device files are filtered out during scan.

**Validation runs inside `spawn_blocking`.** The `*_files_start` functions return an `operationId` immediately, before
any filesystem I/O. Validation (`validate_sources`, `validate_destination_writable`, etc.) runs inside the handler
closure on the blocking thread pool. This keeps the Tauri IPC handler non-blocking, so the frontend can always open
the progress dialog and offer cancel, even if a network mount is stalled.

**`start_write_operation` emits `write-error` for handler errors.** The spawn wrapper matches on the handler's
`Result`: `Ok(Ok(()))` and `Ok(Err(Cancelled))` are no-ops (handlers already emitted the right events), `Ok(Err(e))`
emits `write-error` as a safety net, and `Err(join_error)` handles panics. Double-emit is harmless because the
frontend's `handleError` removes all listeners on first receipt.

**Background cleanup is best-effort.** `remove_file_in_background` and `remove_dir_all_in_background` run on detached
threads (used for temp/backup file cleanup, not for user-visible rollback). If the network mount disconnects or the app
exits, partial files or staging directories may remain on disk. These use the `.cmdr-` prefix, so they're recognizable.

**`volume_copy` path is fully wired up.** The three `volume_*` files are re-exported from `mod.rs` and called by the `copy_between_volumes` and `move_between_volumes` Tauri commands. Both copy and move operations support conflict detection and resolution (Stop/Skip/Overwrite/Rename/OverwriteSmaller/OverwriteOlder) for all volume combinations (Local↔MTP, MTP↔MTP). Volume copy supports rollback (delete all copied files in reverse order with progress events, matching the local copy's `rollback_with_progress` pattern) and cancel cleanup (delete only the last partial file). Rollback uses `delete_volume_path_recursive` which lists directory contents via `Volume::list_directory` and deletes children before parents.

**`write-error` carries a provider-enriched `FriendlyError` for both move and copy.** Both `move_between_volumes` and `copy_volumes_with_progress` keep the originating `VolumeError + path` alongside each `?`-propagated `WriteOperationError` via the shared `WriteFailure` struct (in `volume_copy.rs`). `WriteFailure::from_volume(path, e)` and `WriteFailure::synthetic(write_err)` are the two constructors: one captures volume context, the other doesn't (cancellation, validation, synthetic IoError). The shared `write_error_event_from(...)` helper builds a `WriteErrorEvent` from any `WriteFailure`: when the volume context is present it calls `WriteErrorEvent::with_friendly` (full `friendly_error_from_volume_error` + `enrich_with_provider` pipeline, picks up provider-specific suggestions like "This folder is managed by **MacDroid**…"); otherwise it falls back to the variant-derived `WriteErrorEvent::new` via `friendly_from_write_error`. Both move and copy paths land at the same FE quality.

**Volume copy/move must skip `write-error` emit on `Cancelled`.** `copy_volumes_with_progress` / `move_*` inner handlers already emit `write-cancelled` before returning `Err(Cancelled)`, so the outer `copy_between_volumes` / `move_between_volumes` wrapper must match on `WriteOperationError::Cancelled { .. }` and NOT also emit `write-error`, otherwise the frontend logs a user-initiated cancel as an error. This mirrors `mod.rs`'s `Ok(Err(Cancelled)) ⇒ no-op` branch for the generic `start_write_operation` path; the volume paths don't go through `mod.rs`, so they carry their own version of the check. Related: cancellation must propagate as `VolumeError::Cancelled(msg)`, not `VolumeError::IoError { message: "Operation cancelled" }`; the `matches!(WriteOperationError::Cancelled)` check at the outer layer relies on the typed variant. `SmbVolume`'s streaming reader and `map_smb_error`'s `ErrorKind::Cancelled` arm both return `VolumeError::Cancelled` to stay consistent.

## Events emitted

| Event | Trigger |
|-------|---------|
| `write-progress` | Every ~200ms during copy/move/delete/trash |
| `write-conflict` | Stop mode hit a conflicting destination file |
| `write-complete` | Operation finished successfully |
| `write-cancelled` | Operation cancelled (includes `rolled_back` flag) |
| `write-error` | Operation failed. Carries `error: WriteOperationError` (typed) plus `friendly: FriendlyError` (rendered title/explanation/suggestion + category) populated by `WriteErrorEvent::new` via `friendly_from_write_error`. The FE renders the `friendly` payload directly in `TransferErrorDialog` and applies category-based colors. |
| `write-settled` | Emitted once per op after the spawned background task fully returns. See [Settle contract](#settle-contract). |
| `write-source-item-done` | All files for a top-level source item processed (for gradual deselection) |
| `dry-run-complete` | `config.dry_run == true` (returns `DryRunResult`) |
| `scan-preview-progress` | During `start_scan_preview` |
| `scan-preview-complete` | Preview scan finished |
| `scan-preview-error` | Preview scan failed |
| `scan-preview-cancelled` | Preview scan cancelled |

## Settle contract

`write-settled` fires exactly once per operation, after the spawned background task has fully torn down — including
in-flight USB / network teardown that may briefly outlive the `write-cancelled` emit. The FE uses it to gate the
"Cancelling…" dialog close so the user can't dispatch a new op against a still-tearing-down volume (the wedge mode
the M2 cancel propagation already shortens but doesn't eliminate).

**Ordering**: `write-settled` always fires AFTER the terminal outcome event (`write-complete` / `write-cancelled` /
`write-error`) for the same `operation_id`. The BE guarantees this by placing the settle emit in a `WriteSettledGuard`
RAII struct whose `Drop` runs at the very end of the spawn-task scope, AFTER all the conditional terminal-event emits.

**Guard pattern**: every spawn-task entry point (`start_write_operation` in `mod.rs`, the volume-delete branch in
`delete_files_start`, `copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`) constructs a
`WriteSettledGuard` at the top of the spawned task. The guard's `Drop` impl emits the event. This makes the emit
panic-safe: even if the handler closure panics and the task exits via `JoinError`, the guard still drops as part of
stack unwinding, so the FE never hangs waiting for a settle that never comes. See
`settle_event_tests.rs::settled_fires_on_panic_unwind` for the safety-net pin.

**Payload**: `{ operationId: String, operationType, volumeId: Option<String> }`. The `volume_id` is best-effort: filled
with the source volume's display name for volume-aware ops (copy/move between volumes, volume delete), `None` for pure
local-FS operations. The FE currently filters only by `operationId`; `volume_id` is for diagnostics and forward
compatibility.

**Tests**: `settle_event_tests.rs` pins the guard's invariants (single fire, panic safety, ordering relative to the
terminal event). `volume_cancel_tests::volume_*_emits_write_settled_event` pin the integration shape against the
volume-delete handler.

## Key decisions

**Decision**: `walk_dir_recursive` dedupes hardlinks by inode when summing `total_bytes`.
**Why**: A naïve `*total_bytes += metadata.len()` per direntry over-counts on hardlink-heavy trees (cargo `target/`, sccache caches, deduplicated backups). Without dedup, a 49 GB `target/debug` reported 70+ GB to the scan UI, and the "X% of estimated" progress bar (denominator from the indexer's `dir_stats`, which already inode-dedupes) couldn't converge to 100%. Mirrors `indexing/scanner.rs`'s `seen_inodes: HashSet<u64>` pattern, with the same `nlink == 1` fast path. The set is operation-scoped (shared across all source roots in one scan, dropped when the scan ends), so hardlinks crossing source roots still count once. **Unix-only**: `std::fs::Metadata` has no `nlink()` accessor outside Unix; non-Unix falls back to the old naïve sum. Doesn't apply to `dry_run_scan_recursive` (that path reports for conflict counts, not for a progress denominator).

**Decision**: `WriteProgressEvent::with_scan_meta` is the only path that sets the scan-only fields (`current_dir`, `dirs_done`, `expected_files_total`, `expected_bytes_total`).
**Why**: 20+ emit sites construct `WriteProgressEvent` literals for active-phase events. Adding four optional fields to the struct would force every site to spell out their defaults, pure mechanical noise. The `new(...)` constructor takes the eight core counter fields and defaults the scan meta (`None` / `0`); the scan emit sites in `scan.rs`, `scan_preview.rs`, and `delete.rs::scan_volume_recursive` opt in via `.with_scan_meta(current_dir, dirs_done, expected)`. Future scan-related fields go through the same builder. If a real refactor of the 20 literals to `new(...)` ever happens, the builder pattern still composes cleanly on top.

**Decision**: `copy_volumes_with_progress` scan phase calls `scan_for_copy_batch` once instead of `scan_for_copy` per source (Phase 4 Fix 4)
**Why**: Network-backed volumes (SMB) pay 1 RTT per top-level source in the scan phase. Looping over sources made that serial: for 100 tiny files at ~60 ms RTT, ~5 s of pure stat latency before the copy phase started. `scan_for_copy_batch` surfaces both the aggregate (file/dir counts, total bytes) and a per-path vec (is_directory, size) in a single trait call; the copy engine folds the per-path vec into its `source_hints` map and skips the old per-source re-stat. `SmbVolume` overrides `scan_for_copy_batch` to pipeline N stats over one SMB session; measured 6.5× wall-clock win at 100 files (6.11 s → 947 ms) on a Tailscale link. `LocalPosixVolume` / `InMemoryVolume` inherit the default serial per-path loop; it's cheap for them. See `docs/notes/phase4-rtt-investigation.md`.

**Decision**: All write operations except `trash` go through `OperationEventSink` instead of `tauri::AppHandle`
**Why**: Decouples the copy/move/delete orchestration from the Tauri framework. `TauriEventSink` wraps AppHandle for production; `CollectorEventSink` stores events for test assertions. Enables testing the full pipelines end-to-end (multi-file copy, cancellation, conflict resolution, progress tracking) without a Tauri runtime. Every `*_with_progress` function (local copy, local move, local delete, volume copy, volume move, volume delete) takes `&dyn OperationEventSink` or `Arc<dyn OperationEventSink>` and emits via the sink. `trash.rs` is the only write op that still calls `app.emit` directly — trash has no scan phase and no rollback, and the test surface is smaller; folding it in is tracked for a future pass but not blocking.

**Decision**: `drive_transfer_serial_async` bounds its closures as `for<'a> FnMut(...) -> Pin<Box<dyn Future<...> + Send + 'a>>`, not `AsyncFnMut(...) -> T`.
**Why**: Production callers live inside `tokio::spawn(async move { ... })` (see `volume_copy::copy_between_volumes`), so the driver's returned future must be `Send`. `AsyncFnMut`'s HRTB-bound `CallRefFuture<'a>` is not provably `Send` for all `'a` when the closure body captures `&Arc<...>` or similar refs — the compiler emits "implementation of Send is not general enough" because it can't discharge `for<'a> CallRefFuture<'a>: Send` (rust-lang/rust#100013-class). The explicit boxed-future shape moves the Send obligation inside the per-call return type, where it's discharged at each call site, and `+ Send` on the trait object is what makes the driver's awaiting-this-future state Send. The M2 step 0 prototype used `async ||` + `AsyncFnMut` and passed the driver's own `#[tokio::test]`s; the bug only surfaced when M3 migration started wiring real callers and the spawn-boundary Send check ran. `transfer_driver_tests.rs::driver_future_is_send_across_spawn` pins the contract by routing the driver call through an explicit `tokio::spawn` boundary.

**Decision**: `transfer_driver.rs` ships as two sibling entry points (sync + async), not one generic-over-AsyncFnMut-or-FnMut driver. Conflict resolution lives in the driver for the async path, in the closure for the sync path.
**Why**: `copy_files_with_progress_inner` is sync inside `spawn_blocking`; the three volume ops are async. A single generic driver would either force the sync path through a `Pin<Box<dyn Future>>` per source (allocation per call, no real benefit since the I/O is sync) or use a trait so gnarly that the closures stop reading as straight-line transfer code. Two siblings share `TransferContext`, `TransferOutcome`, `TransferLoopOutcome`, and `build_pre_skip_set` / `emit_progress_and_status` helpers — the duplication is small. For conflict resolution: local-FS conflicts surface mid-flight at parent directories inside `copy_single_item` (a file blocking `create_dir_all`), which the driver can't pre-detect via top-level `dest.get_metadata`; so the sync driver delegates conflict resolution to the closure entirely. Volume ops have only top-level conflicts that always reduce to `resolve_volume_conflict`, so the async driver owns that dispatch (uniform shape across all 3 volume ops, exactly what we want to deduplicate). The data-safety contract (closure never invoked for pre-skipped / resolved-as-Skip / post-cancel) is enforced in both shapes by the driver's loop structure and pinned by `transfer_driver_tests.rs`. See `docs/specs/transfer-driver-refactor-plan.md` § "Design decisions" and § "Concurrent driver scope" for the full rationale; the concurrent path stays inline in `copy_volumes_with_progress` (1-of-4 abstraction not worth its weight).

**Decision**: `copy_files_with_progress_inner` aligns `scan_result.files` to the driver's `&[PathBuf]` API via a paired `Vec<&FileInfo>` and a closure-captured `slice::Iter` advanced in lock-step with the driver iteration.
**Why**: The sync driver iterates a generic `&[PathBuf]`, but the local-FS copy loop needs the full `FileInfo` (for `dest_path`, `is_symlink`, `size`, and the `SourceItemTracker` key). Three alternatives were rejected: (a) indexing into `scan_result.files` by `ctx.files_done_so_far` — wrong, the cumulative counter is bytes-affecting and includes bulk-skipped files, so the index would shift; (b) extending `TransferContext` with a generic associated payload — couples the driver to local-FS specifics; (c) cloning the `FileInfo` slice for `sources` — copies on the hot path. The iterator approach is O(0) memory beyond the path vec and matches the driver's iteration order exactly (`pre_skip_paths` is empty because we pre-filter `scan_result.files` ourselves, so the driver invokes the closure once per surviving file). The `.expect()` is justified inline; if the driver ever stopped calling the closure once per source the test suite would break.

**Decision**: Scan preview reuses watched listings (the "fresh-listing oracle").
**Why**: Pre-flight scans for copy/move on MTP (and to a lesser degree SMB and big local trees) used to duplicate work the backend already had in `LISTING_CACHE`. Selecting 135 photos in a watched `/DCIM/Camera` (~15k entries) and pressing F5 would re-list the parent dir over USB just to look up size by name — ~17 s of "Verifying before copy…" while the listing was already fresh on the pane behind the dialog. `run_volume_scan_preview` now groups input sources by parent dir and consults `try_get_watched_listing(volume_id, parent)` first. On hit, sizes and `is_directory` flags come from the cached `FileEntry` for top-level files; top-level directories recurse via `scan_subtree_with_oracle`, which re-applies the oracle at every level (so a subfolder open in another pane also short-circuits). On miss, the call falls through to `volume.scan_for_copy_batch_with_progress(paths_in_group, ...)` — same code path as before — so MTP's parent-grouping and SMB's pipelined-stat optimizations still run for cold-cache parents. The local-FS walker (`walk_dir_recursive` in `scan.rs`) also takes an oracle check at the top of each recursive call, with `volume_id = "root"` plumbed through from `scan_sources_internal` and `run_scan_preview`. The freshness contract is bright-line at the watcher boundary: no "5 seconds is fresh enough" TTL, just "the volume's `listing_is_watched(path)` returned true." See `file_system/listing/caching.rs::try_get_watched_listing` for the per-backend debounce windows that contract tolerates.

**Decision**: `delete_files_start` routes to either `delete_files_with_progress` (local, uses `walkdir` + `fs::remove_file`) or `delete_volume_files_with_progress` (non-local, uses `Volume` trait) based on `volume_id`.
**Why**: MTP volumes can't use `walkdir` or `fs::remove_*`. Rather than refactoring the existing local delete to go through the Volume trait (which would add overhead for local ops), we keep the fast local path and add a parallel volume-aware path. Both emit identical events so the frontend progress dialog works unchanged.

**Decision**: Volume delete reuses the scan preview and is oracle-aware on the no-preview path.
**Why**: Before this, `delete_volume_files_with_progress_inner` ignored `config.preview_id` entirely and ran `scan_volume_recursive` again. On MTP that meant a second 17 s parent listing for a 135-photo `/DCIM/Camera` delete after the user had just paid that cost in the pre-flight dialog — and the second scan emitted no per-top-level-file progress, so the UI looked frozen. The fix has three parts. (1) `delete_volume_files_with_progress_inner` calls `take_cached_scan_result(preview_id)` at the top; on hit, top-level files are recorded from `CopyScanResult::total_bytes` with no `is_directory` probe and no `list_directory` round-trip, and top-level dirs recurse via the oracle-aware `scan_volume_recursive` (passing `is_dir_hint = Some(true)` so the recursion never re-probes). (2) The walker's internal `volume.list_directory(path, ...)` is now preceded by `try_get_watched_listing(volume_id, path)`; on hit, the cached entries replace the volume call entirely at every recursion level. (3) On the no-preview path (MCP triggers, programmatic deletes), the top-level `volume.is_directory(source)` probe stays only when the parent oracle misses — when a pane has the source's parent open and watcher-fresh, the type comes from the cached `FileEntry` and the probe is skipped. The cache-hit path also emits a throttled scan-progress event per `progress_interval` while building the entry list, so the FE dialog shows movement during the fast path instead of waiting for the delete phase to start. Pinned by `delete_volume_reuse_tests.rs`. Data-safety contract: stale-by-one cached entries can either silently skip a now-gone file (acceptable: the user already moved it elsewhere) or attempt to delete a missing one (the volume's `delete` errors cleanly). Neither direction can delete the wrong file because we feed `volume.delete(&entry.path)` exact paths the cache observed; a cached entry that races with a concurrent rename ends up addressing the old path the next call won't find.

**Decision**: Volume move runs the same preflight scan as volume copy (extracted to `volume_preflight.rs`).
**Why**: Both `move_volumes_with_progress` (cross-volume copy+delete) and `move_within_same_volume_with_progress` (rename) used to skip the scan phase entirely, sending `bytes_total = 0` on every progress event. The FE's `TransferProgressDialog` hides the Size progress bar behind `{#if bytesTotal > 0}`, so during an MTP→local move the user saw only the Files bar with no size feedback — even though `bytes_done` was being tracked correctly. The fix shares one helper (`scan_volume_sources`) between copy and move: reuses a cached `TransferDialog` preview when available (free in the common dialog-driven path), falls through to `volume.scan_for_copy_batch` otherwise. Move now gets the same `(total_files, total_bytes, source_hints)` triple copy has, so progress events carry the real `bytes_total`, the per-source `is_directory` probe inside the move loop is gone (hint comes from the scan), and the same-volume rename's per-iter `get_metadata` for size is gone (hint again). The previous `collect_known_directory_paths` helper (file-only-bulk-skip via per-source `get_metadata`) is replaced by `VolumePreflight::known_directory_paths()`. Behavior change to flag: programmatic moves with no `preview_id` (MCP, etc.) now pay one batch scan up front; for MTP this is ~one parent listing's RTT. Same cost copy has always paid; consistent across both ops. Pinned by `volume_move::tests::*_emits_bytes_total_on_progress`.

**Decision**: Keep `exacl` crate for ACL copy in chunked copies (not custom FFI bindings).
**Why**: `exacl` adds zero new transitive dependencies (all of its deps, `bitflags`, `log`, `scopeguard`, `uuid`, are already in our tree). It provides cross-platform ACL support (macOS, Linux, FreeBSD) and full ACL parsing/manipulation for potential future UI features. The crate appears unmaintained (last release Feb 2024) but ACL APIs are stable and don't change. Our usage is best-effort with graceful fallback: if `exacl` ever breaks, files still copy, they just lose ACLs. MIT licensed (compatible with BSL).

**Decision**: On macOS, use `copyfile(3)` only for same-APFS-volume copies; use chunked copy for everything else.
**Why**: The only practical benefit of `copyfile(3)` is APFS clonefile (instant copy-on-write, zero extra disk usage),
which only works on the same APFS volume. We evaluated `copyfile` on other filesystems:
- **HFS+**: No clonefile. Marginal metadata edge (birthtime, file flags), but HFS+ is rare since Apple converted all
  Macs to APFS in 2017.
- **exFAT / FAT32**: No clonefile, no xattrs, no ACLs, no file flags; the metadata `copyfile` would preserve doesn't
  exist on these filesystems. No practical benefit.
- **NTFS-3G**: FUSE-based, so `copyfile` goes through userspace with the same I/O buffering issues as network mounts.
  `COPYFILE_QUIT` is unreliable. No benefit.
- **Network mounts (SMB, NFS, AFP, WebDAV)**: `copyfile` ignores `COPYFILE_QUIT` while draining buffered I/O, causing
  cancellation to take 30+ seconds or complete the copy entirely. This applies when *either* the source or destination
  is on a network mount (for example, NAS-to-local copies).
- **USB / external drives**: Typically exFAT or HFS+, no clonefile. Different volume from the internal drive, so no
  same-volume benefits.

Our chunked copy (1 MB read/write chunks) provides: identical speed for non-clonefile copies, reliable cancellation
between chunks, and granular progress callbacks. It preserves xattrs (including resource forks), ACLs, timestamps, and
permissions. The only metadata it doesn't preserve is birthtime (creation date) and file flags (`chflags`), which
matter only on same-volume copies where we use `copyfile` anyway. Detection uses `st_dev` (device ID) for same-volume
and `statfs.f_fstypename` for APFS. See `copy_strategy.rs` for the implementation.

## Gotchas

**Gotcha**: On macOS, never use `statvfs` alone for disk space checks; use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks. On APFS, purgeable space (iCloud caches, APFS snapshots) can account for tens of GB that macOS will reclaim on demand. Using `statvfs` causes the "insufficient space" error to reject copies that would actually succeed, and shows a different available-space number than the status bar (which uses the NSURL API). `validate_disk_space` in `helpers.rs` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

**Gotcha**: Hardlink dedup doesn't straddle the oracle/walk boundary.
**Why**: `walk_dir_recursive` dedupes hardlinks by inode for `total_bytes` (so a hardlink-heavy tree like cargo's `target/` reports correct bytes-to-free). `FileEntry` doesn't carry inode, so when the oracle supplies one half of a hardlink pair from the cached listing and a real walk supplies the other half, the dedup misses and bytes get over-counted. Direction is safe: over-counting → pessimistic ETA + conservative disk-space reject, never the other way. The walker's existing `walker_dedupes_hardlinks_by_inode` test still pins the same-side dedup. If true cross-boundary dedup ever becomes worth it, add `inode: Option<u64>` to `FileEntry`; not in this milestone.

**Gotcha**: Volume disconnect mid-walk races with the oracle.
**Why**: The oracle returns `Some(entries)` when `listing_is_watched` is true at the moment of the check. Between that read and the recursive walk consuming the entries (and then issuing real `list_directory` calls for any sub-subfolders that aren't cached), the watcher can die (cable yanked, network drop). The synthesized totals for the cached level are correct — they reflect what the listing held — but recursion into now-disconnected sub-subfolders fails per-call, and the per-file copy/delete later then hits `DeviceDisconnected`-shaped errors instead of a single "device gone" message at the scan level. Same race that `scan_for_copy_batch` already had; the oracle doesn't widen it. Documented here so future investigation knows where to look.

**Gotcha**: Recursive scan helpers that bail with `Err(Cancelled)` must NOT emit `write-cancelled` themselves; their top-level callers must.
**Why**: `delete::scan_volume_recursive` checks `is_cancelled(&state.intent)` at the top of every recursion level. If it emitted `write-cancelled` at the bail site, a mid-walk cancel would fire the terminal event once per recursion frame still on the stack. So the function returns `Err(Cancelled)` silently and the caller is responsible for emitting before propagating. `delete_volume_files_with_progress_inner` uses the `emit_cancelled_if_aborted` helper at each of its three `scan_volume_recursive(...).await?` sites for exactly this. Any future recursive scan that follows the same shape (per-level cancel check) needs the same caller-side emit, otherwise the FE never sees `write-cancelled` for scan-phase cancels and the dialog closes via the M4 settle-fallback path instead of the proper cancel flow. Pinned by `delete_cancel_during_scan_emits_write_cancelled` in `delete_volume_reuse_tests.rs`.

**Gotcha**: Skip-All on volume copy/move with a top-level dir conflict still skips the entire dir subtree, even after the local-FS bulk-skip fix.
**Why**: `build_pre_skip_set` now excludes top-level directories so non-conflicting children inside a conflicting dir get a chance to copy. For LOCAL copy this works because `copy_files_with_progress_inner` flattens dirs to per-file `FileInfo` entries pre-loop, and per-iter conflict resolution then evaluates each child individually. For VOLUME copy/move, the driver iterates top-level paths directly, and `resolve_volume_conflict` returns `Ok(None)` (= Skip) for ANY dir-vs-dir conflict under Skip mode without recursing — so the whole subtree is still dropped. Fixing this requires teaching `resolve_volume_conflict` (or the volume-side closure) to recurse-and-merge for dir conflicts under Skip, the same way `apply_volume_conflict_resolution` already does for Overwrite. Pinned by the Playwright spec `conflict-copy.spec.ts::Copy with Skip All preserves destination files` (local-FS path, currently green) — a volume-side equivalent test would catch the residual.

## Dependencies

- `crate::file_system::volume`: `Volume` trait, `SpaceInfo`, `ScanConflict` (used by `volume_copy`)
- `crate::ignore_poison`: `IgnorePoison` extension for `RwLock`/`Mutex` to not panic on poisoned locks
- External: `tauri` (emit, AppHandle), `uuid` (operation IDs, temp names), `libc` (access, statvfs, sync), `xattr`, `exacl`, `filetime` (metadata preservation in `chunked_copy`)

## Testing bar

This module's state machine (`state.rs`) is the spine of the cancel UX. Past investigations found one real production
bug here ([commit `1de4255d`](../../../../../../docs/notes/speed-up-e2e-tests.md), lost-rollback on `Ok(())` arm) plus
30+ mutation-testing gaps that have since been pinned. New transitions or new cancel paths must:

1. **Drive the state machine through the public interface in tests.** Direct `state.intent.store(...)` mutation bypasses
   the validation guard and effectively dead-tests it. Pattern to copy: `state.rs::tests::test_cancel_via_public_path`.
2. **Cover both the happy path and the cancel-during-X race** for any new write-side operation. The Cancel-copy bug
   was specifically the `Ok(())` arm of the loop not re-checking intent.
3. **Add at least one E2E test** for user-visible flows (transfer dialogs, conflict policies); use
   `dispatchMenuCommand` for keyboard-shortcut triggers, see `docs/testing.md` § "❌ Synthesized F-key dispatches".
4. **Run `cargo mutants --file src/file_system/write_operations/<file>.rs`** after substantial changes; this module has
   ~85-90% mutation score per file and shouldn't regress. See `docs/testing.md` § "Process".

See also: [docs/testing.md](../../../../../../docs/testing.md) for the project-wide testing playbook.
