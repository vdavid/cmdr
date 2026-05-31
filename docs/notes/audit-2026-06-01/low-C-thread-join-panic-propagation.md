# Worker-thread panic re-propagates and crashes the calling command

**Severity:** low **Lens:** C — Error handling **Confidence:** medium

## Location

`file_system/sync_status.rs:144,150`, `icons/mod.rs:513,519`

## What

`get_sync_statuses` and the macOS icon path-fetch both fan out across scoped worker threads, then re-join with
`handle.join().expect("... thread panicked")`. If any worker panics, `join()` returns `Err(Box<dyn Any>)` and the
`.expect()` re-panics on the calling thread. The spawn side also `.expect("failed to spawn ... thread")`, which panics
if the OS refuses a new thread (resource exhaustion).

## Why it matters

These run from Tauri commands (sync-status badges, icon fetch) over user-supplied path lists. The worker bodies
(`get_sync_status`, `fetch_icon_for_path`) are currently panic-free — they swallow their own IO errors and return
`Unknown`/`None` — so the join panic only fires if an ObjC/`NSWorkspace` call or an `std::fs` edge panics unexpectedly.
If it does, the whole command thread unwinds and the panic hook fires a crash report, when the graceful outcome would be
"this batch returns `Unknown` icons/statuses and the UI carries on." Blast radius: a panic mid-batch turns a cosmetic
enrichment failure into an app crash.

## Evidence

```rust
// file_system/sync_status.rs
.spawn_scoped(scope, move || { /* get_sync_status per path */ })
.expect("failed to spawn sync-status thread")
...
for handle in handles {
    result.extend(handle.join().expect("sync-status thread panicked"));
}
```

```rust
// icons/mod.rs
.expect("failed to spawn icon path-fetch thread")
...
results.extend(handle.join().expect("icon path-fetch thread panicked"));
```

## Suggested fix

Treat a panicked or unspawnable worker as a degraded-but-recoverable batch rather than a fatal: on `join()` returning
`Err`, log a warning and skip that chunk's results (the affected paths simply get no badge / fall back to a generic
icon), instead of `.expect()`. Same for the spawn `.expect()` — fall back to running the chunk inline on the current
thread. This is cosmetic metadata; it should never be able to take the app down.

## Notes

Borderline: re-propagating a worker panic is a defensible "fail loud" choice, and the worker bodies don't currently
panic. Filed at **low** because the cost of hardening is small and the payoff (a cosmetic feature can't crash the app)
aligns with the "rock solid / assume the hostile case" principle. Not a correctness bug today.
