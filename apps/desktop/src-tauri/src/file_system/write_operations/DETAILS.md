# Write operations details

Pull-tier docs for `src-tauri/src/file_system/write_operations/`: architecture, flows, and decision rationale.
Must-know invariants and gotchas live in [CLAUDE.md](CLAUDE.md).

Frontend counterpart: [`apps/desktop/src/lib/file-operations/CLAUDE.md`](../../../../src/lib/file-operations/CLAUDE.md)
(umbrella) plus colocated child docs for [`transfer/`](../../../../src/lib/file-operations/transfer/CLAUDE.md),
[`delete/`](../../../../src/lib/file-operations/delete/CLAUDE.md),
[`mkdir/`](../../../../src/lib/file-operations/mkdir/CLAUDE.md), and
[`mkfile/`](../../../../src/lib/file-operations/mkfile/CLAUDE.md).

Subdirs:
- [`transfer/CLAUDE.md`](transfer/CLAUDE.md) — copy + move (local FS, cross-volume, MTP, SMB), conflict resolution, the shared transfer driver, platform-specific copy backends.
- [`delete/CLAUDE.md`](delete/CLAUDE.md) — delete walker (local + volume-aware), trash, the oracle-aware delete fast path.

## Purpose

Implements the four destructive file operations as background tasks that stream Tauri events to the frontend. Every operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites, network mounts, cross-filesystem moves, and name/path length limits.

Pre-flight scans reuse cached listings when the source volume reports an active watcher, avoiding redundant `list_directory` calls. The freshness contract and per-backend debounce windows are documented in `../volume/CLAUDE.md` and `../listing/caching.rs::try_get_watched_listing`.

## Files (top level)

