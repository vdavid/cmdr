# Delete + trash

Delete and trash operations: a local-FS walker, a volume-aware walker (MTP, SMB), and OS-native trash. The local walker
uses `walkdir` + `fs::remove_file`; the volume walker uses the `Volume` trait and is oracle-aware.

See `../CLAUDE.md` for the shared `WriteOperationState`, `OperationIntent` state machine, cancel
contract, ETA estimator, and settle contract. `../transfer/CLAUDE.md` is the parallel doc for
copy + move. Frontend counterpart:
`apps/desktop/src/lib/file-operations/delete/CLAUDE.md`.

## Files

- **`walker.rs`**: local delete (`delete_files_with_progress_inner`) and volume delete
  (`delete_volume_files_with_progress_inner`), both taking `&dyn OperationEventSink`; `delete_files_start` routes by
  `volume_id`. The volume walker (`scan_volume_recursive`) consults `try_get_authoritative_listing` before every
  `list_directory`, so a subtree open in another pane is cache-fed. DETAILS § "Volume-delete internals".
- **`trash.rs`**: `move_to_trash_sync()` (macOS ObjC `trashItemAtURL`; Linux `trash` crate; reused by
  `commands/rename.rs`) and `trash_files_with_progress()` (batch trash with per-item progress, cancellation, partial
  failure). Uses `symlink_metadata()` for existence checks (handles dangling symlinks).
- Test siblings: `delete_integration_test.rs`, `delete_volume_reuse_tests.rs` (preview reuse, oracle fast path, and the
  missing-fact branch audit), `preview_binding_tests.rs` (the cache binding), `volume_cancel_tests.rs`.

## Must-knows

- **Delete order is files first, then directories deepest-first.** The walker collects entries in DFS order and deletes
  in reverse so directories are empty by the time `remove_dir` runs.
- **Delete is not rollbackable.** Once deleted, data is gone (unless trashed). Cancellation stops further deletes but
  won't restore the already-deleted ones.
- **MTP/non-local volumes can't use `walkdir` or `fs::remove_*`** (hence the parallel volume-aware path). Both paths emit
  identical events so the frontend progress dialog works unchanged.
- **Both delete paths reuse the scan-preview cache via `config.preview_id`.** On a cache hit the `ScanResult` is consumed
  directly (no re-scan), and an initial `phase: Deleting` event fires so the FE switches to the active-phase UI with the
  right denominator. Without this, a second BE-side scan starting from `filesDone=0` makes the count visibly reset. The
  volume path is also oracle-aware on the no-preview path; see DETAILS.md.
- **A `preview_id` alone doesn't authorize acting on a path set**, and the LOCAL walker is why: it iterates
  `scan_result.files` and never re-reads its own `sources`, so an unbound cache deletes the previewed tree instead of the
  requested one, with no rollback. `take_cached_scan_result` does the binding (`../CLAUDE.md`); ❌ don't add a consumer
  that skips it.
- **❌ Never resolve a top-level source's type with `.unwrap_or(false)`.** Hand the `Option` to
  `scan_volume_recursive`, which propagates a failed probe. A guessed "file" books the entry as `is_dir: false` with zero
  bytes, so progress lies about a whole tree and the `delete` that follows acts on a fact nobody established. DETAILS §
  "What each branch does with a missing or wrong fact".
- **Trash has no scan phase.** `trashItemAtURL` is atomic per top-level item (the OS moves the whole tree), so progress
  tracks top-level items (optional byte progress from pre-computed sizes). Partial failure is supported.
- **Delete and trash don't `fsync` or fire any global `sync(2)`.** A non-durable delete is annoyance-class, not
  data-loss-class. Don't reintroduce a `sync(2)` here: it flushed every filesystem on the box, stalling unrelated apps,
  and as fire-and-forget didn't make "complete" mean "durable". Pinned by
  `tests.rs::no_global_sync_or_spawn_async_sync_in_write_operations`.
- **Recursive scan helpers that bail with `Err(Cancelled)` must NOT emit `write-cancelled` themselves; the top-level
  caller must.** `scan_volume_recursive` checks cancel at every recursion level; emitting at the bail site would fire
  the terminal event once per stacked frame. So it returns `Err(Cancelled)` silently and the caller emits via
  `emit_cancelled_if_aborted` before propagating. Any new recursive scan with a per-level cancel check needs the same
  caller-side emit, else the FE never sees `write-cancelled` and the dialog closes via the settle-fallback path instead
  of the proper cancel flow. Pinned by `delete_cancel_during_scan_emits_write_cancelled`.

Full details (volume-delete scan-preview reuse and its three parts + data-safety contract, the no-`fsync` decision
rationale): `DETAILS.md`.
