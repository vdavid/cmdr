# Status-cache entries leak on a panic inside the async volume-delete branch

**Severity:** low **Lens:** G — Resource hygiene **Confidence:** high

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/mod.rs:318-345` (volume-delete branch of `delete_files_start`)

## What

The main `start_write_operation` path runs its handler in `spawn_blocking` and does cache cleanup _after_ a `match` that
includes the `Err(join_error)` (panic) arm, so cleanup always runs. The volume-aware delete branch instead runs
`delete_volume_files_with_progress(...).await` directly on the async task, then removes the operation from
`WRITE_OPERATION_STATE` and calls `unregister_operation_status` on the lines _after_ the await. If that async call
panics, the task unwinds past those cleanup lines and the `OPERATION_STATUS_CACHE` + `WRITE_OPERATION_STATE` entries for
that `operation_id` are never removed. (The `WriteSettledGuard` Drop still fires, so the FE isn't wedged — only the two
map entries leak.)

## Why it matters

Each leaked entry is a small `OperationStatusInternal` plus an `Arc<WriteOperationState>`, keyed by a fresh UUID per
operation. It only leaks on an actual panic inside a volume (MTP/SMB) delete, which is rare, so this is a slow drip
rather than OOM — but `list_active_operations` would also keep reporting the dead op forever, and over a long session of
flaky-device deletes the map grows unbounded.

## Evidence

```rust
// mod.rs:318
let result = delete_volume_files_with_progress(
    volume, &volume_id_str, &app, &operation_id_for_spawn, &state, &sources, &config,
).await;                                   // <-- a panic here unwinds the task
// ...match result { ... } ...
if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
    cache.remove(&operation_id_for_cleanup);   // line 342: skipped on panic
}
unregister_operation_status(&operation_id_for_cleanup);  // line 344: skipped on panic
```

## Suggested fix

Mirror the panic-safe pattern the main path already uses. Move the two cleanup calls into the `Drop` of a small RAII
guard constructed alongside `_settled_guard` at the top of the spawned task (or fold them into `WriteSettledGuard`
itself, since it already runs last in scope on unwind). Then a panic in `delete_volume_files_with_progress` still frees
both map entries during stack unwinding, matching the contract documented in `write_operations/CLAUDE.md` ("state
removed from both caches").

## Notes

- `write_operations/CLAUDE.md` documents the `WRITE_OPERATION_STATE` / `OPERATION_STATUS_CACHE` cleanup and the
  `WriteSettledGuard` panic-safety pattern; this one async branch is the spot that doesn't yet route its map cleanup
  through a guard. Related to the verifier `in_flight` leak (same RAII-on-panic class).
