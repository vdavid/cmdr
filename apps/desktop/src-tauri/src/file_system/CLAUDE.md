# File system module

Core filesystem operations: directory listing, file writing, sync status, volume management, and file watching.

Submodule docs: [listing/](listing/CLAUDE.md), [write_operations/](write_operations/CLAUDE.md),
[volume/](volume/CLAUDE.md). Top-level files of note: `cloud_actions.rs` (iCloud make-available-offline / remove-download),
`open_with.rs` (candidate apps + launch), `watcher.rs` (FSEvents incremental listing updates), `sync_status.rs`.

## Gotchas

- **Never use rayon (or any constrained-stack thread pool) for calls into macOS frameworks.** NSURL resource lookups,
  FileProvider queries, and similar Objective-C APIs make synchronous XPC round-trips to system daemons that can descend
  through FileProvider override chains (iCloud, Dropbox) and blow rayon's default 2 MB worker stack. Use dedicated OS
  threads with an explicit 8 MB stack instead. This also keeps I/O-bound XPC off rayon's pool, which is for CPU-bound
  work. `sync_status.rs` is the reference pattern. (The `src/icons/` module, a separate top-level module, follows the
  same rule for `fetch_path_icons`; see its `CLAUDE.md`.)
- **Never `tokio::spawn` from the notify-rs debouncer callback.** It runs on the notify-rs internal thread with no Tokio
  runtime, so `tokio::spawn` panics with "there is no reactor running". Use `tauri::async_runtime::spawn` (same as
  `indexing::watcher`). This bit the watcher's full-reread fallback path (`watcher.rs`, `>500` events or ambiguous event
  kinds).
- **Watcher event paths must be rebased into the listing's path space** (`watcher.rs::rebase_event_path`). On macOS,
  FSEvents reports canonical paths (`/private/tmp/…`) while `LISTING_CACHE` holds the user-navigated form (`/tmp/…`).
  The incremental handler compares the firmlink-normalized forms (`indexing::firmlinks::normalize_path`) and rebases
  matching event paths onto the listing's directory. A raw `path.parent() == dir_path` comparison silently dropped every
  event for listings under `/tmp`, `/var`, and `/etc`, so the pane never updated until the user re-navigated.
- **`cloud_actions.rs` is iCloud Drive only.** The `NSFileProviderManager` host-side methods look cross-provider but are
  reserved for the app that *bundles* the File Provider extension, so third-party apps get
  `NSFileProviderErrorProviderNotFound`. The `FileManager` ubiquity APIs route through iCloud's path and accept any URL
  in an iCloud container, so the menu items are gated by `is_in_icloud_drive` (strict prefix check against
  `~/Library/Mobile Documents/com~apple~CloudDocs/`). Don't widen this to other providers.

Full details (Open with: candidate intersection, session cache, NSWorkspace launch/terminate invalidation, open-panel
fallback; the full cloud-actions rationale): [DETAILS.md](DETAILS.md).
