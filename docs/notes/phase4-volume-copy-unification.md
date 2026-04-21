# Phase 4 — Unified volume copy abstraction + concurrency

**Status**: P4.1 complete, P4.2 complete, P4.3 complete. P4.4 (re-measure on QNAP) pending a network where the NAS is reachable.

Design for removing `Volume::export_to_local` / `import_from_local` from the `Volume` trait and unifying all cross-volume copies on `open_read_stream` + `write_from_stream`, with a single streaming copy engine that dispatches on one fast-path (same-APFS clonefile) and otherwise pipes bytes generically. Concurrency then lives in the copy engine, not per-volume-trait-method, parameterized by a `Volume::max_concurrent_ops()` hint each backend provides.

Purpose: collapse the current three copy paths (local↔local, local↔volume, volume↔volume) into two (APFS clone, streaming), making the Volume trait smaller, new backends easier to add, and concurrency uniformly available to every copy direction.

## Staging

Three stages on `main`, each its own commit and green-light gate:

1. **P4.1 — Unify the trait.** Remove `export_to_local` / `import_from_local`. Ensure all backends (`LocalPosixVolume`, `SmbVolume`, `MtpVolume`, `InMemoryVolume`) implement `open_read_stream` + `write_from_stream`. Refactor `volume_copy.rs::copy_single_path` (or equivalent inner dispatch) to two cases: `LocalPosixVolume↔LocalPosixVolume same APFS` → clonefile; else → generic streaming pipe. **No concurrency changes** — per-file semantics behavior-equivalent to pre-refactor.
2. **P4.2 — Add concurrency at the copy engine.** Introduce `Volume::max_concurrent_ops()` (default `1`). `SmbVolume` returns the settings-configurable value (default `10`). When copying N files between two volumes, the copy engine spawns `min(src.max_concurrent_ops(), dst.max_concurrent_ops())` parallel streaming-pipe tasks via `FuturesUnordered`. Progress aggregates across tasks. Cancellation check stays at task boundary.
3. **P4.3 — Settings surface.** `network.smbConcurrency` in `settings.json`, default `10`, range `1..=32`. Wire through to `SmbVolume::max_concurrent_ops`. Settings > Advanced > Network row with a numeric input.

Why the split: P4.1 is a refactor with zero runtime behavior change (the byte path per file flips from a direct `std::fs` write to a stream pipe; same data, same ordering, same events) — its risk is all in correctness of the new wiring, easy to test. P4.2 adds async complexity that's risky but concentrated in one place (copy engine). P4.3 is UI polish. Independent review units.

## Why

Current state has three copy paths tangled together:

1. **Local↔Local** (both `LocalPosixVolume`) → `copy.rs` with `copy_single_item`, can do APFS `clonefile(2)` (O(1) pointer copy, metadata-preserving) or `copyfile(3)` fallback. Rich cancellation, metadata preservation, rollback.
2. **Local↔any other volume** → `volume.export_to_local()` / `volume.import_from_local()` on the Volume trait. Short-circuits streaming: the volume's backend reads/writes the whole file, local side does a plain `std::fs` write / read. No metadata preservation.
3. **Volume↔Volume, neither local** → `open_read_stream()` + `write_from_stream()` on the Volume trait. Generic stream-pipe.

Path 1 is legit: APFS clonefile is a real capability no streaming abstraction can match. Preserved.

Paths 2 and 3 are not legit to keep as separate concepts: `export_to_local(src, dst)` is functionally "open a read stream, pipe to local `std::fs::File`." The split is historical — path 2 predates streaming being robust. It's tech debt that (a) doubles every backend's copy-method count, (b) forces new backends (S3, WebDAV, FTP) to implement twice as many methods for no benefit, (c) spreads concurrency logic across path 2 methods when it should live once in the copy engine.

Consolidating to one streaming path (plus the APFS clone fast path) means:

