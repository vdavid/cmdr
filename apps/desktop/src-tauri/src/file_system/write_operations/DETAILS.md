# Write operations details

Pull-tier docs for `src-tauri/src/file_system/write_operations/`: architecture, flows, and decision rationale.
Must-know invariants and gotchas live in `CLAUDE.md`.

Frontend counterpart: `apps/desktop/src/lib/file-operations/CLAUDE.md`
(umbrella) plus colocated child docs for `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`,
`apps/desktop/src/lib/file-operations/delete/CLAUDE.md`,
`apps/desktop/src/lib/file-operations/mkdir/CLAUDE.md`, and
`apps/desktop/src/lib/file-operations/mkfile/CLAUDE.md`.

Subdirs:
- `transfer/CLAUDE.md` — copy + move (local FS, cross-volume, MTP, SMB), conflict resolution, the shared transfer driver, platform-specific copy backends.
- `delete/CLAUDE.md` — delete walker (local + volume-aware), trash, the oracle-aware delete fast path.

## Purpose

Implements the four destructive file operations as background tasks that stream Tauri events to the frontend. Every operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites, network mounts, cross-filesystem moves, and name/path length limits.

Ask Cmdr bulk renames use the same managed-operation lifecycle. `LocalPosixVolume` makes every non-forced local rename
atomic-no-overwrite, including attached disks and cloud folders registered under non-root volume IDs: macOS uses
`renamex_np(RENAME_EXCL)` and Linux uses `renameat2(RENAME_NOREPLACE)`. Root bulk-renames share that primitive directly.
The dialog's target-exists preflight improves review UX, but only the kernel operation closes the check-to-rename race.

Pre-flight scans reuse cached listings when the source volume reports `WatchCoverage::EveryWriter`, avoiding redundant `list_directory` calls. The freshness contract and per-backend debounce windows are documented in `../volume/CLAUDE.md` and `../listing/caching.rs::try_get_authoritative_listing`.

## Files (top level)

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The spine: `CLAUDE.md` § Module map.
The full top-level inventory is here:

- `mod.rs` (public API + the `start_write_operation` lifecycle), `manager.rs` (registry + lane admission), `state.rs`
  (`WriteOperationRegistry`, `WriteOperationState`, `CopyTransaction`, the settle guard, the cancel/abort commands),
  `status_cache.rs` (the status cache, the busy-volumes set it derives, the external drag-out seam, and
  `list_active_operations` / `get_operation_status`), `operation_intent.rs` (`OperationIntent`, `PauseGate`),
  `archive_edit/` (the zip-edit driver).
- Scan and preview: `scan.rs`, `scan_preview.rs`, `scan_cache.rs`, `compress_estimate.rs`. Conflicts and overwrite:
  `conflict.rs`, `overwrite.rs`. Cancellation and durability: `cancellable.rs`, `rollback.rs`, `durability.rs`.
- Vocabulary and edges: `types.rs`, `event_sinks.rs`, `error_classification.rs`, `validation.rs`, `analytics.rs`,
  `eta.rs`. Journaling: `journal.rs`, `journal_search.rs`. Remote archive I/O: `archive_remote_edit.rs`,
  `scratch_dir.rs`. Entry points: `create/` + `create.rs`, `rename/` + `rename.rs`, `paste_clipboard.rs`. Fixtures:
  `test_support.rs`.

What the mechanisms DO is in the sections below: the registry, lanes, and `run_instant` in § "Operation manager";
the zip-edit driver in § "Archive edits"; cancellation, pause, Stop-mode conflicts, safe overwrite, scan-preview caching,
and the compressed-size estimate in § "Key patterns and gotchas"; durability and the two byte totals in § "Key
decisions"; the estimator in § "ETA + throughput"; `WriteSettledGuard` in § "Settle contract"; journaling in
`../../operation_log/DETAILS.md` § Capture. Only the layout facts that none of those carry live here:

- **Four re-export facades are deliberate, not collapsible** (§ "Key decisions" has the why): `mod.rs` re-exports
  `transfer::*` + `delete::*` so callers keep their `crate::file_system::write_operations::<symbol>` paths, `state.rs`
  re-exports the `operation_intent` + `scan_cache` types, and `types.rs` re-exports the `event_sinks` types +
  `error_classification::IoResultExt`. **`TauriEventSink` is the exception**: it's re-exported at the
  `write_operations` module root (and up through `file_system`) for the IPC edge, NOT from `types.rs`, because the
  pipeline layer only ever names the trait.
- **Event structs and their builders live apart on purpose**: the struct definitions in `types.rs`, the
  `WriteProgressEvent` (`new` / `with_scan_meta`) and `WriteErrorEvent` (`new`) impls in `event_sinks.rs` beside the
  sinks that emit them.
- **`analytics.rs` is `pub(super)` and reached ONLY from `TauriEventSink::emit_complete`.** Every property is
  categorical (op kind, a count bucket, a bool): no names, no paths ever. Copy/Move → `file_transfer_completed`,
  Delete/Trash → `delete_used`.
- **`error_classification.rs` classifies from `errno` / `ErrorKind` only, never the message**.
- **`validation.rs`'s `ensure_destination_dir` runs AFTER `validate_destination_not_inside_source`**, so creating a
  missing destination (and its ancestors) can never materialize a folder inside a source. The volume-aware pipelines
  mirror both the behavior and the order with `Volume::create_directory_all(dest)`; see `../volume/DETAILS.md`
  § "Recursive destination create".
- **`conflict.rs::numbered_name(stem, ext, counter)` is the ONE ` (N)` formatter** (`counter 0` = bare, `1..` = ` (N)`).
  `find_unique_name` and `paste_clipboard.rs` both go through it, so the two numbering paths can't drift.
- **`create.rs` co-locates the synthetic listing-cache diff** (`should_emit_synthetic_diff` /
  `emit_synthetic_entry_diff`, both `pub(super)`) that lands a brand-new entry in the pane on local-FS-backed volumes.
  `paste_clipboard.rs` reuses both so a pasted file cursor-lands exactly like mkfile.
- **Scan-phase `expected_files_total` / `expected_bytes_total` come from
  `crate::indexing::read::expected_totals::expected_totals_for_sources`** and are `None` when the index doesn't cover
  every source; the FE then falls back to a tally-only display instead of a progress bar. `scan.rs`'s walker derives
  `current_dir` from `path.parent()`, which is what lets the UI show "in directory: …" beside the filename.
- **`rename/bulk.rs` is Ask Cmdr's reviewed batch-rename driver**, and it does NOT use `run_instant`: `start_bulk_rename`
  takes only backend-owned rows accepted by preflight and runs through `spawn_managed` as one lane-queued operation. Its
  dependency planner renames independent rows directly, peels acyclic chains from their free destination, uses one
  same-directory temporary per cycle, and retains one temporary for a case-only rename on a case-insensitive filesystem.
  Local and remote drivers share the plan, so remote rename-as-copy backends don't duplicate every transfer.
  Cancellation happens between components: a started cycle finishes or reverses before the driver observes cancellation
  again. It journals one header and one final outcome per row. The Ask Cmdr command is the only caller, and it never
  receives paths or names from the frontend.
- **`paste_clipboard.rs::write_payload_to_dir` runs under a 30 s write timeout** (`commands/clipboard.rs`), a longer
  tier than the 5 s empty-mkfile write, because the payload can be a large image landing on a slow network volume. It
  takes an already-read `ClipboardPayload` + a `&Path`, decoupled from NSPasteboard and the IPC edge, so it's
  `TempDir`-testable; the retry loop writes via `Volume::create_file` (O_EXCL) and bumps the counter on the TYPED
  `VolumeError::AlreadyExists`, so there's no pre-scan-then-write TOCTOU and it works on any writable volume.
  **Partial-file-on-timeout edge (accepted):** past 30 s the write future is dropped and a partial `pasted.<ext>` may
  remain (the user sees a timeout and can retry or delete). It's bounded and rare (local writes never approach 30 s, and
  on a local FS `create_file`'s `spawn_blocking` isn't even cancellable, so the file actually completes), and only
  affects slow network volumes. If it ever matters, route paste-as-file through the managed transfer engine for
  cancellation + no-partial guarantees. Pasteboard read + flavor precedence:
  `apps/desktop/src-tauri/src/clipboard/DETAILS.md` § Paste clipboard content as a file.

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

**Stop-mode conflict resolution.** Creates a per-conflict `tokio::sync::oneshot` channel, **stores the sender BEFORE emitting the `write-conflict` event**, then blocks on the receiver (`blocking_recv()` inside `spawn_blocking`; the volume path `await`s instead). Store-before-emit is load-bearing: a responder can only answer a conflict it has observed, so if the event reached `resolve_write_conflict` (or a test responder sink) before the sender slot was filled, the take would miss and the recv would hang. Both the local-FS branch (`conflict.rs`) and the volume branch (`transfer/volume/conflict.rs`) order it this way. Frontend calls `resolve_write_conflict(operation_id, resolution, apply_to_all)` which takes the stored `Sender` and sends the `ConflictResolutionResponse`. `cancel_write_operation` drops the sender, causing the receiver to return `Err` (interpreted as cancellation). This is strictly better than the old Condvar+timeout approach: no polling, no 30 s safety timeout needed, immediate unblock on cancel. Pinned by `conflict.rs::stop_branch_store_before_emit_tests` (local) and the `ConflictResponderSink` suites (volume).

