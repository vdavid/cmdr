# `download_update` verifies the signature and writes the full tarball synchronously on the executor

**Severity:** low
**Lens:** B — Concurrency / main-thread responsiveness
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/updater/mod.rs:137`, `:141`, `:144`.

## What
Inside `pub async fn download_update`, after the async download completes, the command runs `signature::verify(&bytes, ...)` (CPU-bound minisign verification over the whole multi-MB blob), then `std::fs::create_dir_all` and `std::fs::write(&tarball_path, &bytes)` (synchronous write of the entire tarball) — all inline on the tokio executor, no `spawn_blocking`.

## Why it matters
Blocks a tokio worker for the duration of a multi-MB hash + disk write. It's a local temp dir, so it won't hang like a network mount, and the updater runs rarely, which is why this is low. But it's a clear instance of CPU + blocking-fs work on the async runtime, against principle #3 ("never block the main thread").

## Evidence
```rust
// updater/mod.rs:137-144
signature::verify(&bytes, &signature)?;                    // CPU-bound, on executor
let temp_dir = std::env::temp_dir().join("cmdr-update");
std::fs::create_dir_all(&temp_dir)...?;
std::fs::write(&tarball_path, &bytes)...?;                 // sync write of whole blob
```

## Suggested fix
Move the verify + write into `tokio::task::spawn_blocking` (or use `tokio::fs::write` for the write), then store the resulting path in `state`.

## Notes
The signature verification itself is correct and pinned (minisign against a hardcoded `PUBKEY_BASE64`, verified before the tarball is consumed by `install_update`) — see the F-lens notes. This finding is purely about doing that work off the executor.