- New backends implement `open_read_stream` + `write_from_stream` and get copy-to-anywhere for free.
- Concurrency lives in one place (`volume_copy.rs`), not duplicated across `export_to_local` / `import_from_local` / `copy_volumes_with_progress`.
- One place to add features (resume, checksums, progress, cancellation) affects all copy directions.

The bench-measured 7x speedup for "100 tiny files from SMB" that `smb2` Phase 3 unlocked can't be realized in cmdr without the copy engine spawning concurrent streams — Phase 3 is the wire-level engine, Phase 4 is the consumer-side driver.

## The unified design in one paragraph

Every `Volume` implements `open_read_stream(path)` and `write_from_stream(path, stream)` (they already do; we just stop bypassing them via `export_to_local` / `import_from_local`). The `volume_copy.rs::copy_single_path` function becomes: (1) if source and destination are both `LocalPosixVolume` on the same APFS volume, delegate to the existing `copy_single_item` which will clonefile; (2) otherwise, open a read stream on the source, write it to the destination via `write_from_stream`, stream-by-stream, with the existing cancellation / progress / conflict logic wrapping the pipe. For batch copies (N files), the engine spawns `min(src.max_concurrent_ops(), dst.max_concurrent_ops())` concurrent streams via `FuturesUnordered`; backends that can't parallelize (MTP over USB) return `1` and nothing changes for them.

## Decisions