**Conflict-dispatch mutex (folder merges).** `WriteOperationState::conflict_dispatch_lock` (a `tokio::sync::Mutex`, next to `conflict_resolution_tx`) serializes the whole Stop-mode dispatch for an operation: there is exactly one human and one oneshot slot, so two tasks both hitting a Stop-mode clash at once — the concurrent volume-copy spawn loop, or two parallel deep directory merges — must queue rather than race to emit a `write-conflict` and clobber each other's sender. The dispatch sequence under the lock: check `is_cancelled` (bail with `Cancelled` so a queued task can't emit a prompt no one will answer after the dialog tears down — a hang), re-check the apply-to-all latch (a prior "…all" answer collapses the queued prompt), emit + await, store the latch, release. Released on every exit, NEVER held across the subsequent file write. Volume-side only today (the local-FS engine's per-file conflicts surface serially inside one `spawn_blocking`).

**`cancel_write_operation` does state transitions.** `rollback=true` → `Running → RollingBack`, `rollback=false` → `Running → Stopped` or `RollingBack → Stopped`. First caller's decision wins; subsequent calls with different intent are no-ops (unless transitioning from `RollingBack → Stopped`). `cancel_all_write_operations` always transitions to `Stopped` (teardown should never silently roll back without visual feedback).

**Scan preview state.** `start_scan_preview` registers one `PREVIEWS` entry in `scan_cache.rs` and spawns a walk. The entry is either in flight or settled, and a settled one carries WHY: complete (with its `CachedScanResult`), errored (with its message), or cancelled. `copy_files_start` / `delete_files_start` consume a completed result via `preview_id` in `WriteOperationConfig`, skipping a redundant scan. An entry is freed by three paths: (1) `take_cached_scan_result(preview_id, sources)` at op start (the consume path), (2) `cancel_scan_preview(preview_id)` on dialog teardown — it sets the in-flight cancel flag AND drops the entry, so a dialog dismissed after the scan completed doesn't leak the result — and (3) a TTL safety net: `settle_preview` first evicts settled, UNCLAIMED entries older than `SCAN_RESULT_TTL` (5 min). The TTL is a backstop for a caller that forgets both (1) and (2); the pure `expired_scan_result_ids` helper is unit-tested. A `CachedScanResult` can hold tens of thousands of `FileInfo`, so none of these paths is optional.

**The cache is bound to its request, and says so when it's incoherent.** A `preview_id` proves the frontend once asked for a scan. It proves nothing about WHICH scan, and three of the six consumers act on the cached file list without ever re-reading their own `sources` again, so an id pointing at a preview of a different selection makes each of them fail differently: the LOCAL delete walker (`delete/walker.rs`) deletes the previewed tree instead of the requested one, with no rollback and no progress line naming it; the LOCAL copy (`transfer/copy/mod.rs`) writes the previewed tree to the destination while its bulk-skip set still reads the requested one; the LOCAL move (`transfer/move_op.rs`) stages the previewed tree and then fails in Phase 3 looking for the requested name in staging, a half-staged move. The VOLUME delete and both `transfer/volume/preflight.rs` sites were already source-bound (they iterate `sources` and fall through per-source on a miss), so they degrade to a rescan rather than acting wrong.

Two mechanisms close it at the choke point, and both live in `scan_cache.rs`:

- `CachedScanResult::sources` records what the preview was asked to walk, and `take_cached_scan_result(preview_id, requested_sources)` compares it SET-wise against the operation's own list. A mismatch is a cache miss: the entry is dropped, a warn names both lists, and the caller takes the fresh-scan fallback it already had. The comparison normalizes nothing on purpose. A path that differs only by a trailing separator is an IPC-edge bug; a lenient comparison here would just be another belief.
- `insert_scan_result` carries a coherence canary: a completed walk with `file_count > 0` and an empty `per_path` warns and trips a `debug_assert!`. That's the shape that let a LOCAL preview hand the copy drivers an empty `source_hints` map, which they read as a confident `is_directory: false`. It's one-directional (a volume batch legitimately caches empty `files` with a populated `per_path`), and it's a `debug_assert!`, so a release build still admits the entry and the drivers still have to survive it (`transfer/volume/copy_source_hint_tests.rs` is that defense's proof, seeding past the canary via `seed_incoherent_scan_result_for_test`).

`SCAN_PREVIEW_RESULTS` is private to `scan_cache.rs` so neither can be walked around: `insert_scan_result`, `take_cached_scan_result`, `cached_scan_totals`, and `release_scan_result` are the whole surface, and `state.rs` re-exports the types but not the map. A `pub(super)` static is a choke point in name only. Pinned by `scan_cache_tests.rs` (both mechanisms), `delete/preview_binding_tests.rs` (the destructive one, plus a volume-side regression fence), and one binding test each in `transfer/copy/copy_tests.rs` and `transfer/move_op_tests.rs`.

