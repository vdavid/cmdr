# File system module

Core filesystem operations: directory listing, file writing, sync status, volume management, and file watching.

Submodule docs: [listing/](listing/CLAUDE.md), [write_operations/](write_operations/CLAUDE.md), [volume/](volume/CLAUDE.md).

## Gotchas

**Never use rayon (or any constrained-stack thread pool) for calls into macOS frameworks.**
NSURL resource-value lookups, FileProvider queries, and similar Objective-C APIs make synchronous XPC round-trips to
system daemons. These can consume deep stack frames through FileProvider override chains (iCloud, Dropbox, etc.),
exceeding rayon's default 2 MB worker stack. Use dedicated OS threads with an explicit stack size (8 MB) instead. This
also prevents I/O-bound XPC calls from starving rayon's pool, which should be reserved for CPU-bound work.
See `sync_status.rs` for the pattern.

**Never `tokio::spawn` from the notify-rs debouncer callback.** The callback runs on the notify-rs internal thread
which has no Tokio runtime context — `tokio::spawn` panics with "there is no reactor running". Use
`tauri::async_runtime::spawn` instead (same pattern `indexing::watcher` uses). This bit the file watcher's full-reread
fallback path (`watcher.rs` — `>500` events or ambiguous event kinds).
