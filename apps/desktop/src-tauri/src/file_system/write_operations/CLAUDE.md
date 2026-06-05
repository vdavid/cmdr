# Write operations

Copy, move, delete, and trash with streaming progress, cancellation, conflict resolution, and rollback. macOS and Linux.

Frontend counterpart: [`apps/desktop/src/lib/file-operations/CLAUDE.md`](../../../../src/lib/file-operations/CLAUDE.md)
(umbrella) plus colocated child docs for [`transfer/`](../../../../src/lib/file-operations/transfer/CLAUDE.md),
[`delete/`](../../../../src/lib/file-operations/delete/CLAUDE.md),
[`mkdir/`](../../../../src/lib/file-operations/mkdir/CLAUDE.md), and
[`mkfile/`](../../../../src/lib/file-operations/mkfile/CLAUDE.md).

Subdirs:
- [`transfer/CLAUDE.md`](transfer/CLAUDE.md) — copy + move (local FS, cross-volume, MTP, SMB), conflict resolution, the shared transfer driver, platform-specific copy backends.
- [`delete/CLAUDE.md`](delete/CLAUDE.md) — delete walker (local + volume-aware), trash, the oracle-aware delete fast path.

This file documents the cross-cutting machinery that both subdirs share: the `OperationIntent` state machine, the `WriteOperationState` cache, the `OperationEventSink` trait, scan + scan-preview, the `EtaEstimator`, and the settle contract.

## Purpose

Implements the four destructive file operations as background tasks that stream Tauri events to the frontend. Every operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites, network mounts, cross-filesystem moves, and name/path length limits.

Pre-flight scans reuse cached listings when the source volume reports an active watcher, avoiding redundant `list_directory` calls. The freshness contract and per-backend debounce windows are documented in `../volume/CLAUDE.md` and `../listing/caching.rs::try_get_watched_listing`.

## Files (top level)