| # | Decision | Choice | Why |
|---|----------|--------|-----|
| F1 | `export_to_local` / `import_from_local` removal | **Big-bang in P4.1.** No deprecation. | Only consumer is `volume_copy.rs`; we control both sides. Deprecation adds maintenance burden. CHANGELOG documents. |
| F2 | `supports_export` capability flag | **Kept.** Renamed semantics: "this volume can stream its bytes" = source for a copy. | Some volumes are write-only or read-only; the flag still gates the copy dialog's "copy from this" UI. |
| F3 | APFS clone fast path | **Kept.** `LocalPosix → LocalPosix, same device` dispatches to `copy.rs` which already handles clone. | Real capability. No streaming alternative is equivalent. |
| F4 | Same-APFS detection | Use `std::os::unix::fs::MetadataExt::dev()` (already exists in `copy_strategy.rs::is_same_apfs_volume`). | Reuse existing logic. |
| F5 | `Volume::max_concurrent_ops()` | New trait method. Signature: `fn max_concurrent_ops(&self) -> usize;` default `1`. `SmbVolume` returns settings-derived value (default 10). `LocalPosixVolume` returns `num_cpus::get().clamp(4, 16)` or similar — disk I/O is multi-queue capable. `MtpVolume` returns 1. `InMemoryVolume` returns `usize::MAX` (clamped by caller). | Each backend knows its own parallelism limit. Copy engine takes `min` of both sides. |
| F6 | Upper bound on concurrency | Copy engine clamps to `min(src, dst, 32)`. | 32 is smb2's `MAX_PIPELINE_WINDOW`. Beyond it doesn't help, and for local-to-local "many tiny files" workloads we don't want to spawn hundreds of tasks. |
| F7 | Threshold for switching on concurrency | `N >= 3` files. Below that, sequential loop. | Spawning a task and collecting a future isn't free; for 1-2 files it's noise. |
| F8 | Concurrency primitive | `futures_util::stream::FuturesUnordered<BoxedCopyFut>` driven by a loop that keeps the in-flight window filled. | Matches the pattern smb2's pipelined loops already use. Keeps the sliding window semantics. |
| F9 | Progress aggregation | Shared `Arc<AtomicU64>` for bytes copied and files done; the existing 200 ms throttle emits snapshots. Each task writes its own per-file deltas. | Lock-free, matches existing progress interval. |
| F10 | Error handling | **Abort on first error**, same as today. On error, remaining in-flight tasks are drop-cancelled (their streams close; their partial-file temps get cleaned up by existing `.cmdr-tmp-*` logic). | Matches pre-refactor behavior. "Continue on error" would surprise users mid-copy. |
| F11 | Partial-file cleanup under concurrency | Existing `safe_overwrite_file` / `find_unique_name` behavior per task. On abort, each in-flight task's drop removes its own `.cmdr-tmp-*` temp. | Drop-based cleanup = concurrency-safe by construction. |
| F12 | Cancellation check frequency | Check `is_cancelled(&state.intent)` at task start, and again between chunks inside each stream pipe. | Already the per-file pattern; carries over. |
| F13 | Ordering of events | `write-progress` events emit aggregated state. `write-source-item-done` fires when each file completes, potentially out of submission order. | Matches what the UI already tolerates (it was already `tokio::spawn` friendly). |
| F14 | Conflict resolution interaction | Conflict detection is still pre-copy (scan phase). Resolution (`Stop` mode) blocks the whole batch until user decides. Per-file `Overwrite` / `Skip` / `Rename` applies at task start. | Scan-first keeps UX the same. |
| F15 | `SmbVolume` mutex handling | **Keep the mutex for single-op paths** (`list_directory`, `stat`, `rename`, etc.) — they're fine serialized. For stream open/write, grab a `Connection` clone **outside the mutex** once, drop the guard, then spawn tasks each holding the clone. Tasks never touch the mutex. | Mutex still protects the `SmbClient` + `Tree` for non-concurrent ops; concurrent ops use the already-Clone'd Connection underneath. No pool, no new data. |
| F16 | `LocalPosixVolume` concurrency strategy | Use `num_cpus::get().clamp(4, 16)`; each task is a `tokio::task::spawn_blocking` doing a chunked local copy. | Local disk can handle plenty of concurrent I/O; clamp to avoid runaway for giant batches. |
| F17 | Settings key for SMB concurrency | `network.smbConcurrency` (number), default `10`, range `1..=32`. | Lives with other network settings. Range matches `MAX_PIPELINE_WINDOW`. |
| F18 | Settings UI placement | Settings > Advanced > Network. Small numeric input, help text "Concurrent operations per SMB connection (default 10)". | Advanced avoids cluttering primary settings. |
| F19 | Cross-volume streaming migration | `volume_copy.rs::copy_single_path` dispatches on `(src.local_path().is_some() && dst.local_path().is_some() && is_same_apfs)` → clone; else → streaming. Both `export_to_local` and `import_from_local` call sites in `volume_copy.rs` get replaced with the streaming branch. | One dispatch point, one diff, easy to review. |
| F20 | Volume trait method removal order | Remove `export_to_local` / `import_from_local` AFTER every caller migrated. Callers are all in `volume_copy.rs`; other crates don't use them. | No downstream ripple beyond cmdr itself. |

## Migration plan (concrete)

### P4.1 — Unify the trait

1. **Remove from trait** (`src/file_system/volume/mod.rs` in the `Volume` trait def): `export_to_local`, `import_from_local`, and the capability check `supports_export` is revisited (still used to gate the copy dialog — kept, same semantics).
2. **Remove implementations**: `LocalPosixVolume::export_to_local` (becomes `std::fs::copy` via `copy.rs`), `SmbVolume::export_to_local` / `import_from_local` (becomes open_read_stream + write_from_stream usage by `volume_copy.rs`), `MtpVolume::export_to_local` / `import_from_local` (same), `InMemoryVolume` (already uses streaming). Net line delta: negative — each backend loses 50-200 LOC.
3. **Verify streaming methods work on every backend**: each of the four backends has `open_read_stream` and `write_from_stream` implemented. `LocalPosixVolume` — add them if not present; they wrap `std::fs::File` + `tokio::task::spawn_blocking` for the blocking reads/writes.
4. **Rewrite `volume_copy.rs::copy_single_path`**:
   ```rust
   async fn copy_single_path(src_vol, src_path, dst_vol, dst_path, …) -> Result<u64> {
       if both_are_local_posix_same_apfs(&src_vol, &dst_vol) {
           copy_single_item(abs_src, abs_dst, ...).await  // clone fast path
       } else {
           stream_pipe(&src_vol, &src_path, &dst_vol, &dst_path, ...).await
       }
   }
   ```
   `stream_pipe` opens a read stream, drives chunks through the destination's `write_from_stream`, reports progress per chunk, checks cancellation between chunks.