- **`mod.rs`**: Public API: `copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start`. Each builds an `OperationDescriptor` + a deferred start and hands them to `manager::spawn_managed` (local sync ops go via `start_write_operation`, which wraps the blocking handler in `spawn_blocking`; the volume-delete branch builds its deferred inline). Validation runs inside the handler closure on the blocking thread pool, never on the async executor. Also re-exports `transfer::*` and `delete::*` public symbols so external callers keep their `crate::file_system::write_operations::<symbol>` import paths.
- **`manager.rs`**: The operation manager — the single registry + lane-admission scheduler every write op flows through. `OperationManager`, `OperationDescriptor`, `DeferredStart`, `LifecycleStatus`, `OperationSnapshot`, `OperationsChanged` (the `operations-changed` event), `ManagedTaskGuard` (panic-safe lane + cache release), and the `list_operations` / `cancel_operation(s)` API. See [Operation manager](#operation-manager).
- **`types.rs`**: Pure serializable DTOs: events, config, errors, results. `WriteOperationConfig`, `ConflictResolution`, `WriteOperationError`, `DryRunResult`, scan preview events, the config convenience impls (`Default`, `VolumeCopyConfig::from(&WriteOperationConfig)`). Holds the event STRUCT definitions; their builder impls and the sinks live in `event_sinks.rs`. Re-exports `OperationEventSink`, `CollectorEventSink` (from `event_sinks`) and `IoResultExt` (from `error_classification`) so existing `types::…` import paths keep resolving. `TauriEventSink` is re-exported at the `write_operations` module root (and up through `file_system`) for the IPC edge, not here — the pipeline layer only ever names the trait.
- **`event_sinks.rs`**: The `OperationEventSink` trait (decouples event emission from `tauri::AppHandle`), `TauriEventSink` (production), `CollectorEventSink` (test-only), and the builder impls for `WriteProgressEvent` (`new`/`with_scan_meta`) and `WriteErrorEvent` (`new`). `TauriEventSink::emit_complete` calls `analytics::emit_completion_analytics`.
- **`archive_edit/`**: Runs a zip mutation (`ArchiveMutator`) as a managed op. Split by seam: `routing.rs` (shared detection/path primitives, the zip-only write guard, the duplicate oracle, the instant-op sink builder), `engine.rs` (the `run_managed_edit` LOCAL/REMOTE apply chokepoint, `PlanError`, `MutatorHooks`, error mapping, post-commit source deletion), `conflicts.rs` (copy-into collision resolution — policy + interactive prompt), `copy_into.rs` (route + source materialization — a remote source is pulled to a scratch dir, a local one used in place — + changeset planning + driver for copy/move INTO a zip), `move_out.rs` (`route_archive_move_out` — the extract-then-batch-delete compound op), `driver.rs` (`ArchiveEditRequest`, `archive_edit_start`, and the in-archive delete route). Public routes are re-exported from `mod.rs` so `archive_edit::<symbol>` paths hold. See [Archive edits](#archive-edits).
- **`analytics.rs`**: PII-free PostHog completion analytics (`emit_completion_analytics`, `item_count_bucket`), `pub(super)`, called only by `TauriEventSink`. Copy/Move → `file_transfer_completed`, Delete/Trash → `delete_used`; every prop is categorical (op, count bucket, a bool), no names or paths.
- **`error_classification.rs`**: Maps raw `std::io::Error` to typed `WriteOperationError` variants from `errno`/`ErrorKind` only (never the message). `classify_io_error`, the `IoResultExt` extension trait (`with_path`), and `impl From<std::io::Error> for WriteOperationError`.
- **`state.rs`**: The operation-lifecycle core. The `WRITE_OPERATION_STATE` + `OPERATION_STATUS_CACHE` `LazyLock<RwLock<HashMap>>` caches, `WriteOperationState`, `CopyTransaction`, busy-volumes tracking, the query/cancel/resolve APIs, and the `WriteSettledGuard` RAII shape for the settle contract. Re-exports the `operation_intent` and `scan_cache` types so their `state::…` paths keep resolving.
- **`operation_intent.rs`**: The two per-operation state machines. `OperationIntent` (the `Running → RollingBack/Stopped` cancellation/rollback machine, with `load_intent` / `is_cancelled`) and `PauseGate` (pause/resume parking: a sync condvar for `spawn_blocking` drivers plus an async `Notify` for volume drivers).
- **`scan_cache.rs`**: Scan-preview caching. `ScanPreviewState`, `CachedScanResult`, the `SCAN_PREVIEW_STATE` / `SCAN_PREVIEW_RESULTS` caches, the scan-result TTL safety net (`insert_scan_result` / `release_scan_result` / `expired_scan_result_ids`, `SCAN_RESULT_TTL`), and the `FileInfo` / `ScanResult` carriers.
- **`validation.rs`**: Source/destination validation: `validate_sources`, `ensure_destination_dir` (the local copy/move destination gate — creates the destination and any missing ancestors via `create_dir_all` when absent, so a transfer into a brand-new folder just works; rejects a path that exists but isn't a directory; runs AFTER `validate_destination_not_inside_source` so it never creates a folder inside a source), `validate_destination_writable` (via `libc::access`), `validate_disk_space` (NSURL API on macOS, `statvfs` on Linux), `validate_not_same_location`, `validate_destination_not_inside_source` (resolves a not-yet-created dest via its nearest existing ancestor, `canonicalize_or_nearest_ancestor`), `validate_path_length`. Identity/filesystem checks: `is_same_file` (inode+device), `is_same_filesystem` (device IDs), `path_exists_or_is_symlink` (dangling-symlink-aware), `is_symlink_loop`. The volume-aware pipelines have the same recursive dest-create behavior: `copy_volumes_with_progress` / `move_volumes_with_progress` (cross-volume) and `move_within_same_volume_with_progress` (same-volume rename) each call `Volume::create_directory_all(dest)` before transferring, so a copy/move into a brand-new nested folder auto-creates it on EVERY backend (local, SMB, MTP, in-memory), matching `ensure_destination_dir`. The cross-volume/copy gate runs AFTER the dest-inside-source guard (same order as local). See `volume/DETAILS.md` § "Recursive destination create".
- **`rename.rs`**: Rename validation + the managed rename mutation. The read-only, UNMANAGED validity/permission checks (`check_rename_validity_impl`, `check_rename_permission_sync`, run per-keystroke / on-commit, never touch the manager) plus `rename_managed`, a managed instant op that runs the rename inside `manager::run_instant` (registers a `Running` record + marks its volume busy for its sub-second duration, reserves no lane, returns its `Result` inline). The command layer (`commands/rename.rs`) is a thin pass-through into here. See [Managed instant ops](#managed-instant-ops-run_instant).
- **`create.rs`**: New-folder / new-file creation. `create_directory_managed` / `create_file_managed` run the mutation inside `manager::run_instant` (busy-mark + brief `Running` record, no lane, returns the new path inline; no inner timeout — the command's outer 5 s timeout drops the future on a hang and the guard releases the busy set). Co-locates the synthetic listing-cache diff (`emit_synthetic_entry_diff` / `should_emit_synthetic_diff`) that updates the pane when a new entry appears, for local-FS-backed volumes. The command layer (`commands/file_system/write_ops.rs`) is a thin pass-through. See [Managed instant ops](#managed-instant-ops-run_instant).
- **`conflict.rs`**: Conflict resolution. The two-bucket `ApplyToAll` latch model (`apply_to_all_effective` / `apply_to_all_record`). `resolve_conflict` (`tokio::sync::oneshot` channel wait for Stop mode), `reduce_conditional_resolution`, `apply_resolution`, `find_unique_name` (O_EXCL reservation). The ` (N)` name formatting lives in ONE pure helper, `numbered_name(stem, ext, counter)` (`counter 0` = bare, `1..` = ` (N)`); `find_unique_name` and the clipboard-paste writer both go through it so the two numbering paths can't drift. Conflict-event/info builders: `build_conflict_event`, `calculate_dest_path`, `create_conflict_info`, `sample_conflicts`.
- **`paste_clipboard.rs`**: `write_payload_to_dir` — the backend half of "paste clipboard content as a file" (issue #35). Takes an already-read `ClipboardPayload` + a `&Path` dir (decoupled from NSPasteboard / the IPC edge, so it's `TempDir`-testable). Maps payload→content (`ext` + `PastedKind` + bytes; markdown sniff for `.md` vs `.txt`), then writes `pasted.<ext>` via a `numbered_name` retry loop: candidate → `Volume::create_file` (O_EXCL create+write) → on the TYPED `VolumeError::AlreadyExists`, bump the counter. No pre-scan-then-write TOCTOU, and it works on any writable volume. Reuses `create::should_emit_synthetic_diff` + `emit_synthetic_entry_diff` (both `pub(super)`) so the new file lands in the pane and the FE cursor-lands like mkfile. `Nothing` payload → `Ok(None)` (the typed no-op). The command (`commands/clipboard.rs::paste_clipboard_as_file`) reads the raw flavors on the main thread, picks/converts off-main (`spawn_blocking`), and calls this under a **30 s** write timeout — a longer tier than the 5 s empty-mkfile write, because the payload can be a large image written to a slow network volume. **Partial-file-on-timeout edge (accepted):** if a very large paste to a very slow volume exceeds 30 s, the write future is dropped and a partial `pasted.<ext>` may remain (the user sees a timeout and can retry / delete). This is bounded, rare (local writes never approach 30 s; on a local FS `create_file`'s `spawn_blocking` isn't even cancellable, so the file actually completes), and only affects slow network volumes. If it ever matters, route paste-as-file through the managed transfer engine for cancellation + no-partial guarantees. Pasteboard read + flavor precedence: [`clipboard/DETAILS.md`](../../clipboard/DETAILS.md) § Paste clipboard content as a file.
- **`overwrite.rs`**: Temp+rename-aside atomicity: `ResolvedDestination`, `safe_overwrite_file`, `safe_overwrite_dir`.
- **`durability.rs`**: `flush_created_destinations` (emits the `Flushing` event, then `fdatasync`s each created destination + parent dir, skipping already-synced paths). `lookup_indexed_size` (drive-index directory size for conflict UI).
- **`cancellable.rs`**: Cancellation-aware execution: `run_cancellable`, `run_cancellable_scoped` (poll the cancel flag while blocking work runs on a separate thread). Detached background cleanup: `remove_file_in_background`, `remove_dir_all_in_background`.
- **`scan.rs`**: `scan_sources` (recursive walk, emits progress), `dry_run_scan`, shared `walk_dir_recursive` walker. The `on_progress` callback receives `(files, dirs, bytes, current_file, current_dir)`; the walker reads `current_dir` from `path.parent()` so the UI can show "in directory: …" alongside the filename. Scan emit sites populate `WriteProgressEvent.current_dir` plus index-derived `expected_files_total` / `expected_bytes_total` (via `WriteProgressEvent::with_scan_meta`) so the frontend renders a real progress bar during the foolproof re-scan. Expected totals come from `crate::indexing::expected_totals::expected_totals_for_sources` (`None` when the index doesn't cover all sources; the FE falls back to a tally-only display).
- **`scan_preview.rs`**: Scan preview subsystem for Copy dialog live stats: `start_scan_preview`, `cancel_scan_preview`, `is_scan_preview_complete`. Background scans (local and volume-based) with result caching. Emits `expected_files_total` / `expected_bytes_total` (sampled once at scan start from the drive index) on every `scan-preview-progress` event, alongside the running tallies and `current_dir`.
- **`eta.rs`**: `EtaEstimator`: time-weighted EWMA per axis (bytes, files), τ ≈ 3 s. Combines via `max(ETA_bytes, ETA_files)`. One per `WriteOperationState`, fed by `state.enrich_progress` at every `write-progress` emit site. See [ETA + throughput](#eta--throughput) below.
- **`tests.rs`**: Cross-cutting unit tests.
- **`scan_preview_listing_progress_tests.rs`**: Regression tests for the `ListingProgress` callback shape.
- **`scan_preview_oracle_tests.rs`**: Integration tests for the fresh-listing oracle inside scan preview.
- **`settle_event_tests.rs`**: Tests for `WriteSettledGuard` invariants (single fire, panic safety, ordering).
- **`validation_integration_test.rs`**: Validation functions, safety checks, path length, disk space tests.

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

**Compressed-size estimate (Compress dialog).** When `start_scan_preview` runs with `sample_for_estimate` set (Compress mode only), the LOCAL walk feeds a cheap deflate-sampling estimator (`compress_estimate::CompressEstimator`) that predicts the zip's output size, shown live-ish in the dialog beside the scanned byte total. Mechanics and invariants:

- **Local-FS walk only; remote is suppressed.** The per-file `WalkContext::on_file` hook fires only from `walk_dir_recursive` / `walk_cached_entries` (the `run_scan_preview` path). `run_volume_scan_preview` (SMB/MTP) never samples and never guesses — the estimate is simply `None`. Sampling a remote source would do real network reads and defeat the oracle's zero-I/O short-circuit, and an extension-only guess is unbounded (a single mistyped file → 8×-wrong), so an absent estimate is the honest choice. **Don't add sampling to the volume/oracle path.**
- **Off the walk thread.** The hook is a cheap channel push; a bounded worker thread deflates a 32 KiB head window per file at reference level 6 under an 8 MiB total byte budget, so the sampling CPU never lands on the walk's critical path and worst-case added time is ~105 ms regardless of tree size. Media-heavy trees cost near zero (an incompressible-extension table shortcuts the read). Files under 4 KiB, budget-exhausted files, and unreadable files take a running-average ratio. The worker cancels with the scan (shared `cancelled` flag) and is joined before the complete event; a sampling panic degrades to `None` and never fails the scan.
- **Per-class subtotals, scaled on the FE.** The estimate ships as three `CompressedSizeEstimate` subtotals of estimated **level-6** bytes, bucketed by each file's sampled compressibility class. The frontend re-scales to the user's selected deflate level via a baked per-class curve (`compress-estimate-scaling.ts`) with no re-scan, so moving the level slider updates the shown number arithmetically. Level 6 is the reference (shown value = sum of subtotals). It rides `scan-preview-complete` (and the `get_scan_preview_totals` recovery path) only; while scanning the dialog shows a loading affordance.
- Parameters (window, budget, tiny threshold, extension table, level curve) and their measured accuracy/cost: [`docs/notes/compress-size-estimate-spike.md`](../../../../../../docs/notes/compress-size-estimate-spike.md).

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

**Contract:** call `crate::downloads::note_pending_write_for_cmdr(&dest_path)` immediately before the write syscall (or the volume-trait equivalent: `Volume::write_from_stream`, `Volume::create_file`, `Volume::create_directory`, `Volume::rename`, `Volume::delete`).

**Locked-in scoping:** the prefix check lives INSIDE the helper (and the underlying `IgnoreSet::note_pending`). Call sites invoke unconditionally; paths outside the resolved Downloads root silently no-op. **Don't add `if path.starts_with(downloads_dir)` guards at call sites**: centralizing the scope in the helper keeps it from drifting across call sites (the downloads watcher's ignore-set design lives in the `downloads` module docs).

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

- **`write-progress`**: Every ~200 ms during copy/move/delete/trash
- **`write-conflict`**: Stop mode hit a conflicting destination file
- **`write-complete`**: Operation finished successfully
- **`write-cancelled`**: Operation cancelled (includes `rolled_back` flag)
- **`write-error`**: Operation could not complete. Carries only `error: WriteOperationError` (typed, word-free); no rendered prose crosses IPC. The FE renders the title/explanation/suggestion + category from this typed error via `transfer-error-messages.ts` in `TransferErrorDialog` and applies category-based colors.
- **`write-settled`**: Emitted once per op after the spawned background task fully returns. See [Settle contract](#settle-contract).
- **`volumes-busy-changed`**: The set of volume IDs with an in-flight op changed (an op started or finished). Payload is `string[]`. See [Busy-volumes set](#busy-volumes-set).
- **`operations-changed`**: The operation registry's membership or lifecycle status changed. Thin snapshot (`{ operations: OperationSnapshot[] }`), NOT 200 ms progress. See [Operation manager](#operation-manager).
- **`write-source-item-done`**: All files for a top-level source item processed (for gradual deselection)
- **`dry-run-complete`**: `config.dry_run == true` (returns `DryRunResult`)
- **`scan-preview-progress`**: During `start_scan_preview`
- **`scan-preview-complete`**: Preview scan finished
- **`scan-preview-error`**: Preview scan failed
- **`scan-preview-cancelled`**: Preview scan cancelled

## Operation manager

The full model and the why behind each decision are captured in this section. Design history is in git (former `docs/specs/2026-06-21-transfer-queue-pause-plan.md`).

`manager.rs` is the single coordinator every write op flows through. It exists because there were FIVE independent spawn paths (`start_write_operation` for local copy/move/trash + local delete; the volume-delete branch in `delete_files_start`; `copy_between_volumes`; `move_between_volumes`; `move_within_same_volume`), each hand-rolling its own `tokio::spawn` + state-insert + status-register + `WriteSettledGuard`, and an op always spawned immediately. The manager unifies them behind `spawn_managed(descriptor, state, deferred)` and adds a registry with real lifecycle states plus **lane-based admission** that can serialize ops which would thrash a shared device.

### Lanes and `Volume::lane_key()`

Each op touches the [`LaneKey`](../volume/CLAUDE.md)s of its source and destination volumes (same-volume ops touch one). Lane keys come from `Volume::lane_key()` (in `volume/mod.rs`), NEVER from parsing a `volume_id` string (`no-string-matching`). Per backend: `LocalPosixVolume` → the volume root (the trait default; each local mount is its own `LocalPosixVolume`, so the root IS the mount root); `MtpVolume` → `device_id` (one USB pipe per device, so every storage on a device shares its lane); `SmbVolume` → its `volume_id` (already `smb_volume_id(server, port, share)` — server+share granularity); `InMemoryVolume` → a `with_lane_key(key)` builder, defaulting to root so the ~169 existing `new(...)` sites are untouched and tests opt into same-lane vs different-lane.

The both-local branches of `copy_between_volumes` / `move_between_volumes` compute the two lane keys from the live volume handles and pass them into `copy_files_start` / `move_files_start` as `Some(lanes)`. The plain local commands pass `None`, so the entry point derives a lane from `volume_ids` (`local_lanes`): empty → the `root` lane, else one lane per id. This is a faithful proxy for `lane_key()` on the path where no `Volume` handle is threaded through — it uses each id as an opaque whole, so `no-string-matching` holds.

### Admission — global FIFO, atomic multi-lane reservation

The manager keeps one ordered queue (`order`) plus a `lane_use` table (lane → in-use count; budget 1 per lane in v1, a lane is free iff its count is 0; a `HashMap` not a set so v2 budgets > 1 reshape nothing). An admission pass walks pending ops oldest-first and admits the first whose EVERY lane is free, reserving all its slots atomically, flipping it to Running, registering its volumes busy, and spawning its deferred start. It loops so one pass can admit several disjoint-lane ops. A two-lane op can't starve behind churn on a single lane — there are no per-lane queues, so the multi-lane op is always considered at its FIFO position against the whole lane table.

### Deferred start, not "spawn then block on a semaphore"

A queued op holds only DATA describing how to begin: a boxed `FnOnce() -> Pin<Box<dyn Future + Send>>` (`DeferredStart`). The manager spawns it only on admission. Blocking a spawned op on a lane semaphore would pin a `spawn_blocking` pool thread idle per queued op — a leak that can deadlock the finite pool under many queued ops. Each deferred future owns the op end-to-end (the `WriteSettledGuard`, the actual transfer/delete, the terminal-event emit) and ends by calling `manager().on_settled(id)`.

### Dequeue on settle — explicit, NOT in `Drop`

`on_settled(id)` (the happy path) frees the op's lane slots, removes it from the registry, cleans `WRITE_OPERATION_STATE` + the status cache, and runs an admission pass (which may spawn the next op). It's sequenced after the terminal event, exactly where the old per-site cache cleanup ran. The `ManagedTaskGuard` is the panic safety net: held by each spawned task, its `Drop` frees lanes + cleans caches but NEVER spawns (no admission pass). Spawning during the previous op's unwind would re-enter the manager mid-panic (abort) or deadlock on a lock held up-stack. So a panicking op still releases its lanes, but the next op is admitted only on a healthy settle (the next registration's admission pass, or another op's `on_settled`, picks it up). The happy path calls `task_guard.disarm()` right before `on_settled` so its now-redundant Drop is a no-op. Pinned by `manager::tests::panicking_op_releases_its_lane_without_spawning_next`.

### The admission pass spawns admitted ops on the APP runtime, not the caller's

The admission pass (from `spawn_managed` or `on_settled`) spawns each admitted op's deferred start with **`tauri::async_runtime::spawn`, deliberately NOT `tokio::spawn`**. `tokio::spawn` binds to whatever runtime is current when the pass runs — and the pass can run on a runtime that has nothing to do with the op it's admitting. Admission is global and there is a lock-free window between an op's registration (`spawn_managed` inserts it Queued, drops the lock) and its own admission pass: a CONCURRENT op's `on_settled`, running on a different runtime, can reach the pass first and admit the freshly-registered op. So the runtime that spawns an op is racy, not "the op's own caller". `async_runtime::spawn` pins every op to the one process-global runtime that outlives them all.

In production this is a no-op (there is exactly one long-lived Tauri runtime, so ambient and app runtime are identical). **The guard exists for the process-global-manager + per-runtime-caller topology**, which is the test harness: every `#[tokio::test]` runs on its own runtime, and with a bare `tokio::spawn` an op admitted by a runtime that is then torn down is orphaned — it never runs, never settles, and leaks its lane forever, wedging later ops (the observed nondeterministic `wait_until` timeouts). Pinned by `manager::tests::admitted_op_runs_even_if_the_admitting_runtime_is_dropped` (a throwaway runtime admits an op and is dropped without driving it; the op must still complete). This race is lane-INDEPENDENT — it hit even unique-lane ops — so a hermetic-lanes-only test fix could not close it; the guard is the actual fix.

Independently, the archive-edit tests still give each op its OWN lane (`archive_edit::test_support::unique_lane_id`, and `InMemoryVolume::with_lane_key`), matching the `manager::tests` discipline ("unique operation ids + lane keys"). That's for test ISOLATION and parallel speed — a shared global lane serializes unrelated tests and couples their timing — not for orphan-safety, which the guard now owns. The one behavior that needs a `"root"` id (a root parent settling with `None`) is pinned by `move_out_tests`, whose lanes come from the volume objects, so it passes a `"root"` settle id WITHOUT reserving the `"root"` lane.

### Lifecycle status and `operations-changed`

`LifecycleStatus` (Queued/Running/Paused/Done/Cancelled/Failed) lives on the manager record. Admission and settle set Queued/Running and removal-on-terminal; the pause/resume path sets the `Paused`↔`Running` flip (see [Pause / resume](#pause--resume)). It is distinct from `WriteOperationPhase` (the progress phase) and `OperationIntent` (the cancel/rollback machine). The `operations-changed` typed event carries a THIN snapshot (`Vec<OperationSnapshot>`: id, type, status, source/dest summary), emitted from `spawn_managed` / `on_settled` / `cancel_if_queued` / `set_paused`. It deliberately excludes 200 ms progress — the queue window reads the per-file `write-progress` stream for live bars. `init_operation_event_emitter(app)` wires the emitter at startup (`lib.rs`), mirroring `init_busy_volume_emitter`.

### IPC

`list_operations` (the thin snapshot), `cancel_operation(id)`, `cancel_operations(ids)` (the queue window's "Cancel selected"), `pause_operation(id)` / `resume_operation(id)`, and `pause_all` / `resume_all`. Cancel routes through `cancel_operation`: a Queued op is dropped from the registry without ever spawning (`cancel_if_queued`); a Running/Paused op falls through to the existing `cancel_write_operation(id, rollback=false)` keep-partials path. Pause/resume flip BOTH the live `WriteOperationState` pause gate (so the driver parks) AND the manager record's `LifecycleStatus` (so the UI shows Paused), via `set_paused`. Registered in `ipc.rs` + `ipc_collectors.rs`; `OperationSnapshot` / `LifecycleStatus` / `OperationsChanged` ride into `bindings.ts`.

### Pause / resume

The paused bit has TWO homes, kept in sync by the IPC layer: a `PauseGate` on `WriteOperationState` (the runtime gate the drivers honor) and the manager record's `LifecycleStatus::Paused` (what the UI sees in `operations-changed`). Pause is **orthogonal to `OperationIntent`** (which stays the cancel/rollback machine) — it never perturbs the validated `Running → RollingBack/Stopped` transitions — and it is **not a `WriteOperationPhase`** (a paused op may be mid-`Copying`).

- **`PauseGate`** (`operation_intent.rs`): a `paused: AtomicBool` plus a `std::sync::Condvar` (for the sync driver, which parks inside `spawn_blocking`) and a `tokio::sync::Notify` (for the async volume drivers). `pause()` sets the flag; `resume()` clears it and wakes both waiters; `wake()` wakes both WITHOUT clearing (the cancel path uses it). `wait_while_paused_sync(&intent)` / `wait_while_paused_async(&intent).await` park while `paused && !cancelled` and return immediately on cancel.
- **Gate placement** (between-files boundaries, immediately AFTER the `is_cancelled` check so the data-safety ordering — cancel/skip before any destructive call — is preserved): both transfer drivers' per-source loop tops (`transfer_driver.rs`), and the delete-phase loops in both delete walkers (files then dirs, `delete/walker.rs`). The delete SCAN recursion is NOT gated (pausing mid-enumeration would freeze a half-counted "Scanning…"). The cross-volume streaming copy path ALSO parks BETWEEN CHUNKS via the `CheckpointStream` wrapper in `transfer/volume_strategy.rs` (the sync per-chunk `on_progress` callback can't `.await`, so the async stream decorator owns mid-file parking + a `yield_now`), so a paused single large file (e.g. MTP→local) stops mid-stream holding only its `.cmdr-tmp-<uuid>`. The local-FS sync chunk loop (`chunked_copy.rs`) still pauses only between files — it receives the cancel atom, not the `PauseGate`. Full rationale + scope: `transfer/DETAILS.md` § "Pause reaches between chunks".
- **Cancellation always wins over pause.** `cancel_write_operation` / `cancel_all_write_operations` flip the intent AND call `pause_gate.wake()`, so a paused, parked op unblocks, observes the non-`Running` intent, and bails through the existing keep-partials path (keeping already-copied files, deleting only the last partial). Without that wake a paused op parked on the condvar would never see the cancel.
- **A paused Running op keeps its lane slots** (`set_paused` never touches lanes), so a same-lane Queued op can't start and then fight it on resume. Resume runs NO admission pass (the op never freed its lanes). Pausing a Queued op is a v1 no-op (it isn't touching a device yet; it stays Queued and admits normally when its lanes free). Pinned by `manager::tests::{set_paused_flips_running_op_to_paused_and_keeps_its_lane, paused_running_op_does_not_admit_a_queued_same_lane_op}`.
- **Concurrent copy path.** `copy_volumes_with_progress`'s `FuturesUnordered` path has no single between-files boundary, and its per-file `on_progress` callback stays cancel-only (pinned by `transfer_driver::tests::concurrent_per_file_callback_is_cancel_only_not_pause_aware`). But its in-flight files stream through the shared `stream_pipe_file`, so each parks between chunks via `CheckpointStream` when paused; the admission loop adds no new files while everyone is parked, so the batch effectively halts. Serial paths (local copy/move, cross-volume serial, delete) honor pause between files; the cross-volume paths additionally park between chunks. See `transfer/DETAILS.md` § "Pause and the concurrent copy path".
- **Accepted resource asymmetry** (principle 5): `wait_while_paused_sync` parks the op's `spawn_blocking` pool thread for the whole pause — the same thing the deferred-start design avoids for *queued* ops. A paused Running op legitimately holds its lane and is rarer than queued ops, so v1 accepts this; many simultaneously-paused local ops could pressure the blocking pool. v2 may bound concurrent paused-and-parked ops if it proves real.
- **Connection-idle caveat** (document, don't fully solve in v1): a long pause holds SMB/MTP connections idle and may hit server/USB timeouts. v1 accepts that resume may surface a normal transient error (SMB already reconnects; MTP stale-handle has a one-shot retry). v2 adds keep-alive / explicit reconnect-on-resume.

### Existing single-op flow is unchanged

When nothing else touches the op's lanes (the common case), `spawn_managed` admits it on the registration's own admission pass — effectively an immediate spawn. The "register + return `operationId` immediately" contract holds: registration and the id return happen before any I/O, so the dialog opens even on a stalled mount. Every pre-existing write-op test passes through the manager path unchanged.

### Managed instant ops (`run_instant`)

Rename, make-folder, and make-file (`WriteOperationType::Rename` / `CreateFolder` / `CreateFile`) are **scan-free, near-instant, result-returning** metadata ops. They flow through `OperationManager::run_instant(descriptor, op)` instead of `spawn_managed`, so the "every write op goes through `spawn_managed`" framing above applies only to the streaming transfer/delete ops.

- **No lane, no admission queue — deliberate.** `run_instant` registers a `Running` record and marks its volumes busy, but reserves NO lane and runs NO admission pass. Lanes exist to stop two big *transfers* thrashing one device; a metadata syscall must never queue behind a multi-minute copy. An inline rename that hangs until its IPC timeout is worse than useless, and the MTP/SMB connection layer already serializes physical device access. It even ignores any lanes in its own descriptor (pinned by `manager::tests::run_instant_does_not_reserve_a_lane`). **Don't "clean this up" into `spawn_managed`** — that silently reintroduces lane-queuing for metadata syscalls, the regression this design forbids.
- **Runs inline and returns the op's result.** Unlike the fire-and-forget spawn path, `run_instant` awaits `op` inline and returns its `T` to the caller. The inline-rename editor and the new-file/new-folder dialogs need the result synchronously (new path for cursor placement / editor-open; conflict/timeout/success for the rename flow). The command layer wraps `run_instant` in its own IPC timeout; nothing inside spawns. Instant ops emit no `write-progress` / `write-complete` / `write-error` (the command return is the result channel) and no completion analytics (explicit no-op arms in `analytics::emit_completion_analytics`).
- **RAII cleanup on drop/panic is mandatory, not happy-path only.** The command wraps `run_instant` in a `tokio::time::timeout`, so a slow op that exceeds it makes the timeout **drop the `run_instant` future mid-`op.await`**; the async volume path can also panic. Either exit MUST still free the record AND unregister the busy status — else the eject guard sticks ON forever (the volume can never be ejected again) and a phantom `Running` row lingers. An `InstantTaskGuard` held across the `op.await` guarantees this: its `Drop` calls `free_and_remove` (record removal + `unregister_operation_status` → `recompute_and_emit_busy_volumes`) and re-emits `operations-changed`. The happy path calls `free_and_remove` + `emit_changed` explicitly, then `guard.disarm()`s so the Drop is a no-op. No admission pass on completion (instant ops reserve no lanes, so nothing waits on them). Pinned by `manager::tests::run_instant_releases_busy_and_record_when_{dropped_midflight,op_panics}`.
- **No `WriteOperationState`.** Instant ops have no intent/pause/conflict oneshot, so `run_instant` inserts none. Consequence: `cancel_operation` on an instant op is a safe no-op — `cancel_if_queued` is false for a Running op, then `cancel_write_operation` finds no state. Acceptable: instant ops finish before a human can cancel.
- **Queue surfacing.** They appear as a `Running` snapshot row that goes away almost immediately (the store prunes terminal/removed rows). A ~50 ms local rename may never render before it's pruned; a slow MTP rename shows a label + spinner with no progress bar (`fraction` is null). Local `root` ops cause NO busy-set churn (`root` is excluded), so inline-renaming local files won't flicker the eject menu; only volume ops mark busy.

## Archive edits

Editing a `.zip` (mkdir/mkfile/rename/delete inside, or copy/move INTO one) is an O(archive) temp+rename rewrite, not a
metadata syscall, so it runs as a managed op through `spawn_managed`, NOT `run_instant`. The `archive_edit/` module is the driver;
the mutation mechanism (`ArchiveMutator`, temp+rename safe-overwrite) lives in the archive backend
(`volume/backends/archive/mutation/DETAILS.md`).

### Reaching the edit driver: parent-aware write-routing

A write only reaches this driver if the routing seam DETECTS its target as archive-inner. That detection MUST be
parent-aware, not `std::fs`-only: the sync `archive::path_is_inside_archive` / `path_crosses_archive_boundary`
predicates confirm a `.zip` via `std::fs::metadata` + a local magic read, which silently returns FALSE for an
`smb://` / `mtp://` path — so a write inside a remote zip would fall through to a plain parent-volume write and error
confusingly (data-safe, but wrong). So the routing seams call the async `VolumeManager::path_is_inside_archive`
(delete `mod.rs`, rename `rename.rs`, copy-out / move-out source `commands/file_system/volume_copy.rs::resolve_source`
and the scan-preview source) and `path_crosses_archive_boundary` (create `create.rs`), which confirm through the
parent's OWN `get_metadata` + four-byte `read_range` for a remote parent (mirroring `VolumeManager::resolve`) and keep
the zero-network `std::fs` fast path for a local one. Copy/move INTO already routed correctly (the dest goes through
the async `resolve` → `dest_resolved.is_archive`). The `route_*` functions then re-split the confirmed path with the
pure-string `archive_boundary_candidate` (NOT `confirm_archive_boundary`, whose `std::fs` confirm would wrongly fail
for a remote zip) — confirmation already happened at the seam. Pinned by the `path_is_inside_archive_*` unit tests in
`volume/manager.rs` (local + remote + `read_range`-unsupported + mislabeled).

### Local vs remote: one closure, one dispatcher (`run_managed_edit`)

Every apply site in `archive_edit/` runs its plan+apply through `engine::run_managed_edit(parent_volume_id, archive_path,
state, plan_and_apply)` rather than a bare `spawn_blocking(mutator::apply(...))`. The closure is the SAME blocking
plan+apply either way — it plans against, and mutates, the path it's HANDED. The dispatcher (keyed on
`parent.supports_local_fs_access()`) decides what that path is:

- **Local parent**: byte-identical to before — the closure runs on the REAL archive file, and the mutator's own
  temp+rename commits the edit. No pull, no upload.
- **Remote parent** (direct SMB / MTP): routed through `archive_remote_edit::pull_apply_upload_swap`.

Because the local mutator's `raw_copy_file` needs a `Read + Seek` source (which async ranged reads can't give), a remote
edit does NOT edit in place — it PULLS the `.zip` to a local temp, runs the ordinary local closure there, uploads the
rewritten temp under a remote temp name, then swaps. This means a remote edit needs only streaming read + write + rename
+ delete on the parent; it does NOT depend on the SMB positioned-read (`read_range`) primitive that BROWSING needs (the
CD is parsed from the pulled-local copy, not over ranged reads).

### Remote edit: the data-safety contract (`archive_remote_edit.rs`)

The remote ORIGINAL is byte-for-byte untouched until the very last swap:

1. **Pull** streams the remote `.zip` to a local scratch copy (`open_read_stream`, cancel-checked between chunks,
   `fsync`ed). Writes nothing remote.
2. **Apply** runs the closure on the local copy — the mutator's temp+rename commits onto the scratch file. A cancel/fault
   leaves the scratch file as the pulled original; nothing remote changed.
3. **Upload** streams the edited copy to a NEW remote name (`foo.zip.cmdr-tmp-<uuid>`) via `write_from_stream`; the
   original keeps its name and bytes. A cancel/fault deletes the partial temp best-effort.
4. **Swap** is the ONLY step that changes the original. Where the backend REJECTS a same-name collision
   (`create_directory_errors_on_existing_dir()` true — SMB, local), it tries an atomic rename-overwrite first (SMB with
   `ReplaceIfExists`); on refusal it falls back to delete-then-rename. A backend that ALLOWS same-name siblings (MTP,
   flag false) goes STRAIGHT to delete-then-rename — a rename onto the live name would DUPLICATE, not replace. The
   delete-then-rename path has exactly ONE crash window (between the delete and the rename): the NEW, fully-uploaded data
   survives under the temp name — never lost, only briefly misnamed.

A cancel at ANY point before the swap completes leaves the remote original intact (the local scratch dir and any partial
remote temp are cleaned up — a RAII `ScratchDir` and the upload's on-error delete). Pinned by `archive_remote_edit_tests`
(round-trip, cancel-before-swap-leaves-the-original, and the sibling-allowing delete-then-rename swap), plus live-remote
integration proofs that drive `pull_apply_upload_swap` against a REAL backend: `smb_integration_test`
(`smb_integration_remote_zip_edit_deletes_an_entry_through_the_share` + `..._cancel_before_swap_keeps_original`, and
routing detection + extract-out in `smb_integration_archive_routing_detection_and_extract_out`) and `mtp_test` under the
`virtual-mtp` feature (`virtual_mtp_archive_browses_and_extracts_via_read_range` +
`virtual_mtp_remote_zip_edit_deletes_an_entry_through_the_device`, exercising the MTP delete-then-rename swap). Cost: O(archive)
network per edit (the pull), documented and accepted — there is no remote random-access WRITE adapter (that's only a
future in-place-append optimization). Remote backends don't carry the archive file's mode/mtime/xattr across the rewrite
the way local `copyfile` does; the upload mints a fresh remote object.

**Stale upload-temp reaping.** A crash or kill in the swap's ONE window (between the upload finishing and the swap
committing) can leave the fully-uploaded temp on the remote under its `<archive>.cmdr-tmp-<uuid>` name. It's harmless
(the original is intact and the temp holds the NEW bytes), but untidy. `pull_apply_upload_swap` reaps it at the start of
the next edit of the SAME remote archive — the mirror of the local mutator's `reap_sibling_temps` — via a single
`list_directory` of the archive's parent, deleting siblings that match this archive's own temp shape. Best-effort and
non-blocking (a listing/delete failure is logged at debug, never fails or delays the edit); one round-trip, nothing on
the read path. Pinned by the four `remote_edit_*` reap tests in `archive_remote_edit_tests` (stale-same-archive reaped,
fresh spared, other-archive ignored, delete-failure doesn't fail the edit).

- **Decision — age-gate the remote reap at 24 h (`REMOTE_TEMP_REAP_MIN_AGE`); the local reap has no threshold.** The
  local reap deletes every matching sibling unconditionally because edits of one archive serialize on the parent lane, so
  a local leftover is ALWAYS an abandoned build. A remote share is multi-machine: a `<archive>.cmdr-tmp-*` sibling with
  this exact shape may be a LIVE upload from ANOTHER Cmdr instance mid-flight, so the remote reap deletes only leftovers
  whose reported mtime is older than 24 h (an entry with no mtime is treated as fresh and spared). Why 24 h: it must
  comfortably exceed the longest plausible single-archive upload (tens of GB over a slow link still finishes in well under
  a day) PLUS clock skew between this machine and the remote's mtime clock (SMB reports server mtime, MTP the device's;
  the dangerous direction is a server clock BEHIND local, which inflates the computed age). The leftover is harmless while
  it waits and gets cleaned lazily at a later edit, so erring long costs almost nothing; erring short risks deleting a
  legitimate in-flight upload. Consequence, accepted: a crash-then-immediate-retry of the same archive leaves the leftover
  in place until an edit more than 24 h after the crash — mtime alone can't tell "my own crash seconds ago" from "another
  instance uploading now."

- **Driver shape.** `archive_edit_start(events, request, interval)` mirrors the volume-delete branch: a deferred async
  start owns the op end to end (a `WriteSettledGuard`, the `ArchiveMutator` run on the blocking pool, the terminal
  event, `on_settled`). The op takes the PARENT drive's lane (archive work shares the device's serialization lane) and
  marks the parent drive busy (eject guard). A `MutatorHooks` bridge wires the mutator's control seam to the live op:
  cancel from `OperationIntent`, pause from the `PauseGate` (a sync park on the blocking thread), throttled
  `write-progress` (two-axis: entries + bytes), and the downloads-watcher ignore registration for the temp AND final
  paths (before each syscall, via the mutator's `note_pending` hook). `Cancelled` emits `write-cancelled`, never
  `write-error`; other mutator faults map to typed `WriteOperationError`. **The terminal `files_processed` is
  `MutationProgress::entries_changed`** (entries the edit adds / deletes / renames), NOT `entries_total` (the
  retained-rewrite count) — deleting one file from a 3-entry zip reports 1, not 2.
- **Routing seams.** The former archive rejections become routing: `create_directory_managed` / `create_file_managed`
  (a `.zip`-crossing parent), `rename_managed` (an in-archive path), `delete_files_start` (in-archive sources), and the
  `copy`/`move_between_volumes` COMMANDS (an archive-resolved destination). The instant-op forks reach a `TauriEventSink`
  via the manager's startup-wired app handle (`operations_app_handle`), so no command signature changes; a
  `create`/`rename` return is the operation id, not a path (the FE reads it as an op handle).
- **Changeset per op.** mkdir → `{ mkdir }`; mkfile → `{ add }` (empty bytes); rename inside → `{ rename }`; delete
  inside → `{ delete }` (batched across a multi-select in one zip); copy/move INTO → one `{ add + mkdir }` for the whole
  transfer (`route_archive_copy_into` walks the LOCAL sources with `walkdir`). A move INTO deletes the top-level sources
  after the commit, and only when nothing was skipped (the move invariant — never delete a source whose bytes didn't
  land): local sources go straight off the FS, remote ones through the source volume (recursive for trees).
- **Compress = seed an empty zip, then copy-into** (`archive_edit/compress.rs`, `compress_start`). Creating a NEW zip and packing the sources into it IS an archive edit, so compress is built ON copy-into rather than as a parallel path: `seed_empty_zip` writes a valid empty archive at the target, then `compress_start` calls `route_archive_copy_into` with `is_move = false`. The seed is the ONLY net-new backend surface — scan, plan-in-closure, progress/ETA, cancel, lane admission, and the mutator's temp+rename durability are all inherited. **The seed is LOAD-BEARING**: `route_archive_copy_into` (and the mutator) open the target with `ZipArchive::new`, which rejects a 0-byte file (`ZipError::InvalidArchive`) — so a brand-new target must already be a valid archive before the copy-into runs. `seed_empty_zip` writes the 22-byte bare end-of-central-directory record (`PK\x05\x06` + 18 zero bytes) — the minimal valid zip, a zero-entry archive that `ZipArchive::new` opens with `len() == 0` and whose first bytes pass `bytes_start_with_zip_signature`. It uses the SAME temp+rename discipline as the mutator (build a `.cmdr-tmp-<uuid>` sibling, fsync, atomic rename over the target, fsync the parent dir), so a crash mid-seed never leaves a torn file and an overwrite is atomic. **Seed matches the parent, local or remote.** `route_archive_copy_into`'s remote path PULLS the existing `.zip` before editing (see the remote-edit contract above), so a local-FS seed would be invisible to a remote parent — the seed must land wherever the copy-into will look for it. So `compress_start` branches on `parent.supports_local_fs_access()`: a LOCAL parent gets the local-FS `seed_empty_zip`; a REMOTE parent (SMB / MTP) gets `seed_empty_zip_remote`, which stages the 22 bytes in a scratch file and places them THROUGH the parent volume via `archive_remote_edit::place_local_file` (the remote edit's own upload-to-temp + atomic-swap commit, generalized to tolerate a MISSING original for a brand-new target). Then the copy-into pulls the seed, adds the sources, and swaps the full archive in. The remote path composes for both swap shapes: SMB's atomic rename-replace and MTP's delete-then-rename (same-name siblings allowed) — MTP needs no compress-specific work beyond the shared remote-edit machinery. **Remote cancel-safety** is inherited, not re-earned: the seed is placed atomically, and a cancel/fault during the copy-into leaves at worst the valid empty seed at the target (`place_local_file` reuses `pull_apply_upload_swap`'s swap, so the target keeps its bytes until the final atomic swap, and any partial upload temp is deleted). `compress_start` reuses `WriteOperationType::ArchiveEdit` (compress has no distinct backend op type — its identity is frontend-only). Pinned by `compress_tests` (local seed validity + atomic overwrite, end-to-end compress of local files and a directory subtree; the seed's load-bearing role is shown by the copy-into failing against a 0-byte target), `compress_remote_tests` (seed-through-volume onto a non-local `InMemoryVolume` for both swap shapes, plus overwrite-replaces-not-merges), and the live-Samba `smb_integration_compress_local_files_onto_the_share`.
- **Compression level threads from the op config onto the changeset.** `VolumeCopyConfig::compression_level` (frontend-owned, read from the `behavior.archiveCompressionLevel` setting at dispatch) is passed through `compress_start` / `route_archive_copy_into` as an `Option<i64>` param and stored on the `Changeset` (`archive_copy_into_start` sets `plan.changeset.compression_level` before `mutator::apply`). It governs every user-driven zip write uniformly — compress AND copy/move INTO an existing archive — because both funnel through the shared mutator. `None` (no caller opinion, or a non-archive copy) means the crate default (level 6). The level applies to NEWLY added entries only and is clamped 1..=9; the mechanism and the clamp rationale are single-sourced in [`../volume/backends/archive/mutation/DETAILS.md`](../volume/backends/archive/mutation/DETAILS.md) § "Compression level applies to ADDED entries only". Internal zips (crash/error-report bundles) keep their own fixed level and never read this setting.
- **Source-side pull for a REMOTE source (SMB / MTP → zip).** A copy/move INTO a zip whose SOURCE volume has no
  `local_path()` can't be walked with `std::fs`, so `archive_copy_into_start` runs a pull stage FIRST, inside the op: it
  streams each source subtree into a `ScratchDir` via the copy engine's `pull_path_to_local` seam (which reuses
  `copy_single_path` — nested-tree recursion, chunked streaming, cancel, pause), then the ordinary changeset walk + apply
  runs against the pulled bytes. This is ORTHOGONAL to the archive PARENT's local-vs-remote handling (`run_managed_edit`),
  so all four source×parent combinations work. The pull is SILENT (no progress events); the rewrite stage drives the
  progress bar, matching the remote-PARENT flow. The metadata size is never trusted — the pull streams the real bytes, so
  a source whose listed size lies still lands correct content. A cancel or fault during the pull returns before
  `run_managed_edit` opens the archive, so the zip stays byte-for-byte intact; the `ScratchDir` (shared with the
  remote-edit flow, `scratch_dir.rs`) is cleaned on every exit. Pinned by the remote-source `copy_into_tests`.
- **Duplicate pre-check for create / rename** (`archive_inner_exists`). `route_archive_create` and
  `route_archive_rename` reject a name that already exists inside the zip UP FRONT with the same friendly "already
  exists" message the real-FS mkdir/rename paths use, so the FE shows the standard copy — the mutator otherwise only
  rejects a duplicate at write time (`zip`'s `Duplicate filename`), after building a temp. It dispatches on the parent
  like `run_managed_edit`: a LOCAL (or unregistered) parent parses the central directory straight off the real file
  (off-executor), a REMOTE parent reads it through the parent volume (a ranged tail read via `resolve`, not a full pull).
  A parse failure resolves to "not a duplicate" so the managed op still surfaces the real fault. Copy/move-INTO conflicts
  are handled by the policy layer below, not this pre-check.
- **Unrepresentable source entries are skipped, never lost (data safety).** A zip changeset can only carry real files
  and directories. When `route_archive_copy_into` walks the sources, any entry that's a symlink or special file
  (fifo/socket/device — including a broken symlink, since `symlink_metadata` classifies it as neither file nor dir) is
  counted as skipped rather than added. On a MOVE, any skip suppresses the source deletion (all-or-nothing — the whole
  transfer degrades to a copy, so a symlink is never removed from the source while absent from the archive). The skip
  count rides in `ArchiveEditRequest.skipped_count` and surfaces as `files_skipped` on the terminal event.
- **Move OUT of a zip is a compound op** (`route_archive_move_out`), NOT a per-file `Volume::delete` (the `ArchiveVolume`
  is read-only). One managed Move op runs two phases on ONE lifecycle: (1) extract the selected entries to the
  destination through the ordinary cross-volume copy engine (`copy_volumes_with_progress`, wrapped in a
  `SuppressTerminalsSink` that withholds the copy's terminal event so the compound op emits the single Move terminal,
  reads `files_skipped`, and collects the fully-extracted sources via `note_source_landed_clean`); (2) a batch
  `{ delete }` archive rewrite via the mutator. **MOVE INVARIANT**: an entry is deleted ONLY after its destination copy
  is durably committed (the copy engine fsyncs each file) AND won't be rolled back, so a crash or cancel never loses both
  copies. **Partial-move policy: per-source convergence.** The batch drops exactly the top-level sources that extracted
  with ZERO deep skips: a source with a skipped child stays in the archive (deleting its subtree would drop the un-landed
  child — the partial-merge-skip hazard); a HARD error deletes the durable PREFIX so a retry moves only the remainder;
  CANCEL and ROLLBACK delete nothing (cancel matches the plain cross-volume move, whose source-delete never runs on
  cancel; rollback removes the dest copies, so nothing durable remains). The delete stays ONE atomic O(archive) rewrite
  over the converged subset (a dir source deletes by prefix), never n per-entry rewrites. **The deep-skip count is
  load-bearing**: a merge child resolved to Skip is invisible to the driver's top-level accounting, so the copy engine
  folds each source's `CreatedPaths::skipped_file_count` into `files_skipped`; without that fold a directory source with
  a deep skip would report zero skips and the delete would drop its whole subtree (data loss). Progress is two honest
  phases (extract bytes, then rewrite bytes). Pinned by the `move_out_*` tests (incl. the deep-skipped-child,
  partial-converge, durable-prefix-on-error, and rollback pins).
- **Conflicts.** An add whose inner path already exists is resolved against the archive index. BOTH the pre-resolved
  policies and Stop PLAN inside the managed op (`archive_copy_into_start`), against the working copy `run_managed_edit`
  hands the closure — the real archive for a LOCAL parent, the pulled-local copy for a REMOTE one. Planning up front
  against the archive path would break a REMOTE edit (`LocalFileSource::open` on a direct-SMB / MTP path fails, or opens
  the OS mount the design routes around); planning inside the op is what keeps a remote plan on the pulled bytes. A
  pre-resolved policy resolves each collision non-interactively (`build_copy_into_changeset`): Skip drops the add;
  Overwrite deletes the existing entry then adds (a clean replace); Rename picks a unique ` (n)` name;
  OverwriteSmaller/Older compare size/mtime (strict). **The Stop policy prompts interactively**
  (`build_copy_into_changeset_interactive`): the op is registered so `resolve_write_conflict(op_id)` can reach the
  oneshot, and each FILE collision emits a `write-conflict` and blocks on the answer, reusing the pure `ApplyToAll` latch
  + the oneshot plumbing (store the sender BEFORE the emit). Dir-vs-dir collisions merge silently — only files prompt
  (the app-wide rule). A cancel during a pending prompt drops the sender → the planner bails → the archive is untouched.
  Every Skip (a conflict resolved to
  Skip, a conditional policy that declines to overwrite, or an unrepresentable entry) increments the plan's
  `skipped_count`, which gates the move-source deletion and surfaces as `files_skipped` on the terminal event. Pinned by
  the `interactive_*` tests.
- **Mutation-test coverage (`cargo mutants` on `archive_edit/`).** Every conflict-resolution and routing/data-path
  mutant is killed (Rename numbering incl. dotfiles, OverwriteSmaller/Older strict `<` incl. the equal-size/mtime
  boundary, move-source deletion gating, per-source move-out convergence (deep-skip count, durable-prefix delete), dir-merge mkdir guard, settle payloads). The only
  deliberately-unkilled survivors are in `MutatorHooks` — progress-emit THROTTLING, pause parking, and the
  cancel-during-rewrite bridge. These are UX/timing, data-safe by construction (the mutator's own cancel-abandons-temp
  and progress semantics are pinned in `backends/archive/mutator_test.rs`), and killing them would need flaky
  timing-based tests — not worth it per the mutation-score guidance.

## Busy-volumes set

Drives "disable Eject while an op reads from / writes to this device" so a disconnect can't truncate an in-flight file. Lives in `state.rs`.

- The manager registers an op's volume IDs busy (`register_operation_status(op_id, type, volume_ids)`) **only when it admits the op (Running)** — a Queued op isn't touching the device, so it marks nothing busy. Source **and** destination go in (a download from a phone is as corruptible as an upload to it). The manager's `on_settled` / `ManagedTaskGuard` Drop unregisters on every exit (including panic), so a finished or panicking op can't leave a volume stuck busy.
- The busy set is the union of every Running op's `volume_ids` **∪ external registrations**, minus `root` (never ejectable). `recompute_and_emit_busy_volumes` fires `volumes-busy-changed` only when membership changes — progress ticks don't churn it (`LAST_EMITTED_BUSY`). Membership-by-union means two concurrent transfers to one device keep it busy until both finish, with no manual refcount.
- **Where `volume_ids` come from**: the `OperationDescriptor` each spawn site hands the manager. The cross-volume entry points (`copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`) and the volume-aware delete carry the IDs; the both-local branch of `copy_between_volumes` (a local→USB / DMG copy) passes both IDs through `copy_files_start` / `move_files_start` so the ejectable destination is still marked. The plain `copy_files` / `move_files` / `trash` commands pass an empty list — the unified transfer dialog only routes through them for same-`root` ops, where no ejectable volume is involved.
- **Consumers**: `busy_volume_ids()` backs the `get_busy_volume_ids` bootstrap command, the `eject_volume` server-side guard (refuses a busy volume — the real safety net, since the picker's disable is only UX), and the native breadcrumb-menu builder (renders the Eject item disabled with a ` (busy)` suffix). The frontend `volume-busy-store.svelte.ts` subscribes to `volumes-busy-changed` and exposes `isVolumeBusy(id)` to disable the picker's eject controls. `init_busy_volume_emitter(app)` wires the emitter at startup (`lib.rs`).
- **External (non-write-op) seam**: the drag-out file-promise fulfillment service (`native_drag::fulfillment`) marks the source volume busy while it streams a promise to a Finder destination, but it isn't a real write op (no `WRITE_OPERATION_STATE`, no progress events, no settle). The `pub(crate)` `register_external_volume_op(op_id, volume_ids)` / `release_external_volume_op(op_id)` pair (in `state.rs`, re-exported from `mod.rs`) is the seam: it touches only the `OPERATION_STATUS_CACHE` half that `recompute_and_emit_busy_volumes` reads, registering under `WriteOperationType::Copy` (the type only affects `list_active_operations` diagnostics; the busy set is type-agnostic). The fulfillment side wraps it in an RAII guard so release fires on every exit path.

## Settle contract

`write-settled` fires exactly once per operation, after the spawned background task has fully torn down — including in-flight USB / network teardown that may briefly outlive the `write-cancelled` emit. The FE uses it to gate the "Cancelling…" dialog close so the user can't dispatch a new op against a still-tearing-down volume (the wedge mode that cancel propagation already shortens but doesn't eliminate).

**Ordering**: `write-settled` always fires AFTER the terminal outcome event (`write-complete` / `write-cancelled` / `write-error`) for the same `operation_id`. The BE guarantees this by placing the settle emit in a `WriteSettledGuard` RAII struct whose `Drop` runs at the very end of the spawn-task scope, AFTER all the conditional terminal-event emits.

**Guard pattern**: every op's deferred start (the future the manager spawns from each of the five entry points) constructs a `WriteSettledGuard` at the top, from the same injected `Arc<dyn OperationEventSink>` the rest of the op emits through. The guard's `Drop` impl calls `sink.emit_settled(...)`. This makes the emit panic-safe: even if the op body panics and the task exits via `JoinError`, the guard still drops during stack unwinding, so the FE never hangs waiting for a settle that never comes. `emit_settled` is a required `OperationEventSink` method (no default no-op), so a new sink can't silently swallow settle. See `settle_event_tests.rs::settled_fires_on_panic_unwind` for the safety-net pin.

**Cache-cleanup panic safety**: removal from `WRITE_OPERATION_STATE` + `OPERATION_STATUS_CACHE` must survive a panic, or the op lingers forever in `list_active_operations`. The manager owns this: `on_settled` removes both maps on the happy path, and the `ManagedTaskGuard` Drop (held by every spawned task, declared so it drops AFTER the `WriteSettledGuard`'s scope cleanup runs but frees caches before the settle emit) does it on unwind. The guard NEVER spawns in Drop — see [Operation manager](#operation-manager) § "Dequeue on settle". Pinned by `manager::tests::panicking_op_releases_its_lane_without_spawning_next`.

**Payload**: `{ operationId: String, operationType, volumeId: Option<String> }`. The `volume_id` is best-effort: filled with the source volume's display name for volume-aware ops (copy/move between volumes, volume delete), `None` for pure local-FS operations. The FE currently filters only by `operationId`; `volume_id` is for diagnostics and forward compatibility.

**Tests**: `settle_event_tests.rs` pins the guard's invariants (single fire, panic safety, ordering relative to the terminal event). `delete/volume_cancel_tests::volume_*_emits_write_settled_event` pin the integration shape against the volume-delete handler.

## Key decisions (shared)

**Decision**: Copy and cross-FS move pre-flight a destination per-file-size limit (FAT32's 4 GiB cap) right after the scan, before the first byte. `validation::validate_file_sizes_for_filesystem` classifies the destination via `crate::file_system::filesystem_kind` (macOS `statfs.f_fstypename` / Linux `/proc/mounts` → `FilesystemKind` → `MaxFileSize`) and, only when the cap is `Limited`, fails the whole operation with `WriteOperationError::FilesTooLargeForFilesystem` (up to 10 offenders, largest first, plus the true count).
**Why**: A FAT32 USB stick silently failed a 5 GB copy ~4 GB in. The gate is all-or-nothing and runs alongside the free-space check (`copy/mod.rs`, `move_op.rs::move_with_staging`). It blocks **only** when certain: `Unlimited` (APFS/exFAT/NTFS/ext4/MTP) and `Unknown` (OS-mounted SMB, unrecognized) never block, so a false positive — worse than the mid-copy failure because it stops a copy that would have succeeded — can't happen. **exFAT must stay `Unlimited`** (it's the common big-USB format with no 4 GiB cap); only FAT32 (`msdos`/`vfat`) is `Limited`. Same-FS moves rename in place and never reach the gate. The kind → cap map in `filesystem_kind::FilesystemKind::max_file_size` is the single source of truth (the write guard, the error prose, and any future volume-picker display all read it). SMB FileSystemName detection (a `smb2`-crate `FileFsAttributeInformation` query) and the volume-picker filesystem display are scoped follow-ups.

**Decision**: Every scan reports **two** byte totals — `total_bytes` (write footprint, un-dedup'd) and `dedup_bytes` (`du`-equivalent, each inode once). Delete consumes `dedup_bytes`; copy/move consume `total_bytes`; the Copy dialog shows both.
**Why**: A hardlink contributes differently to the two operations. **Delete** frees an inode only when its last link is removed, so the bytes-freed number is the dedup'd one — counting every link would claim to free 80 GB when only 60 GB (cargo `target/`) actually frees. **Copy/move** materialize every hardlink as an independent file at the destination (hardlinks don't survive a cross-volume copy, and even a same-FS `cp` doesn't relink), so the bytes-written number — and the disk-space reservation — is the full write footprint. The earlier single-`total_bytes`-is-dedup'd design got delete right but silently regressed copy: the space check under-reserved (risking ENOSPC mid-copy) and the bar hit 100% early. Now `walk_dir_recursive` / `walk_cached_entries` / `scan_volume_recursive` / `LocalPosixVolume::scan_for_copy` / `scan_subtree_with_oracle` all track both, using a `seen_inodes: HashSet<u64>` (mirrors `indexing/scanner.rs`, `nlink == 1` fast path, operation-scoped across source roots; **Unix-only**, where non-Unix has no `nlink()` so `dedup_bytes == total_bytes`). Volume backends populate `FileEntry::inode` only for `LocalPosixVolume` files with `nlink > 1` (MTP/SMB/InMemory leave it `None`, so dedup is a no-op and the two totals are equal). The **scan-phase** progress bar reports the dedup'd running total (it's compared against the indexer's inode-dedup'd `dir_stats` estimate, so reporting the write footprint would overshoot 100% on hardlink trees). The **delete** active phase sums per-entry `progress_bytes`/`VolumeDeleteEntry::progress_bytes` (= dedup'd) against the `dedup_bytes` denominator. The **copy** active phase credits full per-file `size` against the `total_bytes` denominator (no chunk scaling). The Copy dialog surfaces the gap with a one-line note ("X will be written; source is Y; the extra is hardlinked files…") via `dedup_bytes_total` on the scan-preview events — copy-only, since a same-FS move writes nothing. Pinned by `delete/hardlink_progress_tests.rs`, `delete/volume_hardlink_progress_tests.rs`, `transfer/hardlink_progress_tests.rs::copy_counts_write_footprint_for_hardlinks`, `scan.rs::tests::walker_dedupes_*`, `local_posix_test::test_scan_for_copy_dedupes_hardlinks_for_source_size_only`, and `transfer-dialog-utils.test.ts::shouldShowHardlinkNote`.

**Decision**: `WriteProgressEvent::with_scan_meta` is the only path that sets the scan-only fields (`current_dir`, `dirs_done`, `expected_files_total`, `expected_bytes_total`).
**Why**: 20+ emit sites construct `WriteProgressEvent` literals for active-phase events. Adding four optional fields to the struct would force every site to spell out their defaults, pure mechanical noise. The `new(...)` constructor takes the eight core counter fields and defaults the scan meta (`None` / `0`); the scan emit sites in `scan.rs`, `scan_preview.rs`, and `delete/walker.rs::scan_volume_recursive` opt in via `.with_scan_meta(current_dir, dirs_done, expected)`. Future scan-related fields go through the same builder. If a real refactor of the 20 literals to `new(...)` ever happens, the builder pattern still composes cleanly on top.

**Decision**: All write operations go through `OperationEventSink` instead of `tauri::AppHandle`, and the sink is constructed **only at the IPC edge** (`commands/file_system/write_ops.rs` + `commands/file_system/volume_copy.rs`), then injected all the way down.
**Why**: Decouples the copy/move/delete/trash orchestration from the Tauri framework. `TauriEventSink` wraps AppHandle for production; `CollectorEventSink` stores events for test assertions. The whole managed layer — `start_write_operation`, the four starters, the volume entry points (`copy_between_volumes` / `move_between_volumes` / `move_within_same_volume`), every `*_with_progress` function, and `WriteSettledGuard` — takes `&dyn OperationEventSink` / `Arc<dyn OperationEventSink>`, never an `AppHandle`. Each command builds `Arc::new(TauriEventSink::new(app))` once and passes it in (grep confirms zero `TauriEventSink::new` under `write_operations/`). This lets the full pipeline (multi-file copy, cancellation, conflict resolution, progress, the managed spawn path, and settle) run end-to-end under a `CollectorEventSink` with no Tauri runtime — see `tests.rs::injected_sink_receives_complete_and_settled_for_local_copy` and the trash unit tests (`delete/trash.rs::tests::trash_*_via_sink`). `state.emit_progress_via_sink` is the only progress-emit method — `emit_progress_via_app` is gone. The write-error safety-net arms in each deferred also route through `sink.emit_error(...)` rather than a string-named `app.emit("write-error", ...)`.

**Decision**: Scan preview reuses watched listings (the "fresh-listing oracle").
**Why**: Pre-flight scans for copy/move on MTP (and to a lesser degree SMB and big local trees) used to duplicate work the backend already had in `LISTING_CACHE`. Selecting 135 photos in a watched `/DCIM/Camera` (~15k entries) and pressing F5 would re-list the parent dir over USB just to look up size by name — ~17 s of "Verifying before copy…" while the listing was already fresh on the pane behind the dialog. `run_volume_scan_preview` now groups input sources by parent dir and consults `try_get_watched_listing(volume_id, parent)` first. On hit, sizes and `is_directory` flags come from the cached `FileEntry` for top-level files; top-level directories recurse via `scan_subtree_with_oracle`, which re-applies the oracle at every level (so a subfolder open in another pane also short-circuits). On miss, the call falls through to `volume.scan_for_copy_batch_with_progress(paths_in_group, ...)` — same code path as before — so MTP's parent-grouping and SMB's pipelined-stat optimizations still run for cold-cache parents. The local-FS walker (`walk_dir_recursive` in `scan.rs`) also takes an oracle check at the top of each recursive call, with `volume_id = "root"` plumbed through from `scan_sources_internal` and `run_scan_preview`. The freshness contract is bright-line at the watcher boundary: no "5 seconds is fresh enough" TTL, just "the volume's `listing_is_watched(path)` returned true." See `file_system/listing/caching.rs::try_get_watched_listing` for the per-backend debounce windows that contract tolerates.

**Decision**: Copy and move are durable before they report complete: per-file `sync_data` (fdatasync) in chunked copy, plus an end-of-op targeted `fdatasync` pass over the transaction's recorded destinations for the strategies that don't flush themselves. Delete and trash don't sync at all.
**Why**: "Complete" must mean "durable on disk," not "buffered in the OS page cache." Without it, a user who copies to a USB stick / SD card and ejects (or the machine sleeps) right after "Copy finished" loses the file — and on a move it's gone from both source and dest. The flush is targeted, not a whole-machine `libc::sync()`: that global sync also stalled unrelated apps (AGENTS.md principle #5). The mechanism: (1) `transfer/chunked_copy.rs` calls `dst_file.sync_data()` per file, so each file is durable as it completes — a crash mid-batch on a long transfer leaves earlier files safe. (2) Before emitting `write-complete`, `durability::flush_created_destinations` emits a `Flushing`-phase progress event, then `fdatasync`s every recorded destination that wasn't already flushed, plus a best-effort `fsync` of each distinct parent directory so the rename-into-place (temp+rename / cross-FS staging) is durable too. It reuses `CopyTransaction.created_files` (no parallel dest-tracking) and skips an `already_synced: HashSet` of paths the strategy already made durable: chunked-synced files and APFS-clonefile / reflink dests (those share copy-on-write extents with the source, so a flush is moot). On macOS every produced-bytes path is either clonefile (moot) or chunked (already synced), so the end-of-op pass does no extra `fdatasync` there — its job on macOS is purely the honest `Flushing` UI state; on Linux it's the real flush for `copy_file_range` dests. Cross-FS move flushes the FINAL paths (Phase 3 renames staging → destination, so the staging entries in `created_files` are remapped to their final prefix before the pass — this also covers the Phase-3 `throwaway_tx` renames that aren't in the real transaction). Same-FS move (pure rename) writes no data, so its flush just `fdatasync`s the moved files (cheap) and their parent dirs to make the new directory entries durable. The flush is best-effort on error: a failed `sync_data` is logged (`target: "write_durability"`), not propagated — the bytes are written either way and failing the whole op at the final flush is worse UX. Pinned by `transfer/copy_tests.rs::local_copy_emits_flushing_phase_before_complete` and `transfer/move_op_tests.rs::cross_fs_local_move_emits_flushing_phase_before_complete`; FE label by `TransferProgressDialog.flushing.test.ts`. **Cross-volume copy/move landing on a local disk** (MTP → Local, SMB → Local, USB import) doesn't go through this local-FS engine — it flows through `LocalPosixVolume::write_from_stream`, which keeps the same promise by `sync_data`-ing each file (plus a best-effort parent-dir fsync for the directory entry) before it returns, so each file is durable as it completes. That path doesn't yet emit the `Flushing` UI phase (the volume copy/move handlers don't call `flush_created_destinations`); a follow-up could route them through the end-of-op pass for UI consistency, but the per-file `sync_data` already makes them durable.

**Decision**: `state.rs` re-exports the `operation_intent` + `scan_cache` types and `types.rs` re-exports the `event_sinks` types + `error_classification::IoResultExt`, so `state::…` / `types::…` paths resolve for callers. These re-export facades are kept deliberately, not collapsed into direct `scan_cache::…` / `error_classification::…` imports.
**Why**: Every one of the four re-exported groups has a broad consumer surface once grouped `use` blocks are counted: `operation_intent` at ~35 sites across ~20 files (every cancellation check), `event_sinks` at ~11 sites (every progress emit), `IoResultExt` across seven copy/scan/delete backends, and the `scan_cache` types across `scan.rs`, `scan_preview.rs`, `validation.rs`, and two test files. Collapsing any of them is a touch-many-files churn (~12 files for the two smaller ones alone) with no behavior or clarity payoff — a facade fronting a high-traffic name surface is a legitimate shape here, so leave them. If a future split genuinely narrows one group's consumers, revisit then.

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
