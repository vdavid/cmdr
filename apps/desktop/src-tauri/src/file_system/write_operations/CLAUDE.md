# Write operations

Copy, move, delete, and trash with streaming progress, cancellation, conflict resolution, and rollback. macOS and Linux.

## Purpose

Implements the four destructive file operations as background tasks that stream Tauri events to the frontend. Every
operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites,
network mounts, cross-filesystem moves, and name/path length limits.

## Files

| File | Responsibility |
|------|----------------|
| `mod.rs` | Public API: `copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start`. Each delegates to `start_write_operation` which handles state creation, spawn lifecycle, cleanup, and error/panic recovery. Validation runs inside the handler closure on the blocking thread pool, never on the async executor. |
| `types.rs` | All serializable types: events, config, errors, results. `WriteOperationConfig`, `ConflictResolution`, `WriteOperationError`, `DryRunResult`, scan preview events. Also: `OperationEventSink` trait (decouples event emission from `tauri::AppHandle`), `TauriEventSink` (production), `CollectorEventSink` (test-only). |
| `state.rs` | Two `LazyLock<RwLock<HashMap>>` caches (`WRITE_OPERATION_STATE`, `OPERATION_STATUS_CACHE`). `WriteOperationState`, `CopyTransaction`, `ScanResult`, `FileInfo`. |
| `helpers.rs` | Validation (`validate_sources`, `validate_destination_writable` via `libc::access`, `validate_disk_space` via `statvfs`). Conflict resolution (`tokio::sync::oneshot` channel wait for Stop mode). `safe_overwrite_file`/`safe_overwrite_dir` (temp+rename). `find_unique_name`. `run_cancellable`. `is_same_filesystem` (device IDs). Background cleanup helpers: `remove_file_in_background`, `remove_dir_all_in_background`. |
| `scan.rs` | `scan_sources` (recursive walk, emits progress), `dry_run_scan`, shared `walk_dir_recursive` walker. The `on_progress` callback receives `(files, dirs, bytes, current_file, current_dir)`; the walker reads `current_dir` from `path.parent()` so the UI can show "in directory: …" alongside the filename. Scan emit sites populate `WriteProgressEvent.current_dir` plus index-derived `expected_files_total` / `expected_bytes_total` (via `WriteProgressEvent::with_scan_meta`) so the frontend renders a real progress bar during the foolproof re-scan. Expected totals come from `crate::indexing::expected_totals::expected_totals_for_sources` (`None` when the index doesn't cover all sources; the FE falls back to a tally-only display). |
| `scan_preview.rs` | Scan preview subsystem for Copy dialog live stats: `start_scan_preview`, `cancel_scan_preview`, `is_scan_preview_complete`. Background scans (local and volume-based) with result caching. Emits `expected_files_total` / `expected_bytes_total` (sampled once at scan start from the drive index) on every `scan-preview-progress` event, alongside the running tallies and `current_dir`. |
| `copy.rs` | `copy_files_with_progress`: scan → disk space check → per-file copy via `copy_single_item`. `CopyTransaction` for rollback. |
| `move_op.rs` | Same-fs: `fs::rename`. Cross-fs: copy to `.cmdr-staging-<uuid>`, atomic rename, delete sources. |
| `delete.rs` | Scan, delete files first, then directories in reverse/deepest-first order. Not rollbackable. Also contains `delete_volume_files_with_progress` for non-local volumes (MTP): scans via `volume.list_directory()`, deletes via `volume.delete()` per item. |
| `eta.rs` | `EtaEstimator`: time-weighted EWMA per axis (bytes, files), τ ≈ 3 s. Combines via `max(ETA_bytes, ETA_files)`. One per `WriteOperationState`, fed by `state.enrich_progress` at every `write-progress` emit site. See [ETA + throughput](#eta--throughput) below. |
| `trash.rs` | `move_to_trash_sync()` (macOS: ObjC `trashItemAtURL`; Linux: `trash` crate; reused by `commands/rename.rs`) and `trash_files_with_progress()` (batch trash with per-item progress, cancellation, partial failure). Uses `symlink_metadata()` for existence checks (handles dangling symlinks). |
| `copy_strategy.rs` | Strategy selection per file: network FS → chunked copy; overwrite → temp+rename; macOS → `copyfile(3)`; Linux → `copy_file_range(2)`. |
| `macos_copy.rs` | FFI to macOS `copyfile(3)`. Preserves xattrs, ACLs, resource forks, Finder metadata. Supports APFS `clonefile`. |
| `linux_copy.rs` | Linux `copy_file_range(2)` with reflink support on btrfs/XFS. 4 MB chunks, cancellation between iterations. |
| `chunked_copy.rs` | 1 MB chunked read/write, the default copy method for all non-APFS-clonefile copies on macOS and network copies on Linux. Checks cancellation between chunks. Copies xattrs, ACLs, timestamps. |
| `volume_copy.rs` | Volume-to-volume copy (Local↔MTP↔SMB): `copy_between_volumes`, `scan_for_volume_copy`. Uses `OperationEventSink` (not `AppHandle` directly) for event emission. Handles conflict detection, resolution, progress, rollback (delete all copied files in reverse with progress), and partial-file cleanup on cancel. Shared `map_volume_error` helper. |
| `volume_move.rs` | Volume-to-volume move: `move_between_volumes`, `move_within_same_volume`. Same-volume uses `Volume::rename`; cross-volume does copy+delete. |
| `volume_conflict.rs`, `volume_strategy.rs` | Conflict resolution (Stop/Skip/Overwrite/Rename) and copy strategy selection for volume operations. |
| `tests.rs` | Unit tests. |
| `copy_integration_test.rs` | Copy operation integration tests (permissions, symlinks, xattrs, edge cases). |
| `delete_integration_test.rs` | Delete operation integration tests. |
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

**`volume_copy` path is fully wired up.** The three `volume_*` files are re-exported from `mod.rs` and called by the `copy_between_volumes` and `move_between_volumes` Tauri commands. Both copy and move operations support conflict detection and resolution (Stop/Skip/Overwrite/Rename) for all volume combinations (Local↔MTP, MTP↔MTP). Volume copy supports rollback (delete all copied files in reverse order with progress events, matching the local copy's `rollback_with_progress` pattern) and cancel cleanup (delete only the last partial file). Rollback uses `delete_volume_path_recursive` which lists directory contents via `Volume::list_directory` and deletes children before parents.

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
| `write-source-item-done` | All files for a top-level source item processed (for gradual deselection) |
| `dry-run-complete` | `config.dry_run == true` (returns `DryRunResult`) |
| `scan-preview-progress` | During `start_scan_preview` |
| `scan-preview-complete` | Preview scan finished |
| `scan-preview-error` | Preview scan failed |
| `scan-preview-cancelled` | Preview scan cancelled |

## Key decisions

**Decision**: `walk_dir_recursive` dedupes hardlinks by inode when summing `total_bytes`.
**Why**: A naïve `*total_bytes += metadata.len()` per direntry over-counts on hardlink-heavy trees (cargo `target/`, sccache caches, deduplicated backups). Without dedup, a 49 GB `target/debug` reported 70+ GB to the scan UI, and the "X% of estimated" progress bar (denominator from the indexer's `dir_stats`, which already inode-dedupes) couldn't converge to 100%. Mirrors `indexing/scanner.rs`'s `seen_inodes: HashSet<u64>` pattern, with the same `nlink == 1` fast path. The set is operation-scoped (shared across all source roots in one scan, dropped when the scan ends), so hardlinks crossing source roots still count once. **Unix-only**: `std::fs::Metadata` has no `nlink()` accessor outside Unix; non-Unix falls back to the old naïve sum. Doesn't apply to `dry_run_scan_recursive` (that path reports for conflict counts, not for a progress denominator).

**Decision**: `WriteProgressEvent::with_scan_meta` is the only path that sets the scan-only fields (`current_dir`, `expected_files_total`, `expected_bytes_total`).
**Why**: 20+ emit sites construct `WriteProgressEvent` literals for active-phase events. Adding three optional fields to the struct would force every site to write `current_dir: None, expected_files_total: None, expected_bytes_total: None,`, pure mechanical noise. The `new(...)` constructor takes the eight core counter fields and defaults the scan meta to `None`; the scan emit sites in `scan.rs` and `scan_preview.rs` opt in via `.with_scan_meta(...)`. Future scan-related fields go through the same builder. If a real refactor of the 20 literals to `new(...)` ever happens, the builder pattern still composes cleanly on top.

**Decision**: `copy_volumes_with_progress` scan phase calls `scan_for_copy_batch` once instead of `scan_for_copy` per source (Phase 4 Fix 4)
**Why**: Network-backed volumes (SMB) pay 1 RTT per top-level source in the scan phase. Looping over sources made that serial: for 100 tiny files at ~60 ms RTT, ~5 s of pure stat latency before the copy phase started. `scan_for_copy_batch` surfaces both the aggregate (file/dir counts, total bytes) and a per-path vec (is_directory, size) in a single trait call; the copy engine folds the per-path vec into its `source_hints` map and skips the old per-source re-stat. `SmbVolume` overrides `scan_for_copy_batch` to pipeline N stats over one SMB session; measured 6.5× wall-clock win at 100 files (6.11 s → 947 ms) on a Tailscale link. `LocalPosixVolume` / `InMemoryVolume` inherit the default serial per-path loop; it's cheap for them. See `docs/notes/phase4-rtt-investigation.md`.

**Decision**: Volume copy pipeline uses `OperationEventSink` trait instead of `tauri::AppHandle`
**Why**: Decouples the copy/move orchestration from the Tauri framework. `TauriEventSink` wraps AppHandle for production; `CollectorEventSink` stores events for test assertions. Enables testing `copy_volumes_with_progress` end-to-end (multi-file copy, cancellation, conflict resolution, progress tracking) without a Tauri runtime. Currently only the volume copy/move path is migrated; local copy/delete/trash still use AppHandle directly (to be migrated during the async Volume refactor).

**Decision**: `delete_files_start` routes to either `delete_files_with_progress` (local, uses `walkdir` + `fs::remove_file`) or `delete_volume_files_with_progress` (non-local, uses `Volume` trait) based on `volume_id`.
**Why**: MTP volumes can't use `walkdir` or `fs::remove_*`. Rather than refactoring the existing local delete to go through the Volume trait (which would add overhead for local ops), we keep the fast local path and add a parallel volume-aware path. Both emit identical events so the frontend progress dialog works unchanged.

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
