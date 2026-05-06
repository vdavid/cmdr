# File system module

Core filesystem operations: directory listing, file writing, sync status, volume management, and file watching.

Submodule docs: [listing/](listing/CLAUDE.md), [write_operations/](write_operations/CLAUDE.md), [volume/](volume/CLAUDE.md).

## Cloud actions and "Open with" (macOS)

- `cloud_actions.rs` — wraps `NSFileProviderManager.evictItem(...)` and
  `requestDownloadForItem(...)` so the file context menu can offer "Make available offline" and
  "Remove download" for any File-Provider-managed file (iCloud Drive, Dropbox, Google Drive,
  OneDrive, Box). Detection is fast (`is_in_cloud_storage` — pure path-prefix check against
  `~/Library/Mobile Documents/com~apple~CloudDocs` and `~/Library/CloudStorage/`); the actual
  evict/download chain calls async FP APIs synchronously via completion handlers + `mpsc::sync_channel`.
- `open_with.rs` — `URLsForApplicationsToOpenURL:` for candidate apps, with multi-selection
  intersection. Session cache keyed by lowercased extension. Subscribes to
  `NSWorkspace.didLaunchApplicationNotification` / `didTerminateApplicationNotification` for
  invalidation (per AGENTS.md "Subscribe, don't poll" — TTL is fallback only). `open_paths_with`
  launches with one multi-URL `openURLs:withApplicationAtURL:configuration:completionHandler:`
  call. `pick_app_via_open_panel` shows `NSOpenPanel` filtered to `.app` bundles for the
  "Open with → Other..." entry. Worker threads use 8 MB stacks (FileProvider XPC depth).

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
