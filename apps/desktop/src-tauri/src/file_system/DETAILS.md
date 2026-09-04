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

**Read-path, not watcher.** Filtering where the frontend asks for a range (`CachedListing::rows`) rather than where the
cache is filled is what makes the fix safe. The cache stays the truth; every accessor re-tests on every fetch, so an
entry the pane received can always be taken away again. Filtering the watcher instead inverts the bug into a worse one:
a full listing shows the temp, the watcher skips the removal that would clear it, and the pane keeps an entry pointing
at nothing. The `.sb-` filter lived in `crates/cmdr-smb/src/volume/watcher.rs` from 2026-04-10 to 2026-08-01 and had exactly that ghost — its
`continue` sat above the `match action`, so it skipped `Removed` too.

**Re-testing on every fetch is not the same as re-deriving the whole sequence on every fetch**, and conflating the two
is what wedged a big directory (`listing/DETAILS.md` § "Row numbers"). A listing materializes its row numbers once and
keeps the scratch-named entries — the only ones whose answer can still change — in a short side list it re-asks about
per read. `could_be_hidden_from_listings` is the pure name test that gates `is_hidden_from_listings`, so "this name is
settled" holds by construction rather than by reading three functions and hoping.

**Other apps' scratch hides by NAME, and that's a different rule on purpose.** macOS safe-save writes
`file.txt.sb-<uuid>` next to the original on every save (TextEdit, Preview, anything on `NSDocument`). There's no
ownership signal available for a file another process is writing and no way to tell a live one from an abandoned one, so
`advanced.showSafeSaveFiles` is a plain name filter over every drive. That coarseness is acceptable only because the
files aren't ours: an abandoned `.sb-` says something about TextEdit's day, not about a Cmdr bug, so nothing diagnostic
is lost by not surfacing it. It defaults to ON (shown), where Cmdr's own defaults to hidden — hiding another app's files
by name is a bigger claim to make on someone's behalf than hiding our own.

**Ownership, not name.** A scratch file a live operation owns is noise; one nobody owns is a leftover from an
interrupted transfer, and hiding that misreports what's on disk. The mint, the in-flight registry, and why the RAII
guard needs a liveness token behind it all live in `crates/cmdr-fs/src/staging.rs`, so a backend crate can stage a write
without reaching into the app; `staging.rs` re-exports them and adds only the two visibility settings.

