# Phase 4.2 — Concurrent streaming copy: summary

Branch: `p4.2-concurrency`
Final commit: `b63aa907`
Test count: **1331 passed, 0 failed, 23 ignored** (matches the 1328 + 3-new target)

## What changed per file

### Trait + backends

- `apps/desktop/src-tauri/src/file_system/volume/mod.rs` — added
  `Volume::max_concurrent_ops(&self) -> usize` with default `1` and a doc
  comment explaining the contract (backends serialized by a single transport
  return 1; parallel-I/O backends return higher; caller clamps to 32).
- `apps/desktop/src-tauri/src/file_system/volume/local_posix.rs` — override
  returns `available_parallelism() / 2` clamped to 4..=16 (rough physical-core
  stand-in; no `num_cpus` crate added).
- `apps/desktop/src-tauri/src/file_system/volume/smb.rs` — override returns
  `10` with a `TODO(P4.3)` pointing at the future `network.smbConcurrency`
  setting.
- `apps/desktop/src-tauri/src/file_system/volume/mtp.rs` — override returns
  `1` (USB bulk transport is serial).
- `apps/desktop/src-tauri/src/file_system/volume/in_memory.rs` — override
  returns `32`.

### Copy engine

- `apps/desktop/src-tauri/src/file_system/write_operations/volume_copy.rs` —
  rewrote the copy loop in `copy_volumes_with_progress`:
  - Hoisted `files_done_atomic` / `atomic_bytes_done` / `last_progress_mutex`
    to `Arc<...>` so concurrent tasks can share them.
  - Added `copied_paths: Arc<Mutex<Vec<PathBuf>>>` and
    `in_flight_partials: Arc<Mutex<Vec<PathBuf>>>` for concurrency-safe
    tracking.
  - Computed `concurrency = min(src.max_concurrent_ops(),
    dst.max_concurrent_ops(), 32)` (F6).
  - `use_concurrent_path = source_paths.len() >= 3 && concurrency > 1` (F7).
  - Concurrent branch: `FuturesUnordered<Pin<Box<dyn Future + Send + 'a>>>`
    driven by a sliding-window loop that keeps `concurrency` tasks in flight
    (F8). Conflict resolution stays synchronous on the driver (F14) so Stop
    mode still blocks the whole batch.
  - Sequential branch: unchanged semantics, refactored only to use the shared
    `Arc` state for a single code path post-loop.
  - Abort on first error (F10): dropping `in_flight` cancels remaining tasks;
    their `write_from_stream.abort()` handles temp-file cleanup per backend.
  - Cancellation check: at task start (in the while-loop), inside
    `copy_single_path`'s existing between-chunks check, and in the per-task
    progress callback (returns `Break` on cancel).
  - Partial-file cleanup on abort: walks both `last_dest_path` (sequential)
    and `in_flight_partials` (concurrent).

### Tests (in the same file)

Three new tests, all on `InMemoryVolume` (max_concurrent_ops = 32):

1. `test_concurrent_copy_50_files_all_succeed` — 50 × 1 KB files, verifies
   all land with correct content, progress events fire, completion totals
   are right.
2. `test_concurrent_copy_aborts_on_first_error` — 20 files, `PoisonedReadVolume`
   wrapper fails on file 05; verifies the outer `Err` is the injected
   `IoError` and the poisoned file doesn't land. (`PoisonedReadVolume` is a
   small in-test wrapper that delegates to `InMemoryVolume` except for the
   named path.)
3. `test_concurrent_copy_cancellation_mid_batch` — 20 × 200 KB files,
   intent flipped to Stopped from an event-sink hook after 2 progress
   events. Verifies fewer than 20 files at dest and result is `Cancelled`
   or `IoError` (both are valid cancellation shapes; matches the sequential
   `test_multi_file_copy_cancel_mid_flight` precedent).

### Docs

- `docs/notes/phase4-volume-copy-unification.md` — status line "P4.2 complete"
  near the top; new "P4.2 — Add concurrency — DONE" section listing deviations.
- `CHANGELOG.md` — bullet under `### Changed` covering the batch parallelism
  and the new trait method.
- `apps/desktop/src-tauri/src/file_system/volume/CLAUDE.md` — capability-list
  entry for `max_concurrent_ops`; added row to the capability matrix.

## Deviations from F-decisions

- **F5 `LocalPosixVolume` concurrency**: used stdlib
  `std::thread::available_parallelism()` with a `/ 2` rough physical-core
  stand-in, instead of adding a `num_cpus` crate dependency. Same
  `.clamp(4, 16)` as specified. Effective values match on typical user
  hardware (Apple Silicon cores = performance-+efficiency-class, both count;
  for concurrency this is fine).
- **F5 `InMemoryVolume` concurrency**: returns `32` (the caller's upper
  bound) instead of `usize::MAX`. No behavioral difference — both hit the
  clamp — just cleaner in logs and tests.
- **Partial-file cleanup under concurrency**: F11 says "drop-based cleanup =
  concurrency-safe by construction" which is true for the `.cmdr-tmp-<uuid>`
  temp files inside `write_from_stream.abort()`. But a task whose temp was
  already renamed into `dest_item_path` before the abort fires leaves a real
  file behind that none of the per-task drop paths delete. Added explicit
  tracking: each task push its `dest_item_path` into
  `in_flight_partials: Arc<Mutex<Vec<PathBuf>>>` before the copy, removes it
  on completion, and the cancel/error branch walks the remaining set. The
  sequential path's `last_dest_path` logic is unchanged for batches < 3.

## Caveats for P4.3 / leader's bench re-run

- `SmbVolume::max_concurrent_ops` is hardcoded at 10 with a `TODO(P4.3)` —
  when P4.3 adds `network.smbConcurrency`, it should read from the settings
  accessor (default 10, clamp 1..=32).
- The `phase4_bench_baseline_smb_to_local_100_tiny_files` test is still
  marked `#[ignore]`; leader runs it manually. With P4.2 the expected result
  is the ~7x speedup the smb2 Phase 3 bench predicted — concurrency=10 will
  keep ~10 GET_INFO / READ operations in flight per file-pipeline round-trip
  cost instead of one.
- The `write-source-item-done` event is still not emitted from the volume
  copy path (wasn't emitted pre-P4.2 either — the comment in
  `write_operations/CLAUDE.md` covers that only local paths emit it today).
  P4.2 didn't change this. If the UI wants per-file deselection hooked up
  for volume copies, that's a separate task.
- The in-flight partials cleanup uses `delete_volume_path_recursive` per
  partial. On a 30-file in-flight abort this is 30 sequential deletes. If
  that becomes noticeable, spawning them concurrently is a two-line change.
