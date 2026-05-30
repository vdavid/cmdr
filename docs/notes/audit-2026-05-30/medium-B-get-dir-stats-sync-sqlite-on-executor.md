# `get_dir_stats` / `get_dir_stats_batch` run synchronous SQLite reads on the async executor

**Severity:** medium
**Lens:** B — Concurrency / main-thread responsiveness
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/commands/indexing.rs:34` and `:40`, delegating to `apps/desktop/src-tauri/src/indexing/state.rs:432` / `:460`.

## What
Both are `#[tauri::command] async fn` that call `indexing::get_dir_stats(&path)` / `get_dir_stats_batch(&paths)` directly. Those bodies do `pool.with_conn(|conn| { ...SQLite queries... })` — synchronous `rusqlite` reads — with no `spawn_blocking` and no `blocking_with_timeout`. This deviates from the documented contract that every DB/FS-touching command wraps its blocking work (commands/CLAUDE.md § "blocking_with_timeout for all filesystem-touching commands"; architecture.md § Platform constraints #1).

## Why it matters
Synchronous work on a `#[tauri::command] async fn` runs on the tokio executor / IPC thread; if it stalls, subsequent IPC calls queue behind it and the UI appears frozen — the exact failure mode principle #3 ("the UI must always be responsive") forbids. SQLite reads are WAL-mode so they rarely block on the single writer, but `apply_pragmas` sets `busy_timeout = 5000` (indexing/CLAUDE.md), so a read that hits contention (mid-`WalCheckpoint(TRUNCATE)`, or a large writer transaction during a big index build) can park a tokio worker for up to ~5 s. `get_dir_stats_batch` is on the directory-listing stats hot path, so it's the most-exercised of the two.

## Evidence
```rust
// commands/indexing.rs:34-42
#[tauri::command]
pub async fn get_dir_stats_batch(paths: Vec<String>) -> Result<Vec<Option<DirStats>>, String> {
    indexing::get_dir_stats_batch(&paths)   // sync SQLite reads; no spawn_blocking / timeout
}
```

## Suggested fix
Wrap the delegation in `spawn_blocking` + `tokio::time::timeout`, matching the other DB/FS commands. If the WAL-read latency is judged always-bounded (the index DB is local, never a network mount), document an explicit carve-out in commands/CLAUDE.md so the deviation from the "wrap everything" rule is intentional and visible rather than an oversight.

## Notes
Lower-risk than the network-mount commands precisely because it's a local DB — it can't hang for 30–120 s like a dead SMB mount. The gap is that it's the one DB-touching command family that skips the wrapper the module's own docs mandate. The concurrency posture elsewhere (lock-free read pool, clone-Arc-then-release, tokio::Mutex-across-await, bounded buffers, JoinHandle teardown) was verified sound.