**Which token the app hands it.** An operation's temps carry a `Weak` to `WriteOperationState`'s liveness token, dropped
by `end_liveness` wherever the operation leaves `WRITE_OPERATION_STATE`. ❌ Not `Arc<WriteOperationState>` reachability —
a task the driver abandoned holds one of those too, and the whole point is that its leftovers stop being hidden. A temp
minted outside any operation (the local safe-overwrite's two files) passes `None` and hides only until its function
returns.

**Known gap.** A leftover only becomes visible on the next fetch, and nothing forces one at settle. In practice
`transfer/volume/cleanup.rs::clean_abandoned_staged_writes` deletes the leftovers and that delete fires a watcher event,
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
  It's macOS 12+, above the bundle's 10.15 floor, so `fetch_candidates_for_path` gates on
  `crate::platform::macos_at_least(12, 0)` and drops to `URLForApplicationToOpenURL:` (macOS 10.10) below that. Catalina
  and Big Sur therefore see only the OS default app in the menu, which is a shorter list rather than a crash: an
  unrecognized selector would raise `NSInvalidArgumentException` and abort the process. The `allowed-newer-selector`
  marker on the call is what tells `desktop-rust-macos-availability` the gate exists (it reads lines, not control
  flow).
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

## Arming a listing watch is detached

`start_watching_detached` hands the arm to the blocking pool; the listing pipeline never waits for it. Arming is slow,
and slow by an amount that has nothing to do with the directory being listed:

- `FSEventStreamCreate` + `FSEventStreamStart` are a handshake with `fseventsd`, and `notify` blocks until the stream's
  new CFRunLoop thread has published its `CFRunLoopRef`.
- `coverage_for_watched_path` adds a `statfs`.
- Worst of all, it queues on `WATCHER_MANAGER` behind the PREVIOUS listing's teardown, because the frontend fires
  `listDirectoryEnd(old)` immediately before loading the new directory (`listing-loader.ts`).

That mattered because `read_directory_with_progress` armed the watch before it emitted `listing-complete`, and the pane
renders nothing until that event (`listing-loader.ts::handleListingComplete` is the only place a listing is committed).
So the whole arm was dead time the user saw as a stalled "Sorting your files, preparing view…".

Measured while navigating a warm `~/Downloads` (macOS 26.5.2, 2026-08-11, from the `stall_probe::listing` line): p50
88 ms, p75 288 ms, p90 653 ms, max 1,509 ms, against `read_dir` 0–8 ms and `sort` 0 ms. A release build on the same
machine was worse (p90 775 ms, max 5,081 ms), so this was never a debug-build artifact. **Cost is independent of
directory size** — a 3-entry folder hit 723 ms while a 265-entry folder hit 57 ms — which is what says "lock and
run-loop scheduling", not "I/O proportional to the work".

Two supporting changes came with it:

- **`stop_watching` drops the `WatchedDirectory` OUTSIDE the manager's write lock.** Dropping it tears an FSEvents run
  loop down, and notify's teardown busy-spins on `CFRunLoopIsWaiting` before joining the stream's thread. Holding the
  write lock across that is what made an arm queue behind the previous teardown. ❌ Don't fold the removal and the drop
  back into one `if let Ok(mut manager) = …` block.
- **The debouncer uses `NoCache`, not the platform-default `RecommendedCache`** (a `FileIdMap` on macOS). The map exists
  to pair a rename's `From` with its `To` by file id, and pays for it by walking the watched directory and `stat`ing
  every entry at arm time, then re-`stat`ing on every create, rename, and remove. Cmdr gets nothing for that:
  `handle_directory_change_incremental` collects the unique paths out of a batch and re-stats each one, so a rename
  classifies identically whether it arrives as one paired event carrying both paths or as a separate `From` and `To`.
  Root-rename detection is unaffected too, since `watch_root_identity_changed` matches on `Modify(Name(_))`, which the
  debouncer emits either way. Linux already ran this path with `NoCache`.

**The reconcile half of a detached arm is not optional.** `list_directory_end` removes the listing from `LISTING_CACHE`
and then removes a watch the arm may not have inserted yet, so an arm can land on a listing nobody will ever close
again. `arm_and_reconcile` re-checks `LISTING_CACHE` membership after arming and tears the watch down if the listing
went away; without that, each such navigation strands an FSEvents stream, its CFRunLoop thread, and a manager entry for
the life of the process, each one still costing `fseventsd` fan-out. Pinned by
`watcher_test::a_detached_arm_that_lost_the_race_leaves_no_watch_behind`, which drives `arm_and_reconcile` directly so
the losing interleaving is the only one under test.

`watcher_start_ms` in the `stall_probe::listing` line now measures only the dispatch, so it should read ~0. A
regression that puts arming back on the critical path shows up there first.

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

## Reordered rows

A row whose own sort key changes (an mtime bump under sort-by-date, a size change under sort-by-size) jumps to a new
position while every other row merely slides over. `DiffChangeType::Move` reports that jump with `previous_index` and
`index`, and it carries the fresh entry, so it replaces the `Modify` rather than accompanying it.

**Why a dedicated variant rather than a remove plus an add**: the frontend rides the pane cursor and the selection along
a move by identity (`listing-diff-sync.svelte.ts::reconcileCursorAndSelection`). Reported as a remove plus an add, the
cursor instead stays on the vacated index, which now holds a neighbour. Watch a big folder being deleted in a
date-sorted pane and that reads as if the wrong folder were disappearing, which is exactly how it was found.

Both watcher paths produce it: the incremental path from `ModifyResult::Moved` (the cache re-inserts the entry at its
new sorted position and knows both indices), and the full re-read path from `compute_diff`.

`compute_diff` has to separate a real jump from the index shift every row below an add or a remove takes, or an ordinary
delete would make the pane chase the cursor around. It reads the surviving rows' old positions in new order and keeps
the longest increasing subsequence of them (patience sorting, O(n log n)); those rows held their relative order, and the
rest are the minimal set that genuinely moved.

**One event's removals are one call**, `listing::remove_entries_by_paths`, not a loop: the batch resolves every doomed
row against the pre-removal listing in a single pass under one write lock, which is what keeps the indices the
`directory-diff` carries in one index space and stops a 500-path event walking the listing 1,000 times. Why, and what
the loop cost: `listing/DETAILS.md` § "Entries by path".

## Replacing a watch root

The incremental watcher path classifies each event against the cached listing, so it can only learn about entries the OS
names. When the watched directory is itself replaced (a `git checkout` across branches, `rsync --delete`, unzipping over
a folder, a build regenerating its output dir), macOS names almost none of them.

Measured against a live pane by logging the raw debounced batch (macOS 26.5.2, `notify-debouncer-full` 0.7.0,
2026-08-08),
`rm -rf target && mkdir target && touch gamma.txt delta.txt` delivers exactly:

- `Remove(Folder)` on the watch root
- `Create(Folder)` on the watch root
- `Modify(Metadata(Extended))` on the watch root
- `Create(File)` for each NEW child

There is **no remove event for the old children at all**: the directory went away as a unit, and FSEvents reports the
unit. All three root-level events are correctly rejected by `rebase_event_path` (their parent isn't the watch root), so
a child-only classifier applied the adds and kept every entry the replacement took away. The pane then showed a union of
the old and new listing, indefinitely: no later event ever mentions a removed name, and the ghosts survive a `⌘R`-less
session until the user navigates away and back. Repeated replacements stack more ghosts. The FSEvents stream itself
survives the replacement, so this is not a dead watch: the pane keeps getting *some* updates, which is what made it look
like a refresh timing issue rather than a correctness one.

`watch_root_identity_changed` therefore escalates to the full re-read (`handle_directory_change`) whenever a
`Create`, `Remove`, or `Modify(Name)` event names the watch root itself (in either path form, via
`event_targets_watch_root`, for the same reason `rebase_event_path` needs two). The re-read diffs against disk, replaces
the listing, and, when the directory is genuinely gone, emits `directory-deleted` and stops the watch.

**Gotcha**: `Modify(Metadata(_))` on the root is deliberately not a trigger. Every ordinary child create or remove bumps
the directory's own mtime and produces one, so counting it would route every change through the full re-read and cost
the incremental path its entire reason to exist.

## Tag analytics

`toggle_color` reports `tag_toggled` at its single exit, so all three triggers (the seven keyboard commands, the
context-menu circles, and the MCP `tag` tool) are covered without any of them having to remember. The trade is that
the event can't say WHICH trigger fired; a `surface` parameter would have meant threading one through every caller and
every test for a question none of them asks yet. ❌ The `color` prop is the Finder palette's canonical name (a closed
set of seven), never a tag's own text, which is user-authored content. Props: `src-tauri/src/analytics/DETAILS.md` §
"Starter event set".
