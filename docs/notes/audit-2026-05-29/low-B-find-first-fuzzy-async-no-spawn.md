# `find_first_fuzzy_match` is `async` but does pure-CPU work on the runtime worker

**Severity:** low
**Lens:** B — Concurrency
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/commands/file_system/listing.rs:275-283`

## What
The command is declared `async` and has no `.await`, no `spawn_blocking`, and no timeout. It runs `nucleo-matcher` smart-case fuzzy scoring across the entire cached listing on whatever async worker the IPC handler lands on:

```rust
#[tauri::command]
pub async fn find_first_fuzzy_match(
    listing_id: String,
    query: String,
    include_hidden: bool,
) -> Result<Option<usize>, IpcError> {
    ops_fuzzy_find_first_match_in_listing(&listing_id, &query, include_hidden).map_err(IpcError::from_err)
}
```

## Why it matters
For a 100k-entry cached listing (typical iCloud photo library), `nucleo-matcher` scoring all entries to find the best match takes single-digit-to-tens of ms. On modern hardware this is fine in absolute terms, but:

1. Every keystroke during type-to-jump fires the command. A user holding down a key during fast-typing produces 10+ calls/sec.
2. The handler runs on a runtime worker without `spawn_blocking` — each scoring pass parks one worker for its duration. With a small pool, concurrent IPC throughput drops while the worker is busy.
3. The command also takes a `LISTING_CACHE.read()` lock inside `ops_fuzzy_find_first_match_in_listing`. If another thread is trying to `write()` the cache (a watcher diff landing mid-keystroke), the writer is parked too.

This isn't the most painful issue in the audit but it's a pattern that compounds.

## Evidence
- `commands/file_system/listing.rs:275-283`: the handler body.
- `file_system/listing/fuzzy_jump.rs::find_first_match`: pure CPU pass over `entries`.
- Other read-only commands that operate on the cache (`get_file_range`, `get_total_count`, `get_files_at_indices`) are correctly declared `pub fn` (sync), not `async`. Those are O(visible-window) rather than O(N), so it's the right shape.

## Suggested fix
Either:
- Drop the `async` keyword and make it a sync command (Tauri serializes sync commands across IPC anyway; doesn't park a worker). The cost is moving the lookup off the tokio scheduler, which is the right answer for a pure-CPU computation under 50 ms.
- Or, wrap in `tokio::task::spawn_blocking` if you want it to genuinely run on the blocking pool.

The first is the cheapest change and matches the surrounding sync handlers.

## Notes
Catching this also implicitly enforces "if you're `async`, you should be awaiting something" as a review heuristic.
