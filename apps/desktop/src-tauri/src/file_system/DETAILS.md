# File system details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is everything else. Submodule depth lives in each
submodule's own `DETAILS.md` (`listing/`, `write_operations/`, `volume/`).

## Cloud actions (`cloud_actions.rs`)

Wraps `FileManager.evictUbiquitousItem(at:)` and `startDownloadingUbiquitousItem(at:)` so the file context menu can
offer "Make available offline" and "Remove download". **iCloud Drive only.**

`NSFileProviderManager`'s host-side methods looked like the cross-provider API but are reserved for the app that
*bundles* the File Provider extension (Dropbox.app for Dropbox, and so on); a third-party app gets
`NSFileProviderErrorProviderNotFound` ("The application cannot be used right now") on the enumerate / evict / download
calls. The `FileManager` ubiquity APIs route through iCloud's separate code path and accept any URL inside an iCloud
container, so the menu items are offered only for paths under `~/Library/Mobile Documents/com~apple~CloudDocs/`.
`is_in_icloud_drive` (strict path-prefix check) gates them. The module-doc comment in `cloud_actions.rs` has the full
story.

## Open with (`open_with.rs`)

- `URLsForApplicationsToOpenURL:` produces candidate apps, with multi-selection intersection across the selected files.
- A session cache keyed by lowercased extension avoids repeated lookups; it subscribes to
  `NSWorkspace.didLaunchApplicationNotification` / `didTerminateApplicationNotification` for invalidation (per the
  "Subscribe, don't poll" principle; the TTL is a fallback only).
- `open_paths_with` launches with a single multi-URL
  `openURLs:withApplicationAtURL:configuration:completionHandler:` call.
- `pick_app_via_open_panel` shows an `NSOpenPanel` filtered to `.app` bundles for the "Open with → Other…" entry.
- Worker threads use 8 MB stacks (FileProvider XPC depth), per the gotcha in `CLAUDE.md`.

## Finder tags MCP consumer (`tags.rs`)

The MCP `tag` tool wraps `tags::toggle_color` / `set_tags` (and `system_color_name` for canonical names), resolving
target paths off the pane state and refreshing via `apply_tags_to_listing`. `cmdr://state` file entries also surface a
`[tags:…]` marker mirrored from `PaneFileEntry.tags`. See `mcp/DETAILS.md`.

## Threading rationale

The 8 MB-stack OS thread pattern (instead of rayon) for macOS framework calls is in `sync_status.rs` as the reference.
The reasoning: NSURL resource-value lookups and FileProvider queries make synchronous XPC round-trips that can consume
deep stack frames through FileProvider override chains (iCloud, Dropbox), exceeding rayon's 2 MB worker stack; running
them on rayon would also starve the pool, which should stay reserved for CPU-bound work.