| File | Responsibility |
|------|----------------|
| `mod.rs` | Public API: `copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start`. Each delegates to `start_write_operation` which handles state creation, spawn lifecycle, cleanup, and error/panic recovery. Validation runs inside the handler closure on the blocking thread pool, never on the async executor. Also re-exports `transfer::*` and `delete::*` public symbols so external callers keep their `crate::file_system::write_operations::<symbol>` import paths. |
| `types.rs` | All serializable types: events, config, errors, results. `WriteOperationConfig`, `ConflictResolution`, `WriteOperationError`, `DryRunResult`, scan preview events. Also: `OperationEventSink` trait (decouples event emission from `tauri::AppHandle`), `TauriEventSink` (production), `CollectorEventSink` (test-only). |
| `state.rs` | Two `LazyLock<RwLock<HashMap>>` caches (`WRITE_OPERATION_STATE`, `OPERATION_STATUS_CACHE`). `WriteOperationState`, `CopyTransaction`, `ScanResult`, `FileInfo`. `WriteSettledGuard` RAII shape for the settle contract. |
| `validation.rs` | Source/destination validation: `validate_sources`, `validate_destination`, `validate_destination_writable` (via `libc::access`), `validate_disk_space` (NSURL API on macOS, `statvfs` on Linux), `validate_not_same_location`, `validate_destination_not_inside_source`, `validate_path_length`. Identity/filesystem checks: `is_same_file` (inode+device), `is_same_filesystem` (device IDs), `path_exists_or_is_symlink` (dangling-symlink-aware), `is_symlink_loop`. |
| `conflict.rs` | Conflict resolution. The two-bucket `ApplyToAll` latch model (`apply_to_all_effective` / `apply_to_all_record`). `resolve_conflict` (`tokio::sync::oneshot` channel wait for Stop mode), `reduce_conditional_resolution`, `apply_resolution`, `find_unique_name` (O_EXCL reservation). Conflict-event/info builders: `build_conflict_event`, `calculate_dest_path`, `create_conflict_info`, `sample_conflicts`. |
| `overwrite.rs` | Temp+rename-aside atomicity: `ResolvedDestination`, `safe_overwrite_file`, `safe_overwrite_dir`. |
| `durability.rs` | `flush_created_destinations` (emits the `Flushing` event, then `fdatasync`s each created destination + parent dir, skipping already-synced paths). `lookup_indexed_size` (drive-index directory size for conflict UI). |
| `cancellable.rs` | Cancellation-aware execution: `run_cancellable`, `run_cancellable_scoped` (poll the cancel flag while blocking work runs on a separate thread). Detached background cleanup: `remove_file_in_background`, `remove_dir_all_in_background`. |
| `scan.rs` | `scan_sources` (recursive walk, emits progress), `dry_run_scan`, shared `walk_dir_recursive` walker. The `on_progress` callback receives `(files, dirs, bytes, current_file, current_dir)`; the walker reads `current_dir` from `path.parent()` so the UI can show "in directory: …" alongside the filename. Scan emit sites populate `WriteProgressEvent.current_dir` plus index-derived `expected_files_total` / `expected_bytes_total` (via `WriteProgressEvent::with_scan_meta`) so the frontend renders a real progress bar during the foolproof re-scan. Expected totals come from `crate::indexing::expected_totals::expected_totals_for_sources` (`None` when the index doesn't cover all sources; the FE falls back to a tally-only display). |
| `scan_preview.rs` | Scan preview subsystem for Copy dialog live stats: `start_scan_preview`, `cancel_scan_preview`, `is_scan_preview_complete`. Background scans (local and volume-based) with result caching. Emits `expected_files_total` / `expected_bytes_total` (sampled once at scan start from the drive index) on every `scan-preview-progress` event, alongside the running tallies and `current_dir`. |
| `eta.rs` | `EtaEstimator`: time-weighted EWMA per axis (bytes, files), τ ≈ 3 s. Combines via `max(ETA_bytes, ETA_files)`. One per `WriteOperationState`, fed by `state.enrich_progress` at every `write-progress` emit site. See [ETA + throughput](#eta--throughput) below. |
| `tests.rs` | Cross-cutting unit tests. |
| `scan_preview_listing_progress_tests.rs` | Regression tests for the `ListingProgress` callback shape. |
| `scan_preview_oracle_tests.rs` | Integration tests for the fresh-listing oracle inside scan preview. |
| `settle_event_tests.rs` | Tests for `WriteSettledGuard` invariants (single fire, panic safety, ordering). |
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
          → success (copy/move): flush_created_destinations() → emit write-progress (phase: flushing) → fdatasync dests → CopyTransaction::commit(), emit write-complete
          → success (delete/trash): emit write-complete (no sync)
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

**Wiring:** every `write-progress` emit site calls `state.emit_progress_via_sink(events, event)`. Production wraps a Tauri AppHandle in `TauriEventSink`; tests use `CollectorEventSink`. `emit_progress_via_sink` calls `enrich_progress` internally, so no caller has to remember. The `bytes_per_second: None, files_per_second: None, eta_seconds: None` placeholders in the struct literals get overwritten before the event reaches the FE.

**Frontend display:** `TransferProgressDialog.svelte` stores the three fields in local `$state` and renders both speeds side by side ("27.7 MB/s · 1,234 files/s"). A tiny low-pass on the displayed ETA (25% gap-closure per tick) prevents flicker without dampening real changes. The display ETA also resets to `null` on phase transitions to re-warm with the backend.

## Key patterns and gotchas (shared)

**All blocking work in `spawn_blocking`.** Never call blocking I/O on the async executor.

**`OperationIntent` state machine.** Replaces the old `cancelled: AtomicBool` + `skip_rollback: AtomicBool` pair with a single `AtomicU8`-backed enum: `Running → RollingBack` (user clicks Rollback), `Running → Stopped` (user clicks Cancel or teardown), `RollingBack → Stopped` (user cancels the rollback). `Stopped` is terminal. The `is_cancelled()` helper returns true for both `RollingBack` and `Stopped`, so the 40+ cancellation check sites just call `is_cancelled(&state.intent)`.

**Cancel vs Rollback: distinct behaviors:**
- **Cancel (`Stopped`)**: Stop immediately. Keep all fully-copied files. Delete only the last *partial* file (a half-written file is corrupted data, not useful to keep). `rolled_back: false`.
- **Rollback (`RollingBack`)**: Stop copying, then delete ALL files copied so far in reverse order with progress events (`phase: RollingBack`). The progress bars go backwards. User can cancel the rollback (→ `Stopped`), which keeps whatever hasn't been deleted yet. `rolled_back: true`.
- Both are triggered from the same `cancel_write_operation` IPC call, distinguished by the `rollback` parameter.

**Two-layer cancellation.** `AtomicU8` (`OperationIntent`) for fast in-loop checks in local file operations. Volume operations (MTP, SMB) use the same `AtomicU8` checks but run on the async executor (no `spawn_blocking`). `run_cancellable` wraps blocking local operations (for example, network-mount copies that may block indefinitely) in a separate thread, polling the flag every 100 ms via `mpsc::channel`.

**Stop-mode conflict resolution.** Creates a per-conflict `tokio::sync::oneshot` channel, **stores the sender BEFORE emitting the `write-conflict` event**, then blocks on the receiver (`blocking_recv()` inside `spawn_blocking`; the volume path `await`s instead). Store-before-emit is load-bearing: a responder can only answer a conflict it has observed, so if the event reached `resolve_write_conflict` (or a test responder sink) before the sender slot was filled, the take would miss and the recv would hang. Both the local-FS branch (`conflict.rs`) and the volume branch (`transfer/volume_conflict.rs`) order it this way. Frontend calls `resolve_write_conflict(operation_id, resolution, apply_to_all)` which takes the stored `Sender` and sends the `ConflictResolutionResponse`. `cancel_write_operation` drops the sender, causing the receiver to return `Err` (interpreted as cancellation). This is strictly better than the old Condvar+timeout approach: no polling, no 30 s safety timeout needed, immediate unblock on cancel. Pinned by `conflict.rs::stop_branch_store_before_emit_tests` (local) and the `ConflictResponderSink` suites (volume).

**Conflict-dispatch mutex (folder merges).** `WriteOperationState::conflict_dispatch_lock` (a `tokio::sync::Mutex`, next to `conflict_resolution_tx`) serializes the whole Stop-mode dispatch for an operation: there is exactly one human and one oneshot slot, so two tasks both hitting a Stop-mode clash at once — the concurrent volume-copy spawn loop, or two parallel deep directory merges — must queue rather than race to emit a `write-conflict` and clobber each other's sender. The dispatch sequence under the lock: check `is_cancelled` (bail with `Cancelled` so a queued task can't emit a prompt no one will answer after the dialog tears down — a hang), re-check the apply-to-all latch (a prior "…all" answer collapses the queued prompt), emit + await, store the latch, release. Released on every exit, NEVER held across the subsequent file write. Volume-side only today (the local-FS engine's per-file conflicts surface serially inside one `spawn_blocking`). See `transfer/CLAUDE.md` § "The conflict-dispatch mutex".

**`cancel_write_operation` does state transitions.** `rollback=true` → `Running → RollingBack`, `rollback=false` → `Running → Stopped` or `RollingBack → Stopped`. First caller's decision wins; subsequent calls with different intent are no-ops (unless transitioning from `RollingBack → Stopped`). `cancel_all_write_operations` always transitions to `Stopped` (teardown should never silently roll back without visual feedback).

**Scan preview caching.** `start_scan_preview` runs a background scan, caches the result in `SCAN_PREVIEW_RESULTS`. The actual `copy_files_start` / `delete_files_start` can consume the cache via `preview_id` in `WriteOperationConfig`, skipping a redundant scan. The cache is freed by three paths: (1) `take_cached_scan_result(preview_id)` at op start (the consume path), (2) `cancel_scan_preview(preview_id)` on dialog teardown — it now evicts the cached result in addition to setting the in-flight cancel flag, so a dialog dismissed AFTER the scan completed (the FE calls it regardless of `isScanning`) doesn't leak the `CachedScanResult`, and (3) a TTL safety net: every insert goes through `insert_scan_result`, which first evicts entries older than `SCAN_RESULT_TTL` (5 min, keyed on `CachedScanResult::inserted_at`). The TTL is a backstop for a future caller that forgets both (1) and (2); the pure `expired_scan_result_ids` helper is unit-tested. A `CachedScanResult` can hold tens of thousands of `FileInfo`, so none of these paths is optional.

**Progress throttled to 200 ms.** Each operation tracks `last_progress_time` and skips emitting if under the interval.

**Temp files use `.cmdr-` prefix.** Enables recoverability (recognizable leftover files after a crash).

**Symlinks never dereferenced.** All stat calls use `symlink_metadata`. Symlink loop detection uses a `HashSet<PathBuf>` of canonicalized paths.

**Safe overwrite: temp + rename-aside + rename.** Steps: copy source → `dest.cmdr-tmp-<uuid>`, rename dest → `dest.cmdr-temp-<uuid>` (aside), rename temp → dest, delete aside. The original is intact until step 3 completes. The same pattern covers file→folder overwrites (existing dest folder is renamed aside, then the source file lands at the original path) and folder→file overwrites (via `safe_overwrite_dir`: existing file is renamed aside, the folder is materialized in place by the caller's closure, then the aside is deleted; on materialize error or cancel, the aside is rolled back).

**Conditional conflict policies (`OverwriteSmaller` / `OverwriteOlder`)** reduce per-file. The user picks "Overwrite all smaller" / "Overwrite all older" either upfront (TransferDialog radios) or via the per-file conflict dialog's apply-to-all buttons. Each conflict re-evaluates against its own source/dest metadata: `OverwriteSmaller` overwrites only when `dst.len() < src.len()`, `OverwriteOlder` overwrites only when `dst.modified() < src.modified()`. Equal sizes / equal mtimes / unknown metadata all reduce to `Skip` — strict comparison so a borderline file is never silently overwritten. Implemented by `conflict::reduce_conditional_resolution` (sync, local FS) and `transfer/volume_conflict.rs::reduce_volume_conditional_resolution` (async, volume backends). Both log a `target: "conflict_resolution"` info line on every Skip with the reason (not-strictly-smaller, not-strictly-older, missing metadata), so users running an MTP/SMB copy who picked one of these can see in the operation log why their conflicts got skipped instead of being puzzled by silence. **The apply-to-all storage saves the *original* conditional variant**, not the reduced one — subsequent conflicts re-run the comparison against their own files.

**Validation runs inside `spawn_blocking`.** The `*_files_start` functions return an `operationId` immediately, before any filesystem I/O. Validation (`validate_sources`, `validate_destination_writable`, etc.) runs inside the handler closure on the blocking thread pool. This keeps the Tauri IPC handler non-blocking, so the frontend can always open the progress dialog and offer cancel, even if a network mount is stalled.

**`start_write_operation` emits `write-error` for handler errors.** The spawn wrapper matches on the handler's `Result`: `Ok(Ok(()))` and `Ok(Err(Cancelled))` are no-ops (handlers already emitted the right events), `Ok(Err(e))` emits `write-error` as a safety net, and `Err(join_error)` handles panics. Double-emit is harmless because the frontend's `handleError` removes all listeners on first receipt.

**`cancel_all_write_operations` for teardown safety.** A `beforeunload` listener calls this to cancel all active operations (with rollback) on hot-reload, tab close, window close, or navigation. Prevents orphaned background operations when the frontend is destroyed.

**Special files skipped.** Sockets, FIFOs, and device files are filtered out during scan.

## Cmdr-own-write hook (downloads watcher)

Every write-op driver MUST register its destination with the downloads watcher's ignore set BEFORE issuing the syscall. This is what makes the watcher silently suppress events Cmdr itself caused, so the user doesn't see a "Downloaded foo.bin" toast when they just used Cmdr to copy 100 files into `~/Downloads`.

**Contract:** call `crate::downloads::note_pending_write_for_cmdr(&dest_path)` immediately before the write syscall (or the volume-trait equivalent: `Volume::write_from_stream`, `Volume::create_file`, `Volume::create_directory`, `Volume::rename`, `Volume::delete`). For batches with a known full destination list up front, `note_pending_writes_for_cmdr(paths)` saves N-1 mutex acquires.

**Locked-in scoping:** the prefix check lives INSIDE the helper (and the underlying `IgnoreSet::note_pending`). Call sites invoke unconditionally; paths outside the resolved Downloads root silently no-op. **Don't add `if path.starts_with(downloads_dir)` guards at call sites** — see [`docs/specs/downloads-watcher-plan.md`](../../../../../docs/specs/downloads-watcher-plan.md) § "Cmdr-own-write ignore set" for the rationale.

**No-op when the watcher is dormant.** If the FDA gate is closed (or `refresh_runtime` hasn't been called yet), the watcher isn't installed and the helper is a cheap no-op (single mutex `lock + is_none`). Production write ops fire freely; the cost is one atomic-bool read per write.

**Renames register both halves.** A rename moves a file out of one location into another. The Cmdr-own-write contract requires registering both the source path (so a rename-OUT-of-Downloads is also suppressed via the watcher's rename-from-ignored-source branch) and the destination path (so the rename-arrival event is suppressed). See `commands/rename.rs::rename_file` and `transfer/move_op.rs` for the pattern.

**Cross-volume writes that land on a local FS** (MTP→Local, SMB→Local) hook via the local helper inside `transfer/volume_strategy.rs::note_pending_for_local_dest` and `transfer/volume_move.rs::note_pending_for_local_volume`. They check `dest_volume.local_path()` first and skip when the destination isn't a local-FS-backed volume (MTP/SMB/InMemory) — those paths can't trigger the watcher anyway.

Example placement:

```rust
// In `copy_single_item` (transfer/copy.rs), just before `copy_file_with_strategy`:
crate::downloads::note_pending_write_for_cmdr(&actual_dest);
let bytes = copy_file_with_strategy(source, &actual_dest, ..)?;
```

See also: [`apps/desktop/src-tauri/src/downloads/CLAUDE.md`](../../downloads/CLAUDE.md) for the watcher architecture, ignore-set internals, and the FDA-gated lifecycle. End-to-end safety net for the contract lives in `downloads::runtime::tests::note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end`.

## Events emitted

| Event | Trigger |
|-------|---------|
| `write-progress` | Every ~200 ms during copy/move/delete/trash |
| `write-conflict` | Stop mode hit a conflicting destination file |
| `write-complete` | Operation finished successfully |
| `write-cancelled` | Operation cancelled (includes `rolled_back` flag) |
| `write-error` | Operation failed. Carries `error: WriteOperationError` (typed) plus `friendly: FriendlyError` (rendered title/explanation/suggestion + category) populated by `WriteErrorEvent::new` via `friendly_from_write_error`. The FE renders the `friendly` payload directly in `TransferErrorDialog` and applies category-based colors. |
| `write-settled` | Emitted once per op after the spawned background task fully returns. See [Settle contract](#settle-contract). |
| `volumes-busy-changed` | The set of volume IDs with an in-flight op changed (an op started or finished). Payload is `string[]`. See [Busy-volumes set](#busy-volumes-set). |
| `write-source-item-done` | All files for a top-level source item processed (for gradual deselection) |
| `dry-run-complete` | `config.dry_run == true` (returns `DryRunResult`) |
| `scan-preview-progress` | During `start_scan_preview` |
| `scan-preview-complete` | Preview scan finished |
| `scan-preview-error` | Preview scan failed |
| `scan-preview-cancelled` | Preview scan cancelled |

## Busy-volumes set

Drives "disable Eject while an op reads from / writes to this device" so a disconnect can't truncate an in-flight file. Lives in `state.rs`.

- Each op records the volume IDs it touches via `register_operation_status(op_id, type, volume_ids)`. Source **and** destination go in (a download from a phone is as corruptible as an upload to it). `OperationStateGuard`'s `Drop` clears them on unregister, so a panicking op can't leave a volume stuck busy.
- The busy set is the union of every active op's `volume_ids`, minus `root` (never ejectable). `recompute_and_emit_busy_volumes` fires `volumes-busy-changed` only when membership changes — progress ticks don't churn it. Membership-by-union means two concurrent transfers to one device keep it busy until both finish, with no manual refcount.
- **Where `volume_ids` come from**: the cross-volume transfer entry points (`copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`) and the volume-aware `delete_files_start` carry the IDs; `copy_files_start` / `move_files_start` take a `volume_ids` param so the both-local branch of `copy_between_volumes` (which is how a local→USB / DMG copy lands) still marks the ejectable destination. The plain `copy_files` / `move_files` / `trash` commands pass `vec![]` — the unified transfer dialog only routes through them for same-`root` ops, where no ejectable volume is involved.
- **Consumers**: `busy_volume_ids()` backs the `get_busy_volume_ids` bootstrap command, the `eject_volume` server-side guard (refuses a busy volume — the real safety net, since the picker's disable is only UX), and the native breadcrumb-menu builder (renders the Eject item disabled with a ` (busy)` suffix). The frontend `volume-busy-store.svelte.ts` subscribes to `volumes-busy-changed` and exposes `isVolumeBusy(id)` to disable the picker's eject controls. `init_busy_volume_emitter(app)` wires the emitter at startup (`lib.rs`).

## Settle contract

`write-settled` fires exactly once per operation, after the spawned background task has fully torn down — including in-flight USB / network teardown that may briefly outlive the `write-cancelled` emit. The FE uses it to gate the "Cancelling…" dialog close so the user can't dispatch a new op against a still-tearing-down volume (the wedge mode that cancel propagation already shortens but doesn't eliminate).

**Ordering**: `write-settled` always fires AFTER the terminal outcome event (`write-complete` / `write-cancelled` / `write-error`) for the same `operation_id`. The BE guarantees this by placing the settle emit in a `WriteSettledGuard` RAII struct whose `Drop` runs at the very end of the spawn-task scope, AFTER all the conditional terminal-event emits.

**Guard pattern**: every spawn-task entry point (`start_write_operation` in `mod.rs`, the volume-delete branch in `delete_files_start`, `copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`) constructs a `WriteSettledGuard` at the top of the spawned task. The guard's `Drop` impl emits the event. This makes the emit panic-safe: even if the handler closure panics and the task exits via `JoinError`, the guard still drops as part of stack unwinding, so the FE never hangs waiting for a settle that never comes. See `settle_event_tests.rs::settled_fires_on_panic_unwind` for the safety-net pin.

**Cache-cleanup panic safety**: removal from `WRITE_OPERATION_STATE` + `OPERATION_STATUS_CACHE` must also survive a panic, or the op lingers forever in `list_active_operations`. The `start_write_operation` path is already safe — its handler runs in `spawn_blocking`, so a panic returns as `Err(join_error)` and the post-`match` cleanup always runs. The volume-delete branch runs `delete_volume_files_with_progress(...).await` directly on the async task, so its cleanup can't live after the `.await`; it routes through an `OperationStateGuard` (in `state.rs`) constructed right after `_settled_guard`. The guard's `Drop` removes both map entries on unwind. It's declared after `_settled_guard`, so it drops first — cache removal runs before the `write-settled` emit, matching `start_write_operation`'s ordering. Pinned by `state.rs::tests::operation_state_guard_frees_both_caches_on_panic_unwind`.

**Payload**: `{ operationId: String, operationType, volumeId: Option<String> }`. The `volume_id` is best-effort: filled with the source volume's display name for volume-aware ops (copy/move between volumes, volume delete), `None` for pure local-FS operations. The FE currently filters only by `operationId`; `volume_id` is for diagnostics and forward compatibility.

**Tests**: `settle_event_tests.rs` pins the guard's invariants (single fire, panic safety, ordering relative to the terminal event). `delete/volume_cancel_tests::volume_*_emits_write_settled_event` pin the integration shape against the volume-delete handler.

## Key decisions (shared)

**Decision**: Every scan reports **two** byte totals — `total_bytes` (write footprint, un-dedup'd) and `dedup_bytes` (`du`-equivalent, each inode once). Delete consumes `dedup_bytes`; copy/move consume `total_bytes`; the Copy dialog shows both.
**Why**: A hardlink contributes differently to the two operations. **Delete** frees an inode only when its last link is removed, so the bytes-freed number is the dedup'd one — counting every link would claim to free 80 GB when only 60 GB (cargo `target/`) actually frees. **Copy/move** materialize every hardlink as an independent file at the destination (hardlinks don't survive a cross-volume copy, and even a same-FS `cp` doesn't relink), so the bytes-written number — and the disk-space reservation — is the full write footprint. The earlier single-`total_bytes`-is-dedup'd design got delete right but silently regressed copy: the space check under-reserved (risking ENOSPC mid-copy) and the bar hit 100% early. Now `walk_dir_recursive` / `walk_cached_entries` / `scan_volume_recursive` / `LocalPosixVolume::scan_for_copy` / `scan_subtree_with_oracle` all track both, using a `seen_inodes: HashSet<u64>` (mirrors `indexing/scanner.rs`, `nlink == 1` fast path, operation-scoped across source roots; **Unix-only**, where non-Unix has no `nlink()` so `dedup_bytes == total_bytes`). Volume backends populate `FileEntry::inode` only for `LocalPosixVolume` files with `nlink > 1` (MTP/SMB/InMemory leave it `None`, so dedup is a no-op and the two totals are equal). The **scan-phase** progress bar reports the dedup'd running total (it's compared against the indexer's inode-dedup'd `dir_stats` estimate, so reporting the write footprint would overshoot 100% on hardlink trees). The **delete** active phase sums per-entry `progress_bytes`/`VolumeDeleteEntry::progress_bytes` (= dedup'd) against the `dedup_bytes` denominator. The **copy** active phase credits full per-file `size` against the `total_bytes` denominator (no chunk scaling). The Copy dialog surfaces the gap with a one-line note ("X will be written; source is Y; the extra is hardlinked files…") via `dedup_bytes_total` on the scan-preview events — copy-only, since a same-FS move writes nothing. Pinned by `delete/hardlink_progress_tests.rs`, `delete/volume_hardlink_progress_tests.rs`, `transfer/hardlink_progress_tests.rs::copy_counts_write_footprint_for_hardlinks`, `scan.rs::tests::walker_dedupes_*`, `local_posix_test::test_scan_for_copy_dedupes_hardlinks_for_source_size_only`, and `transfer-dialog-utils.test.ts::shouldShowHardlinkNote`.

**Decision**: `WriteProgressEvent::with_scan_meta` is the only path that sets the scan-only fields (`current_dir`, `dirs_done`, `expected_files_total`, `expected_bytes_total`).
**Why**: 20+ emit sites construct `WriteProgressEvent` literals for active-phase events. Adding four optional fields to the struct would force every site to spell out their defaults, pure mechanical noise. The `new(...)` constructor takes the eight core counter fields and defaults the scan meta (`None` / `0`); the scan emit sites in `scan.rs`, `scan_preview.rs`, and `delete/walker.rs::scan_volume_recursive` opt in via `.with_scan_meta(current_dir, dirs_done, expected)`. Future scan-related fields go through the same builder. If a real refactor of the 20 literals to `new(...)` ever happens, the builder pattern still composes cleanly on top.

**Decision**: All write operations go through `OperationEventSink` instead of `tauri::AppHandle`.
**Why**: Decouples the copy/move/delete/trash orchestration from the Tauri framework. `TauriEventSink` wraps AppHandle for production; `CollectorEventSink` stores events for test assertions. Enables testing the full pipelines end-to-end (multi-file copy, cancellation, conflict resolution, progress tracking) without a Tauri runtime. Every `*_with_progress` function (local copy, local move, local delete, local trash, volume copy, volume move, volume delete) takes `&dyn OperationEventSink` or `Arc<dyn OperationEventSink>` and emits via the sink. `state.emit_progress_via_sink` is the only progress-emit method — `emit_progress_via_app` is gone. Trash unit tests (`delete/trash.rs::tests::trash_*_via_sink`) drive `trash_files_with_progress` with a `CollectorEventSink` to pin the empty-sources, pre-cancel, and all-missing-sources branches without invoking the OS trash.

**Decision**: Scan preview reuses watched listings (the "fresh-listing oracle").
**Why**: Pre-flight scans for copy/move on MTP (and to a lesser degree SMB and big local trees) used to duplicate work the backend already had in `LISTING_CACHE`. Selecting 135 photos in a watched `/DCIM/Camera` (~15k entries) and pressing F5 would re-list the parent dir over USB just to look up size by name — ~17 s of "Verifying before copy…" while the listing was already fresh on the pane behind the dialog. `run_volume_scan_preview` now groups input sources by parent dir and consults `try_get_watched_listing(volume_id, parent)` first. On hit, sizes and `is_directory` flags come from the cached `FileEntry` for top-level files; top-level directories recurse via `scan_subtree_with_oracle`, which re-applies the oracle at every level (so a subfolder open in another pane also short-circuits). On miss, the call falls through to `volume.scan_for_copy_batch_with_progress(paths_in_group, ...)` — same code path as before — so MTP's parent-grouping and SMB's pipelined-stat optimizations still run for cold-cache parents. The local-FS walker (`walk_dir_recursive` in `scan.rs`) also takes an oracle check at the top of each recursive call, with `volume_id = "root"` plumbed through from `scan_sources_internal` and `run_scan_preview`. The freshness contract is bright-line at the watcher boundary: no "5 seconds is fresh enough" TTL, just "the volume's `listing_is_watched(path)` returned true." See `file_system/listing/caching.rs::try_get_watched_listing` for the per-backend debounce windows that contract tolerates.

**Decision**: Copy and move are durable before they report complete: per-file `sync_data` (fdatasync) in chunked copy, plus an end-of-op targeted `fdatasync` pass over the transaction's recorded destinations for the strategies that don't flush themselves. Delete and trash don't sync at all.
**Why**: "Complete" must mean "durable on disk," not "buffered in the OS page cache." Without it, a user who copies to a USB stick / SD card and ejects (or the machine sleeps) right after "Copy finished" loses the file — and on a move it's gone from both source and dest. The flush is targeted, not a whole-machine `libc::sync()`: that global sync also stalled unrelated apps (AGENTS.md principle #5). The mechanism: (1) `transfer/chunked_copy.rs` calls `dst_file.sync_data()` per file, so each file is durable as it completes — a crash mid-batch on a long transfer leaves earlier files safe. (2) Before emitting `write-complete`, `durability::flush_created_destinations` emits a `Flushing`-phase progress event, then `fdatasync`s every recorded destination that wasn't already flushed, plus a best-effort `fsync` of each distinct parent directory so the rename-into-place (temp+rename / cross-FS staging) is durable too. It reuses `CopyTransaction.created_files` (no parallel dest-tracking) and skips an `already_synced: HashSet` of paths the strategy already made durable: chunked-synced files and APFS-clonefile / reflink dests (those share copy-on-write extents with the source, so a flush is moot). On macOS every produced-bytes path is either clonefile (moot) or chunked (already synced), so the end-of-op pass does no extra `fdatasync` there — its job on macOS is purely the honest `Flushing` UI state; on Linux it's the real flush for `copy_file_range` dests. Cross-FS move flushes the FINAL paths (Phase 3 renames staging → destination, so the staging entries in `created_files` are remapped to their final prefix before the pass — this also covers the Phase-3 `throwaway_tx` renames that aren't in the real transaction). Same-FS move (pure rename) writes no data, so its flush just `fdatasync`s the moved files (cheap) and their parent dirs to make the new directory entries durable. The flush is best-effort on error: a failed `sync_data` is logged (`target: "write_durability"`), not propagated — the bytes are written either way and failing the whole op at the final flush is worse UX. Pinned by `transfer/copy_tests.rs::local_copy_emits_flushing_phase_before_complete` and `transfer/move_op_tests.rs::cross_fs_local_move_emits_flushing_phase_before_complete`; FE label by `TransferProgressDialog.flushing.test.ts`. **Cross-volume copy/move landing on a local disk** (MTP → Local, SMB → Local, USB import) doesn't go through this local-FS engine — it flows through `LocalPosixVolume::write_from_stream`, which keeps the same promise by `sync_data`-ing each file (plus a best-effort parent-dir fsync for the directory entry) before it returns, so each file is durable as it completes. That path doesn't yet emit the `Flushing` UI phase (the volume copy/move handlers don't call `flush_created_destinations`); a follow-up could route them through the end-of-op pass for UI consistency, but the per-file `sync_data` already makes them durable.

## Shared gotchas

**Gotcha**: On macOS, never use `statvfs` alone for disk space checks; use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks. On APFS, purgeable space (iCloud caches, APFS snapshots) can account for tens of GB that macOS will reclaim on demand. Using `statvfs` causes the "insufficient space" error to reject copies that would actually succeed, and shows a different available-space number than the status bar (which uses the NSURL API). `validate_disk_space` in `validation.rs` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

**Gotcha**: Volume-side `on_progress` callbacks report counts LOCAL to the current scan operation, not cumulative.
**Why**: `Volume::scan_for_copy_batch_with_progress` and `scan_subtree_with_oracle` both invoke `on_progress(count)` with a count local to the current `list_directory` call / subtree (starts at 1 each time). Forwarding that unchanged through `run_volume_scan_preview`'s closure made the FE's running tally drop visibly between parent groups, between sibling top-level dirs in a cache-hit branch, and between recursion frames inside `scan_subtree_with_oracle`. `run_oracle_aware_batch_scan` now wraps `on_progress` with a `baseline = aggregate.file_count` shift before each scan call (cold-cache batch + cache-hit subtree), and `scan_subtree_with_oracle` does the same at its own recursion site (`baseline = totals.file_count`). The visible FE count stays cumulative across the entire scan. Direct `on_progress(aggregate.file_count)` emit sites in `run_oracle_aware_batch_scan` (cache-hit per-file paths, fallthrough `scan_for_copy` after a name miss) stay unwrapped — they're already cumulative. Future scan call sites that delegate to a volume backend or to `scan_subtree_with_oracle` need the same baseline wrap.

**Gotcha**: Copy's bar/space-check use the write footprint (`total_bytes`), not the dedup'd source size — by design.
**Why**: A copy of a hardlink-heavy tree writes every link in full, so the bar fills against the write footprint and the disk-space check reserves it (the headline can legitimately read "80 GB" for a 60 GB-`du` `target/`). This is correct, not a bug — `scan_subtree_with_oracle` and `copy_volumes_with_progress` both carry the un-dedup'd `total_bytes` for copy, while the dedup'd `dedup_bytes` rides alongside purely to drive the dialog's clarifying note. Don't "fix" copy to show the dedup'd number: that would under-reserve disk space and stall the bar on dupes. The cross-volume copy path (`copy_volumes_with_progress` → `volume_strategy::copy_directory_streaming`) credits raw streamed bytes per file, which already equals the write footprint, so no dedup wiring is needed there. The one residual approximation: `dedup_bytes` over the cross-source-hardlink case (a file hardlinked into two separately-selected sources) counts twice, slightly understating the dedup savings shown in the note — safe direction, documented on `CopyScanResult::dedup_bytes`.

**Gotcha**: Volume disconnect mid-walk races with the oracle.
**Why**: The oracle returns `Some(entries)` when `listing_is_watched` is true at the moment of the check. Between that read and the recursive walk consuming the entries (and then issuing real `list_directory` calls for any sub-subfolders that aren't cached), the watcher can die (cable yanked, network drop). The synthesized totals for the cached level are correct — they reflect what the listing held — but recursion into now-disconnected sub-subfolders fails per-call, and the per-file copy/delete later then hits `DeviceDisconnected`-shaped errors instead of a single "device gone" message at the scan level. Same race that `scan_for_copy_batch` already had; the oracle doesn't widen it. Documented here so future investigation knows where to look.

## Dependencies

- `crate::file_system::volume`: `Volume` trait, `SpaceInfo`, `ScanConflict` (used by `transfer/volume_copy.rs`)
- `crate::ignore_poison`: `IgnorePoison` extension for `RwLock`/`Mutex` to not panic on poisoned locks
- External: `tauri` (emit, AppHandle), `uuid` (operation IDs, temp names), `libc` (access, statvfs), `xattr`, `exacl`, `filetime` (metadata preservation in `transfer/chunked_copy.rs`)

## Testing bar

This module's state machine (`state.rs`) is the spine of the cancel UX. Past investigations found one real production bug here ([commit `1de4255d`](../../../../../../docs/notes/speed-up-e2e-tests.md), lost-rollback on `Ok(())` arm) plus 30+ mutation-testing gaps that have since been pinned. New transitions or new cancel paths must:

1. **Drive the state machine through the public interface in tests.** Direct `state.intent.store(...)` mutation bypasses the validation guard and effectively dead-tests it. Pattern to copy: `state.rs::tests::test_cancel_via_public_path`.
2. **Cover both the happy path and the cancel-during-X race** for any new write-side operation. The Cancel-copy bug was specifically the `Ok(())` arm of the loop not re-checking intent.
3. **Add at least one E2E test** for user-visible flows (transfer dialogs, conflict policies); use `dispatchMenuCommand` for keyboard-shortcut triggers, see `docs/testing.md` § "❌ Synthesized F-key dispatches".
4. **Run `cargo mutants --file src/file_system/write_operations/<file>.rs`** after substantial changes; this module has ~85-90% mutation score per file and shouldn't regress. See `docs/testing.md` § "Process".

See also: [docs/testing.md](../../../../../../docs/testing.md) for the project-wide testing playbook.
