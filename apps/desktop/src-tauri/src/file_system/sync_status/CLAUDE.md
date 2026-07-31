# Sync status (cloud badges)

The per-row cloud badge (Dropbox, iCloud Drive, Google Drive, …) on macOS. `probe.rs` asks macOS about one file;
`pool.rs`, `cache.rs`, and `service.rs` exist only to keep that question from costing threads, CPU, or a frozen pane.

Design rationale, the incident behind it, and the tuning numbers: `DETAILS.md`.

## Must-knows

- **The probe ends in a synchronous XPC call that can block forever.** `NSURL getResourceValue` reaches
  `fileproviderd` and the provider's `.appex`; there is no timeout on it and no way to cancel it once entered. Every
  design choice here follows from that.
- **The probe runs on `pool.rs` and nowhere else.** Never rayon (2 MB stacks blow up on provider override chains, see
  `file_system/CLAUDE.md`), never tokio's blocking pool (`spawn_blocking` work can't be cancelled, and the runtime needs
  those threads). The pool is hard-capped at `max_workers` threads for the process lifetime, including ones lost inside
  a provider that stopped answering.
- **A deadline bounds the caller's wait, never the work.** `statuses_within` returning `timed_out` means "ask again",
  not "that work is gone": the batch keeps running and caches what it learns. ❌ Don't wrap these entry points in
  `blocking_with_timeout_flag` — that throws away both the partial answer and the running batch.
- **Exactly one batch is in flight.** A second ask joins it (same paths) or supersedes and cancels it (different
  paths). ❌ Don't add a path that starts a second fan-out; that's the shape that put 21-23 threads in the incident's
  `sample`.
- **Cache invalidation is a caller's job.** Anything that changes a file's cloud state without a reliable FSEvent must
  call `invalidate_path`; `listing::caching::notify_directory_changed` already calls `invalidate_dir` for the general
  case. A missed call shows a stale badge for up to the stable TTL.
- **`bench.rs` is the only honest way to re-tune the pool.** It needs a real File Provider folder
  (`CMDR_SYNC_STATUS_BENCH_DIR`); XPC latency can't be faked. Numbers and method:
  `docs/notes/sync-status-pool-bench-2026-07-31.md`.