5. **Keep `volume_copy.rs::copy_volumes_with_progress` shape**. Internal `copy_single_path` is the only thing that changes. Outer scan/conflict/progress flow stays.
6. **Update `src/file_system/volume/CLAUDE.md`** — trait table, capability table, "how to add a new volume" checklist collapses from "3 copy methods" to "2 stream methods."
7. **Run P4.0 bench** post-P4.1 to confirm no performance regression. Expected: same as pre-P4.1 (same per-file work, just via streaming path; if anything slightly slower due to extra chunk hop, but should be within noise).

### P4.2 — Add concurrency — DONE

Deviations from the pre-pinned F-decisions:

- **F5 — `LocalPosixVolume` concurrency**: Used `std::thread::available_parallelism()` (stdlib) with a rough "halve for physical cores" heuristic instead of adding a `num_cpus` crate dependency. Still clamped to `4..=16`. Same effective concurrency on typical dev/user hardware.
- **F5 — `InMemoryVolume` concurrency**: Returns `32` (matches the caller's upper bound) instead of `usize::MAX`. No behavioral difference — both hit the clamp — but the smaller number is less surprising in logs.
- **Partial cleanup under concurrency**: Added a shared `Arc<Mutex<Vec<PathBuf>>>` tracking in-flight destination paths. On abort/cancel (F10) the partial-cleanup loop now walks both `last_dest_path` (sequential) and `in_flight_partials` (concurrent), which extends F11's "drop-based cleanup = concurrency-safe" with an explicit delete-of-renamed-dest pass. The `.cmdr-tmp-<uuid>` cleanup inside each backend's `write_from_stream.abort()` is unchanged.

Implementation notes:

1. **Add trait method** `fn max_concurrent_ops(&self) -> usize { 1 }` to `Volume`.
2. **Implement overrides** on each backend: `SmbVolume` reads setting (via a global or `AppHandle`), returns `10` default; `LocalPosixVolume` returns `num_cpus::get().clamp(4, 16)`; others stay default.
3. **Rewrite `volume_copy.rs` inner loop** from `for path in source_paths { copy_single_path(…).await }` to:
   ```rust
   let concurrency = src_vol.max_concurrent_ops().min(dst_vol.max_concurrent_ops()).min(32);
   let mut in_flight = FuturesUnordered::new();
   for path in source_paths {
       if in_flight.len() >= concurrency {
           if let Some(result) = in_flight.next().await { process(result?); }
       }
       let src_vol = Arc::clone(&src_vol);
       let dst_vol = Arc::clone(&dst_vol);
       in_flight.push(Box::pin(async move {
           copy_single_path(&src_vol, path, &dst_vol, …).await
       }));
   }
   while let Some(result) = in_flight.next().await { process(result?); }
   ```
   Early-abort on first error via the existing `?` + drop.
4. **Progress aggregation**: the existing `progress_state` (atomic counters) already works; multiple tasks incrementing the same `AtomicU64` is fine. The 200 ms emit throttle on the driver side stays the same.
5. **Run P4.0 bench** post-P4.2, compare to baseline.

### P4.3 — Settings

1. Add `network.smbConcurrency: number` to the settings schema (wherever it lives in `src/lib/settings/`).
2. Global accessor in Rust (like `crate::file_system::is_filter_safe_save_artifacts_enabled`): `fn smb_concurrency() -> usize` that reads from the global settings store with default 10, clamped to 1..=32.
3. `SmbVolume::max_concurrent_ops` returns `smb_concurrency()`.
4. Svelte component in `src/lib/settings/Advanced.svelte` (or similar) with a numeric input + min/max.

## Test plan

### P4.1 (unification) tests

- **Unit**: existing `InMemoryVolume` copy tests should pass unchanged (they already use streaming).
- **Integration**: `phase4_bench_baseline_smb_to_local_100_tiny_files` (the P4.0 test) should produce the same wall clock before and after P4.1 — no concurrency yet, just refactored path. Expected within ~10% of pre-P4.1.
- **Local↔Local APFS clone**: add/keep a test asserting clonefile is used for same-volume (can verify via `statfs` + file count; or explicit test of `copy_strategy::is_same_apfs_volume`).
- **Cross-volume streaming** (SMB↔MTP, MTP↔Local, etc.): pick one representative direction and write/keep a test; the refactor shouldn't regress.

### P4.2 (concurrency) tests

- **Unit**: on `InMemoryVolume` (set `max_concurrent_ops = 10`), copy 100 files; verify progress events and final state. Can't easily measure timing in unit tests, but can assert task count / interleaving via logging.
- **Integration (same as P4.0)**: rerun baseline bench, expect ~7x improvement for SMB→local.
- **Cancellation under concurrency**: spawn 100 in-flight; cancel mid-batch; assert no orphaned temp files and a clean `write-cancelled` event.
- **Error under concurrency**: inject an error on file 5 of 20; assert remaining in-flight tasks drop cleanly and the batch returns the error.

### P4.3 (settings) tests

- Unit test for the settings accessor (clamping, default).
- Manual UI test: change setting, verify `SmbVolume::max_concurrent_ops` reflects it.

## Risks

| Risk | Mitigation |
|------|------------|
| P4.1 streaming path is slower per-file than pre-refactor `export_to_local` for small files. | The bench compares. If slower by > 10%, investigate — likely extra chunk-buffer hop. Fix by tuning chunk size in the stream pipe. |
| `LocalPosixVolume::open_read_stream` / `write_from_stream` isn't implemented. | Check first; if missing, add as part of P4.1 with `spawn_blocking` wrappers around `std::fs::File`. |
| `SmbVolume::max_concurrent_ops` needs access to settings during construction, but SmbVolume is constructed before settings load. | Settings accessor reads from a global `OnceLock` or `RwLock` that's populated early; reading during copy (after user interaction) is always post-settings-load. |
| Progress event storm under 100 concurrent tasks. | 200 ms throttle on the emit side already caps it. Atomic counter + throttle = max 5 events/sec regardless of task count. |
| Partial-file temp cleanup on drop may race with the next task creating a same-named temp. | Existing `.cmdr-tmp-<uuid>` pattern generates unique names per task. No collision possible. |
| `FuturesUnordered` size grows unbounded if we queue all N before awaiting any. | The window-management loop (`if in_flight.len() >= concurrency { .next().await }`) bounds it at `concurrency`. |
| SMB server-side resource limits (QNAP default 256 credits, but enterprise caps?). | Clamped to `32` by F6. QNAP handles 32 comfortably. If we ever hit `STATUS_INSUFFICIENT_RESOURCES`, we add a semaphore. |

## Out of scope for Phase 4

- `delete_many` / `get_metadata_many` batch methods. Same pattern, different operation — do later if we see the pain.
- Multi-session `SmbVolume` connection pool. Phase 3 bench showed single-session concurrent is enough; revisit only if we find workloads that need it.
- Write-side concurrency benchmarking on SMB. Reads are bench-proven; writes may behave differently on QNAP — measure before committing.
- Upload/download progress shown as per-file + aggregate simultaneously in the UI. Current UI shows aggregate; per-file panel is a future UI improvement.
- SMB3 Multichannel (protocol-level multi-connection). Separate big feature.
