# Unwrapped synchronous `std::fs` in `create_directory` / `create_file` fallback paths

**Severity:** low
**Lens:** B — Concurrency
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/commands/file_system/write_ops.rs:103-115` (`create_directory_core` fallback) and `:171-183` (`create_file_core` fallback)

## What
The happy path of `create_directory_core` / `create_file_core` routes through `volume.create_directory` / `create_file` wrapped in a 5 s `tokio::time::timeout`. The "unknown volume" fallback instead calls `std::fs::create_dir` / `std::fs::File::create_new` synchronously, directly in the `async fn` body, with no `spawn_blocking` and no timeout.

## Why it matters
If reached on a hung/slow mount, the blocking syscall runs on a tokio worker with no timeout, contradicting the module's own contract (`commands/CLAUDE.md`: "`blocking_with_timeout` for all filesystem-touching commands … even a simple `path.exists()` can block indefinitely"). In practice "root" and every mounted volume is always registered in `VolumeManager`, so the fallback is effectively dead code — hence low severity — but it's a latent un-timed FS call on the executor that an unusual code path or future refactor could make live.

## Evidence
```rust
// write_ops.rs:103
// Fallback for unknown volumes (shouldn't happen in practice)
let mut new_path = PathBuf::from(&expanded_path);
new_path.push(name);
crate::downloads::note_pending_write_for_cmdr(&new_path);
std::fs::create_dir(&new_path)        // sync, on the async executor, no timeout
    .map_err(|e| match e.kind() { ... })?;
```
```rust
// write_ops.rs:175 (create_file_core)
std::fs::File::create_new(&new_path)  // sync, on the async executor, no timeout
    .map_err(|e| match e.kind() { ... })?;
```

## Suggested fix
Either delete the fallback (the comment already says "shouldn't happen") and return a typed "volume not found" `IpcError`, or wrap it in the same `tokio::time::timeout(Duration::from_secs(5), spawn_blocking(...))` shell the volume path uses. Deleting is cleaner and removes the latent contract violation entirely, since the `VolumeManager` always has "root".

## Notes
- `commands/CLAUDE.md` lists `create_directory` / `rename_file` at the 5 s write tier; this fallback is the one branch that escapes that tier.
