# File system details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is everything else. Submodule depth lives in each
submodule's own `DETAILS.md` (`listing/`, `write_operations/`, `volume/`).

## What `mod.rs` is for

`mod.rs` is a facade: it re-exports the listing, write-operation, volume, and watcher surfaces upward, and it owns
startup wiring that legitimately needs to know every backend (`init_volume_manager`, `register_discovered_volumes`,
`upgrade_existing_smb_mounts`, the direct-SMB and SMB-concurrency settings).

What it deliberately does NOT own is the volume registry itself. The singleton and `get_volume_manager()` live in
`volume/manager.rs` beside the type, and call sites import them from there. Reasons, and why a `pub use` shim back into
this module is the thing to resist: `volume/DETAILS.md` § "Key decisions".

## Hiding transient scratch (`staging.rs`)

Every write lands on a `.cmdr-tmp-*` sibling and takes its real name by a rename, so a copy makes files appear under
names the user didn't create for as long as it takes to write them. The 2026-07-31 incident's visible tail was one of
those left in the pane after an otherwise successful 768-file copy: the SMB watcher won the race against the rename, and
its batched add landed after the rename event that would have cleared it.

**Read-path, not watcher.** Filtering where the frontend asks for a range (`listing/operations.rs::visible_entries`)
rather than where the cache is filled is what makes the fix safe. The cache stays the truth; every accessor re-tests on
every fetch, so an entry the pane received can always be taken away again. Filtering the watcher instead inverts the bug
into a worse one: a full listing shows the temp, the watcher skips the removal that would clear it, and the pane keeps
an entry pointing at nothing. The `.sb-` filter lived in `smb_watcher.rs` from 2026-04-10 to 2026-08-01 and had exactly
that ghost — its `continue` sat above the `match action`, so it skipped `Removed` too.

**Other apps' scratch hides by NAME, and that's a different rule on purpose.** macOS safe-save writes
`file.txt.sb-<uuid>` next to the original on every save (TextEdit, Preview, anything on `NSDocument`). There's no
ownership signal available for a file another process is writing and no way to tell a live one from an abandoned one, so
`advanced.showSafeSaveFiles` is a plain name filter over every drive. That coarseness is acceptable only because the
files aren't ours: an abandoned `.sb-` says something about TextEdit's day, not about a Cmdr bug, so nothing diagnostic
is lost by not surfacing it. It defaults to ON (shown), where Cmdr's own defaults to hidden — hiding another app's files
by name is a bigger claim to make on someone's behalf than hiding our own.

**Ownership, not name.** A scratch file a live operation owns is noise; one nobody owns is a leftover from an
interrupted transfer, and hiding that misreports what's on disk. `StagingTemp` is the only way to name a temp, so
registering can't be forgotten, and the guard's `Drop` is what un-hides.

**Why a guard alone isn't enough.** The transfer driver ABANDONS tasks that won't wind down under the cancel deadline,
and an abandoned task keeps its guard alive: exactly the wedge case, where a leaked registration would hide a real
leftover forever. So an operation's temps also carry a `Weak` to `WriteOperationState`'s liveness token, dropped by
`end_liveness` wherever the operation leaves `WRITE_OPERATION_STATE`. ❌ Not `Arc<WriteOperationState>` reachability —
the abandoned task holds one of those too. A force-quit is covered for free (the registry is in memory), and a temp
minted outside any operation (the local safe-overwrite's two files) hides only until its function returns.

**Known gap.** A leftover only becomes visible on the next fetch, and nothing forces one at settle. In practice
`transfer/volume_cleanup.rs::clean_abandoned_staged_writes` deletes the leftovers and that delete fires a watcher event,
so the pane updates on its own; the gap is the narrow case where the DELETE also fails. An immediate reveal would need
the volume-path to display-path mapping the name-keyed registry deliberately avoids.

**The escape hatch.** `advanced.showStagingTempFiles` (Settings > Advanced, off) shows the in-flight ones too. It's a
separate axis from `showHiddenFiles`: turning on dotfiles isn't a request to watch Cmdr's scratch.

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

## Threading

The 8 MB-stack OS thread pattern (instead of rayon) for macOS framework calls is in `sync_status/pool.rs` as the
reference. The reasoning: NSURL resource-value lookups and FileProvider queries make synchronous XPC round-trips that
can consume deep stack frames through FileProvider override chains (iCloud, Dropbox), exceeding rayon's 2 MB worker
stack; running them on rayon would also starve the pool, which should stay reserved for CPU-bound work.

A per-call `std::thread::scope` isn't good enough on its own: those calls can block forever, so the threads have to be
pooled and hard-capped or they accumulate (21-23 of them in the 2026-07-31 wedge). `src/icons/` (a separate top-level
module) follows the same rule for `fetch_path_icons`.

## Watcher threading

The notify-rs debouncer callback runs on notify-rs's internal thread, which has no Tokio runtime, so `tokio::spawn`
there panics with "there is no reactor running". It bit `watcher.rs`'s full-reread fallback path (`>500` events or
ambiguous event kinds), then again in v0.24.0 via `git::watcher::refresh_local_listings_under` →
`listing::caching::notify_directory_changed(FullRefresh)` (CRASH-26SBB) — which is why FullRefresh dispatch now funnels
through `caching::spawn_full_refresh`. Use `tauri::async_runtime::spawn` (same as `indexing::watch::watcher`), and
apply the rule to every watcher OS thread (git, SMB, MTP, archive), not just notify-rs.

## Watcher path rebasing

On macOS, FSEvents reports canonical paths (`/private/tmp/…`) while `LISTING_CACHE` holds the user-navigated form
(`/tmp/…`). `watcher.rs::rebase_event_path` compares the firmlink-normalized forms
(`indexing::paths::firmlinks::normalize_path`) and rebases matching event paths onto the listing's directory. A raw
`path.parent() == dir_path` comparison silently dropped every event for listings under `/tmp`, `/var`, and `/etc`, so
the pane never updated until the user re-navigated.

FSEvents also resolves a **symlinked watch root** and reports events under the real target, so the handler additionally
matches against the `canonicalize`d watch dir. This bit Google Drive, whose `My Drive` is a symlink to `~/My Drive`, so
rename/create/delete never refreshed the pane; iCloud and Dropbox mount real directories and hit the firmlink path
instead.