**A completed preview always carries `per_path`, whichever walk produced it.** Both `run_scan_preview` (the LOCAL `std::fs` walk, via `scan.rs::walk_sources_with_per_path`) and `run_volume_scan_preview` (SMB / MTP) cache one `CopyScanResult` per top-level source: its type, file/dir counts, and byte totals. That map is the ONLY thing that tells a cross-volume copy whether each selected item is a file or a folder without paying a stat probe per source, and it's what a volume delete's fast path reads for top-level file sizes. Downstream code treats a source that isn't in the map as **unknown** and probes (`transfer/volume/DETAILS.md` § "A missing source hint means unknown"); ❌ don't add a preview path that completes without filling it — an empty map costs one round trip per source on SMB/MTP, and any consumer that guesses instead of probing is a data-safety bug waiting to happen. Per-source accounting is a before/after snapshot of the shared walk counters, so hardlink dedup still spans all sources (a later source's `dedup_bytes` share can read low; informational only). Pinned by `scan.rs::tests::walk_reports_type_and_totals_per_top_level_source`.

**Decision**: `scan_sources_internal` does NOT collect `per_path`, and `ScanResult` / `CachedScanResult` stay two structs.
**Why**: `scan_sources_internal`'s result goes straight back to its caller and never enters the preview cache, so its empty `per_path` can't be read by a consumer that would mistake it for "these sources are files". Filling it would buy a symmetry that only exists on paper, at the cost of a per-source counter bracket on the local copy/move/delete hot path. The uniformity worry underneath is real, though: an empty `Vec` is doing the job of "not collected", which is the same anti-pattern as `SourceHint::default()` one level up. **The elegant fix, when someone wants it, is a named enum** (`PerSource::NotCollected` / `PerSource::Collected(...)`), so "empty because there were no sources" and "empty because this walk doesn't collect them" stop being the same value. That has real reach (six consumers plus two crates' worth of `BatchScanResult` plumbing) and deserves its own decision rather than riding along on a safety pass. Until then the contract lives on `scan_sources_internal`'s doc comment: if that result ever starts crossing the cache, it has to collect `per_path` first.

**Compressed-size estimate (Compress dialog).** When `start_scan_preview` runs with `sample_for_estimate` set (Compress mode only), the LOCAL walk feeds a cheap deflate-sampling estimator (`compress_estimate::CompressEstimator`) that predicts the zip's output size, shown live-ish in the dialog beside the scanned byte total. Mechanics and invariants:

- **Local-FS walk only; remote is suppressed.** The per-file `WalkContext::on_file` hook fires only from `walk_dir_recursive` / `walk_cached_entries` (the `run_scan_preview` path). `run_volume_scan_preview` (SMB/MTP) never samples and never guesses — the estimate is simply `None`. Sampling a remote source would do real network reads and defeat the oracle's zero-I/O short-circuit, and an extension-only guess is unbounded (a single mistyped file → 8×-wrong), so an absent estimate is the honest choice. **Don't add sampling to the volume/oracle path.**
- **Off the walk thread.** The hook is a push on a deliberately UNBOUNDED channel (a `sync_channel` would block the walk when the sampler falls behind, e.g. an oracle-cached fast walk of a huge tree — the transient queue of `(PathBuf, u64)` is the accepted cost, and post-budget the worker drains via cheap lookups; don't "fix" this into a bounded channel). The worker thread deflates a 32 KiB head window per file at reference level 6 under an 8 MiB total byte budget, so the sampling CPU never lands on the walk's critical path and worst-case added time is ~105 ms regardless of tree size. Media-heavy trees cost near zero (an incompressible-extension table shortcuts the read). Files under 4 KiB, budget-exhausted files, and unreadable files take a running-average ratio. The worker cancels with the scan (shared `cancelled` flag) and is joined before the complete event; a sampling panic degrades to `None` and never fails the scan.
- **Per-class subtotals, scaled on the FE.** The estimate ships as three `CompressedSizeEstimate` subtotals of estimated **level-6** bytes, bucketed by each file's sampled compressibility class. The frontend re-scales to the user's selected deflate level via a baked per-class curve (`compress-estimate-scaling.ts`) with no re-scan, so moving the level slider updates the shown number arithmetically. Level 6 is the reference (shown value = sum of subtotals). It rides `scan-preview-complete` (and the `get_scan_preview_totals` recovery path) only; while scanning the dialog shows a loading affordance.
- Parameters (window, budget, tiny threshold, extension table, level curve) and their measured accuracy/cost: `docs/notes/compress-size-estimate-spike.md`.

**Progress throttled to 200 ms.** Each operation tracks `last_progress_time` and skips emitting if under the interval.

**Temp files use `.cmdr-` prefix.** Enables recoverability (recognizable leftover files after a crash).

**Symlinks never dereferenced.** All stat calls use `symlink_metadata`. Symlink loop detection uses a `HashSet<PathBuf>` of canonicalized paths.

**Every local write lands via temp + rename** (`overwrite::stage_and_land_file`). Steps: write the bytes to `dest.cmdr-tmp-<uuid>` (a sibling, so the landing rename is same-directory and therefore atomic); if an existing entry is being replaced, rename dest → `dest.cmdr-temp-<uuid>` (the aside); rename temp → dest; delete the aside. The original is intact until the landing rename completes, and the destination name never holds a partial at any observable moment. The same pattern covers file→folder overwrites (existing dest folder renamed aside, then the source file lands at the original path) and folder→file overwrites (via `safe_overwrite_dir`: existing file renamed aside, the folder materialized in place by the caller's closure, then the aside deleted; on materialize error or cancel, the aside is rolled back).

The four copy mechanisms (`copy_strategy::LocalCopyStrategy`) each hand `stage_and_land_file` a closure that puts bytes at a path, so the landing is written once rather than once per branch. ❌ Don't add a mechanism that writes straight to `dest`. Details and the no-clobber rule: `transfer/DETAILS.md` § "Local copies stage".

**Conditional conflict policies (`OverwriteSmaller` / `OverwriteOlder`)** reduce per-file. The user picks "Overwrite all smaller" / "Overwrite all older" either upfront (TransferDialog radios) or via the per-file conflict dialog's apply-to-all buttons. Each conflict re-evaluates against its own source/dest metadata: `OverwriteSmaller` overwrites only when `dst.len() < src.len()`, `OverwriteOlder` overwrites only when `dst.modified() < src.modified()`. Equal sizes / equal mtimes / unknown metadata all reduce to `Skip` — strict comparison so a borderline file is never silently overwritten. Implemented by `conflict::reduce_conditional_resolution` (sync, local FS) and `transfer/volume/conflict.rs::reduce_volume_conditional_resolution` (async, volume backends). Both log a `target: "conflict_resolution"` info line on every Skip with the reason (not-strictly-smaller, not-strictly-older, missing metadata), so users running an MTP/SMB copy who picked one of these can see in the operation log why their conflicts got skipped instead of being puzzled by silence. **The apply-to-all storage saves the *original* conditional variant**, not the reduced one — subsequent conflicts re-run the comparison against their own files.

**Validation runs inside `spawn_blocking`.** The `*_files_start` functions return an `operationId` immediately, before any filesystem I/O. Validation (`validate_sources`, `validate_destination_writable`, etc.) runs inside the handler closure on the blocking thread pool. This keeps the Tauri IPC handler non-blocking, so the frontend can always open the progress dialog and offer cancel, even if a network mount is stalled.

**`start_write_operation` emits `write-error` for handler errors.** The spawn wrapper matches on the handler's `Result`: `Ok(Ok(()))` and `Ok(Err(Cancelled))` are no-ops (handlers already emitted the right events), `Ok(Err(e))` emits `write-error` as a safety net, and `Err(join_error)` handles panics. Double-emit is harmless because the frontend's `handleError` removes all listeners on first receipt.

**A volume-aware op never turns its own `Cancelled` into a `write-error`.** The inner handler already emitted `write-cancelled` on that path, so the outer arm passes `WriteOperationError::Cancelled` through silently (`transfer/volume/copy.rs`, `mod.rs`'s safety net, and the archive-edit driver all order it this way). Re-emitting would show the user a failure for something they asked for, and the retained-failure list would have to filter it back out (§ "Retained failures" excludes `Cancelled` by typed variant for exactly this reason).

**`cancel_all_write_operations` is the quit teardown's first move, and its ONLY caller is the quit gate.** ❌ A window going away is not a reason to stop work: an operation outlives the view watching it, so nothing in the frontend may reach for this. `crate::quit::tear_down_and_exit` calls it with keep-partials semantics, waits up to 1.5 s for the operations to answer, and only then escalates. `crate::quit::DETAILS.md` § "The teardown's order".

**`abort_all_write_operations` is the second tier, and is ❌ NOT this.** It fires `WriteOperationState::backend_abort` on top of a cooperative cancel, which makes the cross-volume streaming path stop WAITING for a backend rather than asking it to stop — buying a bounded wind-down at the cost of the backend's own partial cleanup. That is a trade only a deadline holder may make, so the caller is the quit gate and nothing else; a teardown that can still afford to wait stays on `cancel_all_write_operations`. Mechanism, cost, and the invariant it must not break: `transfer/DETAILS.md` § "Two tiers of cancel". The per-operation `abort_write_operation` is `#[cfg(test)]`: a deadline always aborts everything, so production has no use for the narrower form.

**Special files skipped.** Sockets, FIFOs, and device files are filtered out during scan.

## Cmdr-own-write hook (downloads watcher)

Every write-op driver MUST register its destination with the downloads watcher's ignore set BEFORE issuing the syscall. This is what makes the watcher silently suppress events Cmdr itself caused, so the user doesn't see a "Downloaded foo.bin" toast when they just used Cmdr to copy 100 files into `~/Downloads`.

**Contract:** call `crate::downloads::note_pending_write_for_cmdr(&dest_path)` immediately before the write syscall (or the volume-trait equivalent: `Volume::write_from_stream`, `Volume::create_file`, `Volume::create_directory`, `Volume::rename`, `Volume::delete`).

**Locked-in scoping:** the prefix check lives INSIDE the helper (and the underlying `IgnoreSet::note_pending`). Call sites invoke unconditionally; paths outside the resolved Downloads root silently no-op. **Don't add `if path.starts_with(downloads_dir)` guards at call sites**: centralizing the scope in the helper keeps it from drifting across call sites (the downloads watcher's ignore-set design lives in the `downloads` module docs).

**No-op when the watcher is dormant.** If the FDA gate is closed (or `refresh_runtime` hasn't been called yet), the watcher isn't installed and the helper is a cheap no-op (single mutex `lock + is_none`). Production write ops fire freely; the cost is one atomic-bool read per write.

**Renames register both halves.** A rename moves a file out of one location into another. The Cmdr-own-write contract requires registering both the source path (so a rename-OUT-of-Downloads is also suppressed via the watcher's rename-from-ignored-source branch) and the destination path (so the rename-arrival event is suppressed). See `commands/rename.rs::rename_file` and `transfer/move_op.rs` for the pattern.

**Cross-volume writes that land on a local FS** (MTP→Local, SMB→Local) hook via the local helper inside `transfer/volume/strategy.rs::note_pending_for_local_dest` and `transfer/volume/move.rs::note_pending_for_local_volume`. They check `dest_volume.local_path()` first and skip when the destination isn't a local-FS-backed volume (MTP/SMB/InMemory) — those paths can't trigger the watcher anyway.

Example placement:

```rust
// In `copy_single_item` (transfer/copy.rs), just before `copy_file_with_strategy`:
crate::downloads::note_pending_write_for_cmdr(&actual_dest);
let bytes = copy_file_with_strategy(source, &actual_dest, ..)?;
```

See also: `apps/desktop/src-tauri/src/downloads/CLAUDE.md` for the watcher architecture, ignore-set internals, and the FDA-gated lifecycle. End-to-end safety net for the contract lives in `downloads::runtime::tests::note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end`.

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

**MCP consumer note**: the MCP server records each op's terminal outcome into a bounded ring (`mcp::terminal_ops`) at the `TauriEventSink` emit sites for `write-complete` / `write-cancelled` / `write-error`, so its `await operation_complete` can report a settled op's status. It has to tap these terminal events specifically: `operations-changed` fires AFTER removal-on-terminal, so a completed or cancelled op leaves the snapshot entirely. A FAILED op is the one exception — it stays on the snapshot as a retained row (see [Retained failures](#retained-failures)) — but the ring still owns `Completed` / `Cancelled`. See `mcp/DETAILS.md` § State stores.

## Operation manager

The full model and the why behind each decision are captured in this section. Design history is in git (former `docs/specs/2026-06-21-transfer-queue-pause-plan.md`).

`manager.rs` is the single coordinator every write op flows through. It exists because there were FIVE independent spawn paths (`start_write_operation` for local copy/move/trash + local delete; the volume-delete branch in `delete_files_start`; `copy_between_volumes`; `move_between_volumes`; `move_within_same_volume`), each hand-rolling its own `tokio::spawn` + state-insert + status-register + `WriteSettledGuard`, and an op always spawned immediately. The manager unifies them behind `spawn_managed(descriptor, state, deferred)` and adds a registry with real lifecycle states plus **lane-based admission** that can serialize ops which would thrash a shared device.

### Lanes and `Volume::lane_key()`

Each op touches the [`LaneKey`](../volume/CLAUDE.md)s of its source and destination volumes (same-volume ops touch one). Lane keys come from `Volume::lane_key()` (in `volume/mod.rs`), NEVER from parsing a `volume_id` string. Per backend: `LocalPosixVolume` → the volume root (the trait default; each local mount is its own `LocalPosixVolume`, so the root IS the mount root); `MtpVolume` → `device_id` (one USB pipe per device, so every storage on a device shares its lane); `SmbVolume` → its `volume_id` (already `smb_volume_id(server, port, share)` — server+share granularity); `InMemoryVolume` → a `with_lane_key(key)` builder, defaulting to root so the ~169 existing `new(...)` sites are untouched and tests opt into same-lane vs different-lane.

The both-local branches of `copy_between_volumes` / `move_between_volumes` compute the two lane keys from the live volume handles and pass them into `copy_files_start` / `move_files_start` as `Some(lanes)`. The plain local commands pass `None`, so the entry point derives a lane from `volume_ids` (`local_lanes`): empty → the `root` lane, else one lane per id. This is a faithful proxy for `lane_key()` on the path where no `Volume` handle is threaded through — it uses each id as an opaque whole, with no substring parsing.

### Admission — global FIFO, atomic multi-lane reservation

The manager keeps one ordered queue (`order`) plus a `lane_use` table (lane → in-use count; budget 1 per lane in v1, a lane is free iff its count is 0; a `HashMap` not a set so v2 budgets > 1 reshape nothing). An admission pass walks pending ops oldest-first and admits the first whose EVERY lane is free, reserving all its slots atomically, flipping it to Running, registering its volumes busy, and spawning its deferred start. It loops so one pass can admit several disjoint-lane ops. A two-lane op can't starve behind churn on a single lane — there are no per-lane queues, so the multi-lane op is always considered at its FIFO position against the whole lane table.

### Deferred start, not "spawn then block on a semaphore"

A queued op holds only DATA describing how to begin: a boxed `FnOnce() -> Pin<Box<dyn Future + Send>>` (`DeferredStart`). The manager spawns it only on admission. Blocking a spawned op on a lane semaphore would pin a `spawn_blocking` pool thread idle per queued op — a leak that can deadlock the finite pool under many queued ops. Each deferred future owns the op end-to-end (the `WriteSettledGuard`, the actual transfer/delete, the terminal-event emit) and ends by calling `manager().on_settled(id)`.

### Dequeue on settle — explicit, NOT in `Drop`

`on_settled(id)` (the happy path) frees the op's lane slots, removes it from the registry, cleans `WRITE_OPERATION_STATE` + the status cache, and runs an admission pass (which may spawn the next op). It's sequenced after the terminal event, exactly where the old per-site cache cleanup ran. The `ManagedTaskGuard` is the panic safety net: held by each spawned task, its `Drop` frees lanes + cleans caches but NEVER spawns (no admission pass). Spawning during the previous op's unwind would re-enter the manager mid-panic (abort) or deadlock on a lock held up-stack. So a panicking op still releases its lanes, but the next op is admitted only on a healthy settle (the next registration's admission pass, or another op's `on_settled`, picks it up). The happy path calls `task_guard.disarm()` right before `on_settled` so its now-redundant Drop is a no-op. Pinned by `manager::tests::panicking_op_releases_its_lane_without_spawning_next`.

### Observing an admission pass (`admission_passes`)

Admission is otherwise invisible from outside: a pass that walks the queue and admits NOBODY changes no status, emits no event, and touches no lane. The negative assertions ("B must still be Queued after A settles") need exactly that moment, and without a signal for it a test can only guess at a wall-clock span.

`OperationManager::admission_passes` is an `AtomicU64` bumped ONCE at the end of `run_admission_pass`, read by `admission_pass_count()` (test-only). A test reads the count, triggers the settle, and waits for it to grow: an advance means a pass walked the whole FIFO queue and either admitted the waiting op or declined to. Three properties make it work:

- **Bumped LAST, and on every exit.** The signal means "the pass finished", so a new early `return` inside `run_admission_pass` that skips the bump would hang every waiting test. Nothing in production reads the count, so the bump is pure signal, never a decision input.
- **`SeqCst` on the bump and the load,** not `Relaxed`. The waiter reads the count, acts, and waits for it to advance, so the bump has to be ordered against the admission work it claims finished; a `Relaxed` counter buys a rarer, harder flake than the sleep it replaces.
- **Global count, global queue.** A pass triggered by an unrelated concurrent test is equally good evidence, because a pass considers every registered op, not just its trigger's.

`Notify` would be wrong here and `watch` is second-best: the settle path runs the pass BEFORE a test could await, so `notified()` created afterwards is a guaranteed lost wakeup. Prefer a monotonic counter for any future "background work reached a point" signal.

`force_admission_pass()` (test-only) runs a pass inline and returns when it's done, for the paths production never runs one on: `set_paused` admits nobody, so `paused_running_op_does_not_admit_a_queued_same_lane_op` runs its own pass and asserts B is STILL Queued. Admission flips a record to Running before it spawns, so a Queued status after a completed pass also proves the deferred start never ran. The waits themselves use `crate::test_support::wait_until_async` (`docs/testing.md`).

### The admission pass spawns admitted ops on the APP runtime, not the caller's

The admission pass (from `spawn_managed` or `on_settled`) spawns each admitted op's deferred start with **`tauri::async_runtime::spawn`, deliberately NOT `tokio::spawn`**. `tokio::spawn` binds to whatever runtime is current when the pass runs — and the pass can run on a runtime that has nothing to do with the op it's admitting. Admission is global and there is a lock-free window between an op's registration (`spawn_managed` inserts it Queued, drops the lock) and its own admission pass: a CONCURRENT op's `on_settled`, running on a different runtime, can reach the pass first and admit the freshly-registered op. So the runtime that spawns an op is racy, not "the op's own caller". `async_runtime::spawn` pins every op to the one process-global runtime that outlives them all.

In production this is a no-op (there is exactly one long-lived Tauri runtime, so ambient and app runtime are identical). **The guard exists for the process-global-manager + per-runtime-caller topology**, which is the test harness: every `#[tokio::test]` runs on its own runtime, and with a bare `tokio::spawn` an op admitted by a runtime that is then torn down is orphaned — it never runs, never settles, and leaks its lane forever, wedging later ops (the observed nondeterministic `wait_until` timeouts). Pinned by `manager::tests::admitted_op_runs_even_if_the_admitting_runtime_is_dropped` (a throwaway runtime admits an op and is dropped without driving it; the op must still complete). This race is lane-INDEPENDENT — it hit even unique-lane ops — so a hermetic-lanes-only test fix could not close it; the guard is the actual fix.

Independently, the archive-edit tests still give each op its OWN lane (`archive_edit::test_support::unique_lane_id`, and `InMemoryVolume::with_lane_key`), matching the `manager::tests` discipline ("unique operation ids + lane keys"). That's for test ISOLATION and parallel speed — a shared global lane serializes unrelated tests and couples their timing — not for orphan-safety, which the guard now owns. The one behavior that needs a `"root"` id (a root parent settling with `None`) is pinned by `move_out_tests`, whose lanes come from the volume objects, so it passes a `"root"` settle id WITHOUT reserving the `"root"` lane.

### Lifecycle status and `operations-changed`

`LifecycleStatus` (Queued/Running/Paused/Done/Cancelled/Failed) lives on the manager record. The snapshot also carries
`supports_rollback` (see below) and `error` (see [Retained failures](#retained-failures)). Admission and settle set Queued/Running and removal-on-terminal; the pause/resume path sets the `Paused`↔`Running` flip (see [Pause / resume](#pause--resume)). It is distinct from `WriteOperationPhase` (the progress phase) and `OperationIntent` (the cancel/rollback machine). The `operations-changed` typed event carries a THIN snapshot (`Vec<OperationSnapshot>`: id, type, status, source/dest summary), emitted from `spawn_managed` / `on_settled` / `cancel_if_queued` / `set_paused` / the two dismiss commands. It deliberately excludes 200 ms progress — the queue window reads the per-file `write-progress` stream for live bars. `init_operation_event_emitter(app)` wires the emitter at startup (`lib.rs`), mirroring `init_busy_volume_emitter`.

A live RECORD only ever holds `Queued` / `Running` / `Paused`: `Done` and `Cancelled` are declared but never assigned, because `on_settled` deletes the record before anything could set them. `Failed` is the one terminal status that reaches a snapshot, and only through the retained-failure list below, never on a record.

### Retained failures

**The exception to removal-on-terminal.** Nothing else can hold the failure of an operation that was backgrounded: the record is deleted on settle, and `write-error` only reaches a window that is listening at that moment — and the queue window being closed is the exact scenario this is for. So the manager keeps a bounded list of failures OUT OF BAND — `ManagerInner::failures`, a `VecDeque<OperationSnapshot>` capped at `FAILURE_CAPACITY = 20`, oldest evicted first — and `snapshot()` appends them after the live rows.

- **`free_and_remove` is untouched.** Admission, laning, the busy-volumes set, and settling behave exactly as before: a failed op frees its lane slots, cleans its caches, deletes its record, and admits the next op on the same `on_settled` it always did. Retention is a separate structure the settle path never consults. Pinned by `manager::tests::a_failed_op_frees_its_lane_and_admits_the_next_exactly_as_before`.
- **Recorded at the emit site.** `TauriEventSink::emit_error` calls `record_failure`, next to the `mcp::terminal_ops::record` line. ❌ Not in the `OperationEventSink` trait and not in `CollectorEventSink`: test sinks stay side-effect-free.
- **⚠️ A failure whose record is still LIVE is filtered out of the snapshot.** `emit_error` runs inside the op's own task, before `on_settled` removes the record, so for a moment the op is both running and failed. Emitting both rows would put one `operationId` in the list twice, and the queue window's keyed `{#each}` throws on a duplicate key. `record_failure` therefore stays silent whenever the record is live; the row surfaces on `on_settled`'s existing `emit_changed`, which is the honest moment anyway. It DOES emit on the other branch, where the record is already gone: no duplicate is possible without a record, and no `on_settled` is coming, so the row would otherwise sit unbroadcast until some unrelated emit — no toast, no chip, nothing until the queue window next opens. Pinned by `a_failure_with_no_live_record_broadcasts_itself`.
- **Not every `write-error` is a failure.** `WriteOperationError::Cancelled` (some volume paths emit it) and `ArchiveNeedsPassword` (a recoverable prompt the frontend answers and retries) are excluded by TYPED variant match, never by message text.
- **First error per id wins.** `write-error` can fire twice for one op — an inner handler emits and returns `Err`, then `mod.rs`'s safety net emits again — and the first one is what actually stopped it.
- **A retained row reports `supports_rollback: false`** and reuses the live record's source/destination summary, so it reads like the running row it replaces. If the record is already gone, it falls back to the event's own `operation_type` and no summary.
- **Only an explicit action clears one**: `dismiss_failed_operation(id)` or `dismiss_all_failed_operations()`. ❌ Never a timer, ❌ never a window close, ❌ never "the next operation starting". A 40-minute copy that failed while the user was away has to still be there.

**Decision: retention lives in the manager, not the frontend. Why.** The failure has to reach two separate webviews, and it can land while either is closed. A store in the queue window misses it outright (that window is closed in the exact scenario this exists for); a store in the main window can't be read by the queue window and would need a hand-rolled state server over `emitTo`, inverting the dependency. The operation log (`journal.rs`) is the durable history and stays that, but it records an `ExecutionStatus`, not the typed `WriteOperationError` the frontend's message pipeline needs, and browsing the past is a different job from a live notice. The manager already owns membership and already broadcasts it to both windows, so retention there needs no new event, no new listener, and seeds correctly through the existing `list_operations` on window open.

**Decision: runtime-only, capped at 20. Why.** The cap and its reasoning mirror `mcp::terminal_ops::CAPACITY`: enough that a user returning from lunch still finds what went wrong, bounded so a long batch session can't grow memory without limit. A restart clears the list, which is consistent with the rest of the manager's state and correct in kind — the operation log is where a failure lives permanently; this list only exists to make one visible right now.

### Rollback availability (`supports_rollback`)

`OperationDescriptor::supports_rollback` says whether cancelling this op can also UNDO what it has written
(`cancel_write_operation(id, rollback = true)`), and it rides through to `OperationSnapshot` so the queue window can
offer Rollback on exactly the rows the progress dialog would. It's on the snapshot because the operation queue window is its
own webview: it never sees the source/destination volume ids the dialog decides from, and two surfaces disagreeing about
whether an operation is reversible is the kind of drift that ends with a button that lies.

Every construction site states its own verdict (a struct literal, so a new spawn path can't forget to decide):

- **`true`** — local copy/move via `start_write_operation` (`CopyTransaction` deletes the copies it made;
  `MoveTransaction` renames them back), and volume copy (`volume/cleanup.rs` deletes the destination copies).
- **`false`** — delete and trash (the files are already gone; there's nothing to put back); a SAME-volume move (a
  server-side rename-merge that stops without reversing); a CROSS-volume move, which copies and deletes the source per
  file and whose driver treats `RollingBack` exactly like `Stopped`, reporting `rolled_back: false`; archive edits (an
  all-or-nothing temp+rename rewrite); instant metadata ops; and the operation-log rollback's own inverse op (rolling
  back a rollback would re-apply what the person just undid).

⚠️ The progress DIALOG doesn't read this flag yet: it decides from the volume ids it holds, disabling Rollback only for
a same-volume move. So it still offers Rollback on a cross-volume move, where the click only cancels. Pointing the
dialog at this flag (or teaching the cross-volume move driver to reverse) is the fix; until then, the operation queue window
is the honest one.

The flag is a property of the op's STRATEGY, so it never changes over the op's life. Whether Rollback is offered *right
now* is the UI's call on top of it: the queue row also requires a running/paused op that isn't already rolling back
(which it reads from the live `write-progress` phase, since rollback is an `OperationIntent`, not a `LifecycleStatus`).

### IPC

`list_operations` (the thin snapshot), `cancel_operation(id)`, `cancel_operations(ids)` (the queue window's "Cancel selected"), `pause_operation(id)` / `resume_operation(id)`, and `pause_all` / `resume_all`. Cancel routes through `cancel_operation`: a Queued op is dropped from the registry without ever spawning (`cancel_if_queued`); a Running/Paused op falls through to the existing `cancel_write_operation(id, rollback=false)` keep-partials path. Pause/resume flip BOTH the live `WriteOperationState` pause gate (so the driver parks) AND the manager record's `LifecycleStatus` (so the UI shows Paused), via `set_paused`. Plus `dismiss_failed_operation(id)` / `dismiss_all_failed_operations()`, which drop retained failures and re-emit ([Retained failures](#retained-failures)). Registered in `ipc.rs` + `ipc_collectors.rs`; `OperationSnapshot` / `LifecycleStatus` / `OperationsChanged` ride into `bindings.ts`. No capability change: manager commands go through the invoke handler, not the ACL.

### Pause / resume

The paused bit has TWO homes, kept in sync by the IPC layer: a `PauseGate` on `WriteOperationState` (the runtime gate the drivers honor) and the manager record's `LifecycleStatus::Paused` (what the UI sees in `operations-changed`). Pause is **orthogonal to `OperationIntent`** (which stays the cancel/rollback machine) — it never perturbs the validated `Running → RollingBack/Stopped` transitions — and it is **not a `WriteOperationPhase`** (a paused op may be mid-`Copying`).

- **`PauseGate`** (`operation_intent.rs`): a `paused: AtomicBool` plus a `std::sync::Condvar` (for the sync driver, which parks inside `spawn_blocking`) and a `tokio::sync::Notify` (for the async volume drivers). `pause()` sets the flag; `resume()` clears it and wakes both waiters; `wake()` wakes both WITHOUT clearing (the cancel path uses it). `wait_while_paused_sync(&intent)` / `wait_while_paused_async(&intent).await` park while `paused && !cancelled` and return immediately on cancel.
- **Gate placement** (between-files boundaries, immediately AFTER the `is_cancelled` check so the data-safety ordering — cancel/skip before any destructive call — is preserved): both transfer drivers' per-source loop tops (`transfer_driver.rs`), and the delete-phase loops in both delete walkers (files then dirs, `delete/walker.rs`). The delete SCAN recursion is NOT gated (pausing mid-enumeration would freeze a half-counted "Scanning…"). The cross-volume streaming copy path ALSO parks BETWEEN CHUNKS via the `CheckpointStream` wrapper in `transfer/volume/strategy.rs` (the sync per-chunk `on_progress` callback can't `.await`, so the async stream decorator owns mid-file parking + a `yield_now`), so a paused single large file (e.g. MTP→local) stops mid-stream holding only its `.cmdr-tmp-<uuid>`. The local-FS sync chunk loop (`chunked_copy.rs`) still pauses only between files — it receives the cancel atom, not the `PauseGate`. Full rationale + scope: `transfer/DETAILS.md` § "Pause reaches between chunks".
- **Cancellation always wins over pause.** `cancel_write_operation` / `cancel_all_write_operations` flip the intent AND call `pause_gate.wake()`, so a paused, parked op unblocks, observes the non-`Running` intent, and bails through the existing keep-partials path (keeping already-copied files, deleting only the last partial). Without that wake a paused op parked on the condvar would never see the cancel.
- **A paused Running op keeps its lane slots** (`set_paused` never touches lanes), so a same-lane Queued op can't start and then fight it on resume. Resume runs NO admission pass (the op never freed its lanes). Pausing a Queued op is a v1 no-op (it isn't touching a device yet; it stays Queued and admits normally when its lanes free). Pinned by `manager::tests::{set_paused_flips_running_op_to_paused_and_keeps_its_lane, paused_running_op_does_not_admit_a_queued_same_lane_op}`.
- **Concurrent copy path.** `copy_volumes_with_progress`'s `FuturesUnordered` path has no single between-files boundary, and its per-file `on_progress` callback stays cancel-only (pinned by `transfer_driver::tests::concurrent_per_file_callback_is_cancel_only_not_pause_aware`). But its in-flight files stream through the shared `stream_pipe_file`, so each parks between chunks via `CheckpointStream` when paused; the admission loop adds no new files while everyone is parked, so the batch effectively halts. Serial paths (local copy/move, cross-volume serial, delete) honor pause between files; the cross-volume paths additionally park between chunks. See `transfer/volume/DETAILS.md` § "Pause and the concurrent copy path".
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

## The scan-wait

A confirmed transfer is registered with the manager immediately, before its `TransferDialog` preview has finished
walking. That is what gives it an `operationId`, a queue row, its lanes, a busy-volume entry, and a place in the quit
gate from the moment the user confirms — the whole point, because before this a user could not background a scanning
transfer at all and the scan died with its dialog. The wait itself moved into the operation's own task
(`scan_bridge::await_claimed_preview`), which every deferred start calls first, BEFORE its journal open: an operation
that never got past its scan wrote nothing, so journaling one would record work that didn't happen.

**Claims, and why exactly one.** An operation claims its `preview_id` inside `spawn_managed`, before the record is even
inserted. The claim is what lets the progress bridge forward the walk's counts under the operation's id, what exempts a
settled result from TTL eviction while its owner sits Queued behind a busy lane (with `LANE_BUDGET = 1` that can be
well past five minutes), and what stops `cancel_scan_preview` from freeing a result an operation is about to read. A
SECOND operation naming the same id is refused and falls back to its own walk: `take_cached_scan_result` REMOVES what
it reads, so two claimants would race for one consumable result and the loser would silently get nothing. Not
hypothetical — the archive-password retry re-dispatches a new operation over the same sources, which is why
`dialog-state.svelte.ts` clears `previewId` on that path.

**The terminal outcome, not a completion pulse.** Both preview workers used to remove their in-flight state before
publishing a result, so an operation looking the preview up could find nothing and had four indistinguishable reasons:
complete-and-consumed, errored, cancelled, and never-existed. With `LANE_BUDGET = 1` "nothing there" is the common
case, not the rare one. `settle_preview` replaces the entry in one write with a `ScanOutcome`, readable afterwards.
⚠️ `Cancelled` comes from the worker's own cancel FLAG at its exit, ❌ never from which event fired: a genuinely
cancelled walk returns an error (the local walk's `on_cancelled` string, the volume path's
`"Scan failed: {VolumeError::Cancelled}"`), so classifying on the event would reach the operation as a failure whose
message merely says "cancelled", and recovering the truth from that message would be string-matching on the control
path. Both workers' arms were reconciled to match.

**Trash is the one operation that doesn't wait.** `trashItemAtURL` is atomic per top-level item, so a trash walks
nothing: there is no second walk to serialize against and no cached result to consume, and waiting would be pure delay
— a long one on a big tree. `trash_files_start` therefore passes no `preview_id` and frees the preview outright,
because nothing downstream will read it and the dialog deliberately skips its own cleanup after a confirm (on the
DELETE path the operation DOES consume it). Pinned by `a_trash_frees_its_preview_instead_of_waiting_on_it`.

**Misses are always a re-walk, never a hang.** An unknown `preview_id` (evicted, or stale from a reloaded window) and a
refused claim both leave the operation with no claim at all, so it proceeds straight to its own foolproof scan.

**The progress bridge.** `scan-preview-progress` is keyed by `previewId` and carries no `operationId`, and nothing else
emits for an operation that is only waiting, so without a bridge every scan-phase surface would render zeros rather
than live counts. Each preview tick for a CLAIMED preview is republished as the owner's `write-progress` in
`phase: 'scanning'` (`scan_bridge::forward_scan_progress`), alongside — never instead of — the preview event, since a
pre-confirm dialog may still be watching the same preview by id. `files_total` / `bytes_total` stay 0 through the scan;
the index expectation rides `expected_*`, which every surface already treats as a hint.

⚠️ **The opening tick's ORDERING is load-bearing.** `row.progress` must stop being `null` immediately rather than at
the next `progress_interval_ms` boundary, which on a preview near its end may never arrive. But `spawn_managed` inserts
the record, runs admission, and only THEN calls `emit_changed()`, while the frontend store's `applyProgress`
early-returns for an id with no snapshot row yet. A tick that beats its own `operations-changed` is discarded and the
row stays blank — exactly the case the tick exists for. So `emit_initial_scan_tick` fires AFTER `emit_changed`, for a
`Queued` row as much as a `Running` one (the queued row renders the scan line too, and its task may not spawn for
minutes). `scan_bridge_tests` asserts the ordering against the manager's broadcast counter, not merely the tick's
existence.

**Pause is refused, and the refusal is LATCHED.** `set_paused` flips any `Running` record, and a scan-waiting operation
is `Running`, so without a rule the snapshot would say `Paused`, the dialog title would say "Paused", and the walk
would carry on at full speed — while `set_paused` deliberately keeps the lane slots, so a "paused" scan would hold its
lane indefinitely doing nothing. The refusal is one match arm on the record's `in_scan_wait` flag, and it is already
observable everywhere it matters: no surface flips optimistically, so a refused pause shows as "the status stayed
`running`, the button still says Pause". No return-type change, no `bindings.ts` regeneration, no new agent-facing
string.

The latch is the part that would otherwise ship as a real defect. `pause_all` walks `running_ids()` calling
`pause_operation`, which sets the driver's park gate only if `set_paused` returned true — so a bare refusal drops the
request on the floor, and minutes later that one operation starts writing at full speed while every other operation is
paused and the user believes the device is free. The refusing arm records the request on the record, and
`end_scan_wait` applies it the moment the wait ends. Withdrawing (a resume during the scan) clears it the same way.
Nothing surfaces the PENDING pause: the row keeps saying "Running", then flips to "Paused" on its own when the write
would have begun. Showing "pause pending" would mean a new snapshot field and a new string, and the harm being fixed is
the silent full-speed write, not the surprise. Revisit if the delayed flip confuses anyone.

**Real parking was rejected**, and the volume path settles it rather than taste: a local walk could poll a pause flag
per entry the way it polls `cancelled`, but a volume scan sits inside `scan_for_copy_batch_with_progress` for a whole
batch, so there is no park point, and on MTP the batch can be the entire scan. Pausing would therefore work on the
volume kind that needs it least.

**Two leaks the wait creates, and their hooks.** (1) Every exit from the scan-wait has to reach `on_settled` or the row
leaks and its lane stays reserved; the operation's task owns that, not the detached scan worker, and `free_and_remove`
also abandons any claim still held (the panic / quit-drop net). (2) `cancel_if_queued` removes a Queued record WITHOUT
ever running its `DeferredStart`, which is where the wait and its cleanup live — so it calls `abandon_claim` explicitly.
A `Queued` op cancelled before admission is the ordinary case on a busy lane, and without the hook its walk would keep
going for an operation that no longer exists while its result sat until a TTL sweep. These are separate leaks and no
single test catches both.

**The lanes are reserved from confirm**, and this is the one genuinely contested decision here. For: with
`LANE_BUDGET = 1` a transfer confirmed while another runs on the same lane is admitted as `Queued` and the existing
auto-queue path backgrounds it with no new code; and admission is oldest-first, so operations run in the order the user
confirmed them, which they did NOT before (A dispatched only after its scan, so B, confirmed second with a finished
scan, took the lane first). Against: an operation scanning for three minutes holds a lane it is not writing to, so a
destination device can sit idle. Matching confirm order is a correctness property and better utilization is an
optimization, so reserve from confirm. Two-phase reservation (scan without lanes, request them at write start) costs
the invariant that `Running` means "admitted and holding its lanes" and adds a second admission point that can leak a
lane: reach for it only if the idle-device cost shows up in practice.

**Two behavior changes worth naming.** Eject is disabled from confirm rather than from first byte (the operation is
committed, so that is right, but it is new). And an operation confirmed onto a busy lane is `Queued` from registration,
so the progress dialog's one-shot `listOperations()` seed sees `queued` immediately and the auto-queue path mounts and
unmounts the modal within a frame or two, with a toast and a queue window. That flash is not new, only more frequent;
fixing it properly means moving dispatch out of the dialog.

**Archive routes await but do not reuse.** `copy_into.rs` plans its changeset with its own `WalkDir` and never calls
`take_cached_scan_result`, so `preview_id` is threaded through `route_archive_copy_into`, `compress_start`, and
`route_archive_delete` for the WAIT alone: it restores the serialization the frontend's scan-wait used to provide.
Without it, ⌥F5's sampling preview and the planner's walk would run down the same tree at once, which is what costs on
MTP and SMB. The duplicate walk itself is pre-existing and out of scope; teaching the archive changeset planner to seed
from a `CachedScanResult` is real work with its own correctness questions.

**Testing the scanning window.** E2E fixture trees are deliberately tiny and `data-scan-state` signals "counting done",
the opposite of what a scanning-phase test needs to hold, so `set_test_scan_preview_delay` (an IPC override behind the
`playwright-e2e` feature, falling back to `CMDR_E2E_SCAN_PREVIEW_DELAY_MS`) holds every worker at its starting line for
a set number of milliseconds. Per-test rather than per-process, so one spec's window doesn't slow the whole run.

## Archive edits

Editing a `.zip` (mkdir/mkfile/rename/delete inside, or copy/move INTO one) is an O(archive) temp+rename rewrite, not a
metadata syscall, so it runs as a managed op through `spawn_managed`, NOT `run_instant`. The `archive_edit/` module is the driver;
the mutation mechanism (`ArchiveMutator`, temp+rename safe-overwrite) lives in the archive backend
(`crates/cmdr-archive/src/mutation/DETAILS.md`).

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
- **Compression level threads from the op config onto the changeset.** `VolumeCopyConfig::compression_level` (frontend-owned, read from the `behavior.archiveCompressionLevel` setting at dispatch) is passed through `compress_start` / `route_archive_copy_into` as an `Option<i64>` param and stored on the `Changeset` (`archive_copy_into_start` sets `plan.changeset.compression_level` before `mutator::apply`). It governs every user-driven zip write uniformly — compress AND copy/move INTO an existing archive — because both funnel through the shared mutator. `None` (no caller opinion, or a non-archive copy) means the crate default (level 6). The level applies to NEWLY added entries only and is clamped 1..=9; the mechanism and the clamp rationale are single-sourced in `crates/cmdr-archive/src/mutation/DETAILS.md` § "Compression level applies to ADDED entries only". Internal zips (crash/error-report bundles) keep their own fixed level and never read this setting.
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
  and progress semantics are pinned in `crates/cmdr-archive/src/mutation/mutator_test.rs`), and killing them would need flaky
  timing-based tests — not worth it per the mutation-score guidance.

## Busy-volumes set

Drives "disable Eject while an op reads from / writes to this device" so a disconnect can't truncate an in-flight file. Lives in `status_cache.rs`, alongside the cache it derives from: `recompute_and_emit_busy_volumes` reads that module's own `OPERATION_STATUS_CACHE`, and register / unregister are the only two places membership can change, so the two can't be separated without putting a private static on the wrong side of a module boundary.

The same two register/unregister functions also feed `crate::priority::transfers` (the per-volume "a transfer is
running" gauge indexing yields to — transfers trump indexing). ONE lifecycle choke point on purpose: a new op kind that
guards eject automatically also outranks indexing, and the panic-safe unregister paths below cover both signals. Don't
add a second feed site (see `../../priority/CLAUDE.md`).

- The manager registers an op's volume IDs busy (`register_operation_status(op_id, type, volume_ids)`) **only when it admits the op (Running)** — a Queued op isn't touching the device, so it marks nothing busy. Source **and** destination go in (a download from a phone is as corruptible as an upload to it). The manager's `on_settled` / `ManagedTaskGuard` Drop unregisters on every exit (including panic), so a finished or panicking op can't leave a volume stuck busy.
- The busy set is the union of every Running op's `volume_ids` **∪ external registrations**, minus `root` (never ejectable). `recompute_and_emit_busy_volumes` fires `volumes-busy-changed` only when membership changes — progress ticks don't churn it (`LAST_EMITTED_BUSY`). Membership-by-union means two concurrent transfers to one device keep it busy until both finish, with no manual refcount.
- **Where `volume_ids` come from**: the `OperationDescriptor` each spawn site hands the manager. The cross-volume entry points (`copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`) and the volume-aware delete carry the IDs; the both-local branch of `copy_between_volumes` (a local→USB / DMG copy) passes both IDs through `copy_files_start` / `move_files_start` so the ejectable destination is still marked. The plain `copy_files` / `move_files` / `trash` commands pass an empty list — the unified transfer dialog only routes through them for same-`root` ops, where no ejectable volume is involved.
- **Consumers**: `busy_volume_ids()` backs the `get_busy_volume_ids` bootstrap command, the `eject_volume` server-side guard (refuses a busy volume — the real safety net, since the picker's disable is only UX), and the native breadcrumb-menu builder (renders the Eject item disabled with a ` (busy)` suffix). The frontend `volume-busy-store.svelte.ts` subscribes to `volumes-busy-changed` and exposes `isVolumeBusy(id)` to disable the picker's eject controls. `init_busy_volume_emitter(app)` wires the emitter at startup (`lib.rs`).
- **External (non-write-op) seam**: the drag-out file-promise fulfillment service (`native_drag::fulfillment`) marks the source volume busy while it streams a promise to a Finder destination, but it isn't a real write op (no `WRITE_OPERATION_STATE`, no progress events, no settle). The `pub(crate)` `register_external_volume_op(op_id, volume_ids)` / `release_external_volume_op(op_id)` pair (in `status_cache.rs`, surfaced through `state::` and re-exported from `mod.rs`) is the seam: it touches only the `OPERATION_STATUS_CACHE` half that `recompute_and_emit_busy_volumes` reads, registering under `WriteOperationType::Copy` (the type only affects `list_active_operations` diagnostics; the busy set is type-agnostic). The fulfillment side wraps it in an RAII guard so release fires on every exit path.

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
**Why**: A hardlink contributes differently to the two operations. **Delete** frees an inode only when its last link is removed, so the bytes-freed number is the dedup'd one — counting every link would claim to free 80 GB when only 60 GB (cargo `target/`) actually frees. **Copy/move** materialize every hardlink as an independent file at the destination (hardlinks don't survive a cross-volume copy, and even a same-FS `cp` doesn't relink), so the bytes-written number — and the disk-space reservation — is the full write footprint. The earlier single-`total_bytes`-is-dedup'd design got delete right but silently regressed copy: the space check under-reserved (risking ENOSPC mid-copy) and the bar hit 100% early. Now `walk_dir_recursive` / `walk_cached_entries` / `scan_volume_recursive` / `LocalPosixVolume::scan_for_copy` / `scan_subtree_with_oracle` all track both, using a `seen_inodes: HashSet<u64>` (mirrors `indexing/scanner/mod.rs`, `nlink == 1` fast path, operation-scoped across source roots; **Unix-only**, where non-Unix has no `nlink()` so `dedup_bytes == total_bytes`). Volume backends populate `FileEntry::inode` only for `LocalPosixVolume` files with `nlink > 1` (MTP/SMB/InMemory leave it `None`, so dedup is a no-op and the two totals are equal). The **scan-phase** progress bar reports the dedup'd running total (it's compared against the indexer's inode-dedup'd `dir_stats` estimate, so reporting the write footprint would overshoot 100% on hardlink trees). The **delete** active phase sums per-entry `progress_bytes`/`VolumeDeleteEntry::progress_bytes` (= dedup'd) against the `dedup_bytes` denominator. The **copy** active phase credits full per-file `size` against the `total_bytes` denominator (no chunk scaling). The Copy dialog surfaces the gap with a one-line note ("X will be written; source is Y; the extra is hardlinked files…") via `dedup_bytes_total` on the scan-preview events — copy-only, since a same-FS move writes nothing. Pinned by `delete/hardlink_progress_tests.rs`, `delete/volume_hardlink_progress_tests.rs`, `transfer/hardlink_progress_tests.rs::copy_counts_write_footprint_for_hardlinks`, `scan.rs::tests::walker_dedupes_*`, `local_posix_test::test_scan_for_copy_dedupes_hardlinks_for_source_size_only`, and `transfer-dialog-utils.test.ts::shouldShowHardlinkNote`.

**Decision**: `WriteProgressEvent::with_scan_meta` is the only path that sets the scan-only fields (`current_dir`, `dirs_done`, `expected_files_total`, `expected_bytes_total`).
**Why**: 20+ emit sites construct `WriteProgressEvent` literals for active-phase events. Adding four optional fields to the struct would force every site to spell out their defaults, pure mechanical noise. The `new(...)` constructor takes the eight core counter fields and defaults the scan meta (`None` / `0`); the scan emit sites in `scan.rs`, `scan_preview.rs`, and `delete/walker.rs::scan_volume_recursive` opt in via `.with_scan_meta(current_dir, dirs_done, expected)`. Future scan-related fields go through the same builder. If a real refactor of the 20 literals to `new(...)` ever happens, the builder pattern still composes cleanly on top.

**Decision**: All write operations go through `OperationEventSink` instead of `tauri::AppHandle`, and the sink is constructed **only at the IPC edge** (`commands/file_system/write_ops.rs` + `commands/file_system/volume_copy.rs`), then injected all the way down.
**Why**: Decouples the copy/move/delete/trash orchestration from the Tauri framework. `TauriEventSink` wraps AppHandle for production; `CollectorEventSink` stores events for test assertions. The whole managed layer — `start_write_operation`, the four starters, the volume entry points (`copy_between_volumes` / `move_between_volumes` / `move_within_same_volume`), every `*_with_progress` function, and `WriteSettledGuard` — takes `&dyn OperationEventSink` / `Arc<dyn OperationEventSink>`, never an `AppHandle`. Each command builds `Arc::new(TauriEventSink::new(app))` once and passes it in (grep confirms zero `TauriEventSink::new` under `write_operations/`). This lets the full pipeline (multi-file copy, cancellation, conflict resolution, progress, the managed spawn path, and settle) run end-to-end under a `CollectorEventSink` with no Tauri runtime — see `tests.rs::injected_sink_receives_complete_and_settled_for_local_copy` and the trash unit tests (`delete/trash.rs::tests::trash_*_via_sink`). `state.emit_progress_via_sink` is the only progress-emit method — `emit_progress_via_app` is gone. The write-error safety-net arms in each deferred also route through `sink.emit_error(...)` rather than a string-named `app.emit("write-error", ...)`.

**Decision**: Scan preview reuses watched listings (the "fresh-listing oracle").
**Why**: Pre-flight scans for copy/move on MTP (and to a lesser degree SMB and big local trees) used to duplicate work the backend already had in `LISTING_CACHE`. Selecting 135 photos in a watched `/DCIM/Camera` (~15k entries) and pressing F5 would re-list the parent dir over USB just to look up size by name — ~17 s of "Verifying before copy…" while the listing was already fresh on the pane behind the dialog. `run_volume_scan_preview` now groups input sources by parent dir and consults `try_get_authoritative_listing(volume_id, parent)` first. On hit, sizes and `is_directory` flags come from the cached `FileEntry` for top-level files; top-level directories recurse via `scan_subtree_with_oracle`, which re-applies the oracle at every level (so a subfolder open in another pane also short-circuits). On miss, the call falls through to `volume.scan_for_copy_batch_with_progress(paths_in_group, ...)` — same code path as before — so MTP's parent-grouping and SMB's pipelined-stat optimizations still run for cold-cache parents. The local-FS walker (`walk_dir_recursive` in `scan.rs`) also takes an oracle check at the top of each recursive call, with `volume_id = "root"` plumbed through from `scan_sources_internal` and `run_scan_preview`. The freshness contract is bright-line at the watcher boundary: no "5 seconds is fresh enough" TTL, just "the volume's `listing_watch_coverage(path)` returned `EveryWriter`." See `file_system/listing/caching.rs::try_get_authoritative_listing` for the per-backend debounce windows that contract tolerates.

**Decision**: Copy and move are durable before they report complete: per-file `sync_data` (fdatasync) in chunked copy, plus an end-of-op targeted `fdatasync` pass over the transaction's recorded destinations for the strategies that don't flush themselves. Delete and trash don't sync at all.
**Why**: "Complete" must mean "durable on disk," not "buffered in the OS page cache." Without it, a user who copies to a USB stick / SD card and ejects (or the machine sleeps) right after "Copy finished" loses the file — and on a move it's gone from both source and dest. The flush is targeted, not a whole-machine `libc::sync()`: that global sync also stalled unrelated apps (AGENTS.md principle #5). The mechanism: (1) `transfer/chunked_copy.rs` calls `dst_file.sync_data()` per file, so each file is durable as it completes — a crash mid-batch on a long transfer leaves earlier files safe. (2) Before emitting `write-complete`, `durability::flush_created_destinations` emits a `Flushing`-phase progress event, then `fdatasync`s every recorded destination that wasn't already flushed, plus a best-effort `fsync` of each distinct parent directory so the rename-into-place (temp+rename / cross-FS staging) is durable too. It reuses `CopyTransaction.created_files` (no parallel dest-tracking) and skips an `already_synced: HashSet` of paths the strategy already made durable: chunked-synced files and APFS-clonefile / reflink dests (those share copy-on-write extents with the source, so a flush is moot). On macOS every produced-bytes path is either clonefile (moot) or chunked (already synced), so the end-of-op pass does no extra `fdatasync` there — its job on macOS is purely the honest `Flushing` UI state; on Linux it's the real flush for `copy_file_range` dests. Cross-FS move flushes the FINAL paths (Phase 3 renames staging → destination, so the staging entries in `created_files` are remapped to their final prefix before the pass — this also covers the Phase-3 `throwaway_tx` renames that aren't in the real transaction). Same-FS move (pure rename) writes no data, so its flush just `fdatasync`s the moved files (cheap) and their parent dirs to make the new directory entries durable. The flush is best-effort on error: a failed `sync_data` is logged (`target: "write_durability"`), not propagated — the bytes are written either way and failing the whole op at the final flush is worse UX. Pinned by `transfer/copy_tests.rs::local_copy_emits_flushing_phase_before_complete` and `transfer/move_op_tests.rs::cross_fs_local_move_emits_flushing_phase_before_complete`; FE label by `TransferProgressDialog.flushing.test.ts`. **Cross-volume copy/move landing on a local disk** (MTP → Local, SMB → Local, USB import) doesn't go through this local-FS engine — it flows through `LocalPosixVolume::write_from_stream`, which keeps the same promise by `sync_data`-ing each file (plus a best-effort parent-dir fsync for the directory entry) before it returns, so each file is durable as it completes. That path doesn't yet emit the `Flushing` UI phase (the volume copy/move handlers don't call `flush_created_destinations`); a follow-up could route them through the end-of-op pass for UI consistency, but the per-file `sync_data` already makes them durable.

**Decision**: `state.rs` re-exports the `operation_intent` + `scan_cache` + `status_cache` names and `types.rs` re-exports the `event_sinks` types + `error_classification::IoResultExt`, so `state::…` / `types::…` paths resolve for callers. These re-export facades are kept deliberately, not collapsed into direct `scan_cache::…` / `error_classification::…` imports.
**Why**: Every one of the four re-exported groups has a broad consumer surface once grouped `use` blocks are counted: `operation_intent` at ~35 sites across ~20 files (every cancellation check), `event_sinks` at ~11 sites (every progress emit), `IoResultExt` across seven copy/scan/delete backends, and the `scan_cache` types across `scan.rs`, `scan_preview.rs`, `validation.rs`, and two test files. Collapsing any of them is a touch-many-files churn (~12 files for the two smaller ones alone) with no behavior or clarity payoff — a facade fronting a high-traffic name surface is a legitimate shape here, so leave them. If a future split genuinely narrows one group's consumers, revisit then.

## Shared gotchas

**Gotcha**: On macOS, never use `statvfs` alone for disk space checks; use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks. On APFS, purgeable space (iCloud caches, APFS snapshots) can account for tens of GB that macOS will reclaim on demand. Using `statvfs` causes the "insufficient space" error to reject copies that would actually succeed, and shows a different available-space number than the status bar (which uses the NSURL API). `validate_disk_space` in `validation.rs` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

**Gotcha**: Volume-side `on_progress` callbacks report counts LOCAL to the current scan operation, not cumulative.
**Why**: `Volume::scan_for_copy_batch_with_progress` and `scan_subtree_with_oracle` both invoke `on_progress(count)` with a count local to the current `list_directory` call / subtree (starts at 1 each time). Forwarding that unchanged through `run_volume_scan_preview`'s closure made the FE's running tally drop visibly between parent groups, between sibling top-level dirs in a cache-hit branch, and between recursion frames inside `scan_subtree_with_oracle`. `run_oracle_aware_batch_scan` now wraps `on_progress` with a `baseline = aggregate.file_count` shift before each scan call (cold-cache batch + cache-hit subtree), and `scan_subtree_with_oracle` does the same at its own recursion site (`baseline = totals.file_count`). The visible FE count stays cumulative across the entire scan. Direct `on_progress(aggregate.file_count)` emit sites in `run_oracle_aware_batch_scan` (cache-hit per-file paths, fallthrough `scan_for_copy` after a name miss) stay unwrapped — they're already cumulative. Future scan call sites that delegate to a volume backend or to `scan_subtree_with_oracle` need the same baseline wrap.

**Gotcha**: Copy's bar/space-check use the write footprint (`total_bytes`), not the dedup'd source size — by design.
**Why**: A copy of a hardlink-heavy tree writes every link in full, so the bar fills against the write footprint and the disk-space check reserves it (the headline can legitimately read "80 GB" for a 60 GB-`du` `target/`). This is correct, not a bug — `scan_subtree_with_oracle` and `copy_volumes_with_progress` both carry the un-dedup'd `total_bytes` for copy, while the dedup'd `dedup_bytes` rides alongside purely to drive the dialog's clarifying note. Don't "fix" copy to show the dedup'd number: that would under-reserve disk space and stall the bar on dupes. The cross-volume copy path (`copy_volumes_with_progress` → `volume::strategy::copy_directory_streaming`) credits raw streamed bytes per file, which already equals the write footprint, so no dedup wiring is needed there. The one residual approximation: `dedup_bytes` over the cross-source-hardlink case (a file hardlinked into two separately-selected sources) counts twice, slightly understating the dedup savings shown in the note — safe direction, documented on `CopyScanResult::dedup_bytes`.

**Gotcha**: Volume disconnect mid-walk races with the oracle.
**Why**: The oracle returns `Some(entries)` when `listing_watch_coverage` reports `EveryWriter` at the moment of the check. Between that read and the recursive walk consuming the entries (and then issuing real `list_directory` calls for any sub-subfolders that aren't cached), the watcher can die (cable yanked, network drop). The synthesized totals for the cached level are correct — they reflect what the listing held — but recursion into now-disconnected sub-subfolders fails per-call, and the per-file copy/delete later then hits `DeviceDisconnected`-shaped errors instead of a single "device gone" message at the scan level. Same race that `scan_for_copy_batch` already had; the oracle doesn't widen it. Documented here so future investigation knows where to look.

## Dependencies

- `crate::file_system::volume`: `Volume` trait, `SpaceInfo`, `ScanConflict` (used by `transfer/volume/copy.rs`)
- `crate::ignore_poison`: `IgnorePoison` extension for `RwLock`/`Mutex` to not panic on poisoned locks
- External: `tauri` (emit, AppHandle), `uuid` (operation IDs, temp names), `libc` (access, statvfs), `xattr`, `exacl`, `filetime` (metadata preservation in `transfer/chunked_copy.rs`)

## Testing bar

This module's state machine (`state.rs`) is the spine of the cancel UX. Past investigations found one real production bug here ([commit `1de4255d`](../../../../../../docs/notes/speed-up-e2e-tests.md), lost-rollback on `Ok(())` arm) plus 30+ mutation-testing gaps that have since been pinned. New transitions or new cancel paths must:

1. **Drive the state machine through the public interface in tests.** Direct `state.intent.store(...)` mutation bypasses the validation guard and effectively dead-tests it. Pattern to copy: `state.rs::tests::test_cancel_via_public_path`.
2. **Cover both the happy path and the cancel-during-X race** for any new write-side operation. The Cancel-copy bug was specifically the `Ok(())` arm of the loop not re-checking intent.
3. **Add at least one E2E test** for user-visible flows (transfer dialogs, conflict policies); use `dispatchMenuCommand` for keyboard-shortcut triggers, see `docs/testing.md` § "❌ Synthesized F-key dispatches".
4. **Run `cargo mutants --file src/file_system/write_operations/<file>.rs`** after substantial changes; this module has ~85-90% mutation score per file and shouldn't regress. See `docs/testing.md` § "Process".

### Test isolation for `WRITE_OPERATION_STATE`

`cargo test` runs the crate's tests as threads in ONE process, so the `WRITE_OPERATION_STATE` map is shared by every
write-op test at once. `test_support::TestOperationGuard` owns one entry per test:

- **Unique key.** `register(tag)` / `register_state(tag, state)` mint a process-unique op id (tag + pid + counter), so a
  hardcoded literal can't collide with a sibling test. `register_as(op_id, state)` adopts an id the suite already
  generated (`transfer_driver`'s `unique_op_id`), for tests that thread the id through the call under test.
- **Panic-safe teardown.** `Drop` removes the entry, so an assertion that fails before a hand-rolled `remove` can't
  leave a corpse for the next test's `cancel_all_write_operations` to walk or `list_active_operations` to count. Pinned
  by `state::tests::guard_unregisters_its_state_even_when_the_test_body_panics`. Keep the guard on the stack: a
  `std::mem::forget` or a clone that outlives the test defeats it.

Same shape as `listing::caching_test_support::TestListingGuard` (over `LISTING_CACHE`) and
`indexing::tests::stress_test_helpers::TestInstanceGuard` (over `INDEX_REGISTRY`).

The operation-log journal slot is the exception to the unique-key pattern: it's a single value, not a keyed map, so
tests that install a journal SERIALIZE on `operation_log::TestJournalGuard` (one guard per test, lock held for the
test's duration, slot cleared on drop — see its doc for the multi-arm `hold_empty`/`set` shape and the non-reentrancy
deadlock warning). Never call `set_journal` directly from a test. Residual under plain `cargo test`: non-journal
write-op tests still journal their own ops into whatever DB is installed, so journal assertions stay scoped to the
test's own `op_id` (`journal_capture_tests::dir_volume_ids` joins dirs through the op's item rows for this reason).

#### ❌ Never drive a walk-everything mutator from a test

A unique key isolates a test that touches ONE entry. It does nothing for a function that walks the WHOLE registry:
`cancel_all_write_operations` stops every registered op, so a test calling it cancels whatever operations the tests
running beside it have in flight. That failure lands in the victim, not the culprit — a managed-op test seeing
`write-cancelled` (or a bare 0 events) where it expects `write-complete`, membership shifting run to run with
co-scheduling. It reads exactly like environment flake, which is how it survived: mis-blamed on load starvation once
and on a dependency upgrade once.

**The fix is a registry the test owns, not a lock around the tests.** `WriteOperationRegistry` (in `state.rs`) holds the
map and owns `cancel_all`; `WRITE_OPERATION_STATE` is one instance of it and `cancel_all_write_operations()` is a
one-line delegation. A test constructs its own `WriteOperationRegistry`, registers states in it, and calls
`cancel_all()` — the same production code, on a registry nothing else can see. Serializing the two suites behind a
mutex, `#[ignore]`ing the victim, or loosening its assertion would all have hidden the defect instead.

`state::tests::cancel_all_write_operations_walks_the_global_registry` is the ONE exception, because its subject IS the
global wiring. It's gated on `test_support::one_test_per_process()` (nextest's `NEXTEST_EXECUTION_MODE`), so it runs in
the sanctioned lane and skips under plain `cargo test`, where it would be the poisoner. Don't copy the gate to reach for
a global out of convenience; copy the private-registry pattern.

Lower-severity siblings, both currently harmless: `manager::force_admission_pass()` (test-only) runs a process-wide
admission pass, safe only because every manager test mints a unique `LaneKey`; and `watcher_test.rs`'s
`handle_directory_change` tests insert into `LISTING_CACHE` by hand with a tail `remove` instead of `TestListing`, so a
failing assertion leaks the entry (unique uuid keys keep it from poisoning anyone).

**A manager counter is never scoped by a unique op id.** `emit_count()` and `admission_pass_count()` count a WHOLE
manager's activity, so a sibling test spawning or settling ANY op moves them; a unique id, which is what keeps the
record-and-lane assertions honest, buys nothing here. A test asserting on either drives its own manager instead of
`manager()`: `private_manager()` for the lock-only paths, and `failures::leaked_manager()` (a `Box::leak`) when the test
has to `spawn_managed`, which takes `&'static self` because an admitted op's task outlives every borrow. Pair it with
`tests::gated_deferred_on`, so the synthetic op settles on the same private manager. The leak is one small struct per
test that asks for one. An equality assertion against the global counter passes only because nextest forks per test;
under plain `cargo test` it fails outright (`retained_failure_stays_hidden_until_the_record_settles` did).
The two `admission_pass_count()` waits in `tests.rs` stay on the global one: they wait for GROWTH, not an equality, so a
sibling's pass can only satisfy them early, never fail them.

**New subsystem state hangs off a struct, not a `static`.** These guards are the retrofit cost of a process-global; a
handle threaded through its callers needs none of it.

See also: `docs/testing.md` for the project-wide testing playbook.
