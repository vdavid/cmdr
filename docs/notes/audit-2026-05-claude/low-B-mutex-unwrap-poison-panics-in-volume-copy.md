# Volume-copy uses raw `.lock().unwrap()` for 19 mutexes; bypasses `IgnorePoison`

**Severity:** low **Lens:** B — Concurrency **Confidence:** high

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_copy.rs` — 19 occurrences (see
`grep -n '.lock().unwrap()' volume_copy.rs`) Related: same pattern at `volume_move.rs` (9 occurrences) and
`mcp/pane_state.rs` (9 occurrences).

## What

The codebase has an `IgnorePoison` extension trait at `apps/desktop/src-tauri/src/ignore_poison.rs` precisely for
"simple value stores where a panic in another thread doesn't invalidate the data" — the documented convention.
`volume_copy.rs` ignores it and calls `.lock().unwrap()` directly on `in_flight_partials`, `last_progress_mutex`,
`last_emit`, `apply_to_all`, `copied_paths`, `failure_ctx_cell`, `last_dest_cell`, etc. Each is a panic point if its
mutex ever gets poisoned.

## Why it matters

- These mutexes guard per-operation state inside the concurrent copy path (`FuturesUnordered` over N parallel streams).
  If ANY of the spawned tasks panics while holding a lock, every remaining `.lock().unwrap()` against that same mutex
  panics, which cascades into the parent `copy_between_volumes` task, which bubbles up as `JoinError`, which
  `start_write_operation`'s safety-net path turns into a `write-error` event for the operation — losing the
  cancel/rollback flow.
- More subtly: panics in volume backends are observable. A panic in an MTP background poll or an SMB watcher (both have
  a history of bug-driven panics) can poison a per-op mutex if the panicking thread happens to be one of the parallel
  copy futures. The poisoning is silent until the next lock attempt.

The `IgnorePoison` trait's docstring is explicit that these data shapes (`Vec<PathBuf>`, `Instant`, `Option<...>`,
`Option<ConflictResolution>`) are exactly the cases where the wrapped value is fine to use even after a poison — losing
the partial state isn't worse than crashing the whole copy.

## Evidence

```rust
// volume_copy.rs
in_flight_partials.lock().unwrap().push(dest_item_path.clone());     // line 700
copied_paths.lock().unwrap().push(completed_dest);                   // line 800
*last_dest_cell.lock().unwrap() = Some(dest_item_path.clone());      // line 1060
*failure_ctx_cell.lock().unwrap() = Some((e, source_path));          // line 1082
let mut latched = apply_to_all.lock().unwrap().take();               // line 940
*last_prog_a.lock().unwrap() = Instant::now();                       // line 732
```

The `ignore_poison.rs` trait header says:

> All 75 uses of `.lock().unwrap_or_else(|e| e.into_inner())` in the codebase store simple values where poison is
> irrelevant. This trait replaces the boilerplate with a readable `.lock_ignore_poison()` call.

The volume_copy mutexes match that description exactly — simple value stores, no shared invariants — yet they don't use
the trait.

## Suggested fix

Mechanical: import `crate::ignore_poison::IgnorePoison` and replace every `.lock().unwrap()` with
`.lock_ignore_poison()`. The trait is `use`-only; no other change required. Verify by running
`cargo nextest run --lib write_operations::transfer::volume_copy_tests` (existing tests pin the data-safety contract).

Same swap applies to `volume_move.rs` and `mcp/pane_state.rs` for consistency, though those are lower-traffic paths.

## Notes

This isn't a correctness bug today (no known panic source in the copy hot path), but every panic in the volume-copy
graph becomes a poisoned mutex, and every poisoned mutex turns into a second panic on the next lock. With the
panic-hook + crash-reporter pipeline live, the second panic isn't a silent failure — it just amplifies the noise. The
trait already exists; using it is free.
