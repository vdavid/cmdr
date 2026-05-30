# `INDEXING` global mutex held across a blocking up-to-5 s shutdown drain

**Severity:** medium
**Lens:** B — Concurrency
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/indexing/state.rs:130-145` (`stop_indexing`) and the parallel `clear_index` (`:283-298`), with the blocking drain in `indexing/manager.rs::shutdown` (`block_in_place` + `block_on(timeout(5s, ...))`).

## What
`stop_indexing()` and `clear_index()` acquire the process-wide `INDEXING` `std::sync::Mutex`, then call `mgr.shutdown()` while still holding the guard. `shutdown()` runs `tokio::task::block_in_place(|| block_on(timeout(Duration::from_secs(5), live_event_task)))`. So the lifecycle mutex is held across a synchronous block of up to 5 seconds. Both functions are invoked directly from the body of `async` Tauri commands (`set_indexing_enabled`, `clear_drive_index` in `commands/indexing.rs`).

## Why it matters
Per `indexing/CLAUDE.md`, the `INDEXING` mutex "now guards only lifecycle transitions (start, stop, clear, **status**)" — reads moved to `ReadPool` precisely to avoid contending on it. While the drain runs, every other caller that locks `INDEXING` blocks: `get_status()`, `get_debug_status()`, `is_active()`, and the per-navigation `trigger_verification()` (which acquires `INDEXING` in its spawned task). So toggling indexing off, or clearing the index, on a busy device freezes the index-status IPC surface and stalls navigation-time verification kicks for up to 5 seconds. It also parks a tokio worker for the whole window (`block_in_place` keeps the worker; `block_on` drives the future on it).

## Evidence
```rust
// state.rs:130 (stop_indexing) — guard taken, then held across the drain
let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;
match std::mem::replace(&mut *guard, IndexPhase::ShuttingDown) {
    IndexPhase::Running(mut mgr) => {
        mgr.shutdown();              // <- block_in_place + block_on up to 5 s, guard STILL held
        *guard = IndexPhase::Disabled;
```
```rust
// manager.rs (shutdown)
let task = self.live_event_task.lock().unwrap().take();
if let Some(task) = task {
    tokio::task::block_in_place(|| {
        tauri::async_runtime::block_on(async {
            match tokio::time::timeout(Duration::from_secs(5), task).await { ... }
```

## Suggested fix
The mutex only needs to protect the atomic phase swap, not the drain. Line 132 already swaps the phase out to `IndexPhase::ShuttingDown` and binds the owned `IndexPhase::Running(mut mgr)`. Move the owned `mgr` out, `drop(guard)`, then call `mgr.shutdown()` on it. The live event loop uses `ReadPool` and doesn't reacquire `INDEXING`, so dropping the guard before the drain is safe and lets concurrent `get_status` / verifier calls proceed against the already-published `ShuttingDown` phase. This also removes the worker-park-while-locked window. Apply the same change to `clear_index`.

## Notes
- Triggered by user actions (Settings/Debug: disable indexing, clear index), so not hot-path frequent — but it directly violates the module's own "reads never contend on `INDEXING`" contract for the duration, and the UI's index-status surface is exactly what a user watches right after toggling indexing.
- Navigation itself isn't frozen (verification is fire-and-forget on a spawned task), but the verification work and any status poll stall.
