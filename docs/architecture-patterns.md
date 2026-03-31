# Cross-cutting architecture patterns

Deep reference for how Cmdr's subsystems interact. Read the relevant section when working on navigation, file
operations, volumes, or cancellation. For the subsystem map, see [architecture.md](architecture.md).

## Data flow: frontend <> backend

File data lives in Rust (`LISTING_CACHE`). Frontend fetches visible ranges on-demand via IPC (`getFileRange`). This
avoids serializing 50k+ entries. Virtual scrolling renders only ~50 visible items.

## Navigation lifecycle

User navigates -> old listing cleaned up -> new listing started -> events stream back -> UI updates.

**Three navigation types, same cleanup/load sequence:**

| Type                | Entry point                                                                                           | Who moves history?                                                         | Timing                                                                            |
| ------------------- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| **Enter on folder** | `FilePane.handleNavigate` -> `loadDirectory`                                                          | `DualPaneExplorer.applyPathChange` pushes history AFTER `listing-complete` | History push on success only                                                      |
| **Back/forward**    | `DualPaneExplorer.handleNavigationAction` -> `setPanePath` -> FilePane `$effect` -> `loadDirectory`   | `updatePaneAfterHistoryNavigation` moves history BEFORE load               | Optimistic — if path is gone, error handler resolves upward                       |
| **Volume switch**   | `VolumeBreadcrumb.onVolumeChange` -> `FilePane.loadDirectory` + `DualPaneExplorer.handleVolumeChange` | Pushed immediately in `handleVolumeChange`                                 | Optimistic — `determineNavigationPath` may correct to a better path in background |

**Old listing cleanup** (in `FilePane.loadDirectory`, every navigation):

1. `++loadGeneration` — invalidates all in-flight events
2. `cancelListing(oldId)` -> sets `AtomicBool` in Rust -> background task stops within ~100ms
3. `listDirectoryEnd(oldId)` -> stops file watcher, removes from `LISTING_CACHE`
4. Unlisten all 6 event listeners
5. Generate new `listingId` (frontend `crypto.randomUUID()`), subscribe new listeners, call `listDirectoryStart`

**Volume switch specifics**: `handleVolumeChange` saves the old volume's `lastUsedPath` immediately (no debounce), then
runs `determineNavigationPath` in background (each check has 500ms timeout). A `volumeChangeGeneration` counter guards
against stale corrections if the user switches again.

## Listing lifecycle

```
Frontend (FilePane)                    Rust backend (streaming.rs)
   |                                        |
   |-- loadDirectory(path)                  |
   |   listingId = randomUUID()             |
   |   listDirectoryStart(...) ------------>| spawn tokio task -> spawn_blocking
   |<-- { listingId, status: Loading }      |
   |                                        |-- emit listing-opening
   |                                        |-- volume.list_directory_with_progress()
   |<-- listing-progress (every 200ms) -----|   (on separate OS thread, polls cancel every 100ms)
   |<-- listing-read-complete --------------|-- sort + enrich + cache insert (atomic with cancel check)
   |                                        |-- start file watcher
   |<-- listing-complete ------------------|
   |   handleListingComplete()              |
   |   onPathChange -> push history          |
   |                                        |
   |== ACTIVE: getFileRange on demand =====>|== LISTING_CACHE serves ranges
   |<= directory-diff events ===============|== file watcher detects changes
   |                                        |
   |== CLEANUP (next nav or destroy) =======|
   |-- cancelListing(id) ----------------->|-- AtomicBool -> task exits
   |-- listDirectoryEnd(id) -------------->|-- stop watcher, remove from cache
```

Multiple listings coexist (two panes, rapid navigation). Each keyed by unique `listingId` in all global state:
`LISTING_CACHE`, `STREAMING_STATE`, `WATCHER_MANAGER`. Events carry `listingId`; listeners filter by it.

## Concurrency guards

| Guard                    | Defined in                                   | Incremented by                                              | Checked by                                                                  | Prevents                                                         |
| ------------------------ | -------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `loadGeneration`         | `FilePane` (per-instance)                    | `loadDirectory()`, `adoptListing()`                         | All 6 listing event handlers; post-`listDirectoryStart` staleness check     | Stale listing events from previous navigation applied to current |
| `listingId` match        | `FilePane` (per-instance)                    | Set to new UUID each `loadDirectory()`                      | Every event handler (belt-and-suspenders with `loadGeneration`)             | Wrong listing's events applied to a different navigation         |
| `volumeChangeGeneration` | `DualPaneExplorer` (singleton)               | `handleVolumeChange()`                                      | `determineNavigationPath` callback                                          | Stale path corrections applied after user switched volumes again |
| `cacheGeneration`        | `FilePane` -> prop to `FullList`/`BriefList` | `refreshView()`, `adoptListing()`, `directory-diff` handler | Virtual scroll `$effect` via `shouldResetCache()`                           | Stale frontend scroll cache after sort/filter/watcher changes    |
| `AtomicBool` (Rust)      | `StreamingListingState` per listing          | `cancel_listing()` IPC                                      | Per-entry during read, every 100ms poll, at cache-insert (under write lock) | Cancelled listing inserting into cache or continuing I/O         |

## Cancellation patterns

Three architectural patterns used consistently:

1. **`AtomicBool` flag (Rust)**: Every long-running backend task (listing, copy/move/delete, scan, search, indexing, AI
   download) uses an `AtomicBool` checked at iteration boundaries. Stored in global `HashMap` keyed by operation ID.
2. **Generation counter (TypeScript)**: Incremented on each new request. Stale responses silently discarded.
   Lightweight, no backend coordination needed. Used for `loadGeneration` and `currentFetchId` (viewer).
3. **Tauri event feedback**: Backend emits terminal events (`listing-cancelled`, `write-cancelled`, etc.) after
   cancellation completes. For rollback operations, the frontend waits for this event before closing the dialog.

| Operation          | User trigger                  | Frontend                                 | Backend                                                                | Coordinated?                                  |
| ------------------ | ----------------------------- | ---------------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------- |
| Directory listing  | ESC / new navigation          | `loadGeneration++` + `cancelListing` IPC | `AtomicBool` per-entry, 100ms poll                                     | Yes                                           |
| File copy/move     | Cancel button in dialog       | `cancelWriteOperation` IPC               | `AtomicBool` per-file; `CopyTransaction.rollback()` if requested       | Yes (waits for `write-cancelled` on rollback) |
| File delete/trash  | Cancel button in dialog       | `cancelWriteOperation` IPC               | `AtomicBool` per-file; **no rollback** (already deleted)               | Yes, but irreversible                         |
| Scan preview       | Dialog close                  | `cancelScanPreview` IPC                  | `AtomicBool` per-entry                                                 | Yes                                           |
| File viewer search | New query / ESC / close       | `viewerSearchCancel` IPC                 | `AtomicBool` in search loop                                            | Yes                                           |
| Drive indexing     | App shutdown / volume unmount | `stopDriveIndex` IPC                     | `AtomicBool` on jwalk walker; incomplete scan detected on next startup | Yes                                           |
| AI model download  | Cancel button                 | `cancelAiDownload` IPC                   | Flag checked per HTTP chunk; partial file kept for resume              | Yes                                           |
| Viewer line fetch  | Rapid scroll                  | `currentFetchId++` (discard stale)       | None — backend serves all                                              | No, frontend-only                             |
| Inline rename      | ESC / navigate away           | `rename.cancel()` resets state           | None — purely frontend until submit                                    | No, frontend-only                             |

**Known gap**: On stuck network mounts, the OS `read_dir` syscall blocks the I/O thread. The `AtomicBool` check runs
between entries, not during the syscall. Mitigation: I/O runs on a separate OS thread; the main task polls via
`mpsc::channel` every 100ms and can respond to cancellation without waiting for the syscall.

## Volume mount/unmount chain

```
Detection:
  macOS: FSEvents on /Volumes (non-recursive)
  Linux: inotify on /proc/mounts + /run/user/<uid>/gvfs/
  MTP:   nusb USB hotplug stream (separate system, own events)

-> State diff against KNOWN_VOLUMES (implicit debounce — multiple FSEvents, one diff)

-> Rust processing:
    Mount:   register LocalPosixVolume with VolumeManager, emit "volume-mounted"
    Unmount: unregister from VolumeManager, emit "volume-unmounted"

-> Frontend listeners (both fire independently):
    VolumeBreadcrumb: clear space cache, reload volume list (dropdown refresh)
    DualPaneExplorer: mount -> refresh list; unmount -> handleVolumeUnmount()
```

**`handleVolumeUnmount`**: Hard redirect — any pane on the unmounted volume switches to `~` on root volume. No
parent-walking (entire volume is gone). Both panes checked independently. State persisted immediately.

**Safety nets against races**: FilePane's 2-second `dirExistsPollInterval` also detects missing paths. If the volume
root itself is gone, it defers to the unmount handler (avoids double-navigation). If only a subdirectory is gone but the
volume exists, it resolves upward via `resolveValidPath`.

**MTP differs**: Detection via USB hotplug (not filesystem). Three events: `mtp-device-detected`, `mtp-device-removed`,
`mtp-device-connected`. VolumeManager registration happens on explicit `connect()`, not on detection. SMB shares on
macOS appear under `/Volumes` and use the same path as local drives.

## Error recovery

| Scenario                 | Detection                                                                  | User sees                                                | Recovery                                                                                 | Cleanup                                               |
| ------------------------ | -------------------------------------------------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| **Path deleted**         | `listing-error` + `pathExists` check; watcher `directory-deleted`; 2s poll | Brief spinner -> auto-navigates to parent                | `resolveValidPath`: walk parents -> `~` -> `/` (each step 1s frontend + 2s Rust timeout) | `cancelListing` + `listDirectoryEnd`                  |
| **Permission denied**    | Rust `PermissionDenied` -> `listing-error`                                 | `PermissionDeniedPane` with OS-specific fix instructions | None (manual fix required)                                                               | `listingId` cleared, no cache/watcher                 |
| **Network slow/dead**    | Frontend timeouts (500ms/1s); Rust timeout (2s); ESC cancel                | "Opening folder..." -> progress -> "Press ESC to cancel" | ESC navigates back; timeouts cause graceful fallback                                     | `AtomicBool` cancellation                             |
| **Mid-stream I/O error** | Rust error through channel -> `listing-error`                              | Spinner -> auto-navigates to parent                      | Same as "path deleted"                                                                   | No partial cache (listing is atomic — all or nothing) |
| **Volume unmounted**     | `volume-unmounted` Tauri event (dedicated handler)                         | Pane switches to home directory                          | Hard switch to root volume + `~`                                                         | Full pane state overwrite + persist                   |
| **MTP disconnect**       | `mtp-device-removed` event                                                 | Falls back to default volume                             | `handleMtpFatalError` -> root volume + `~`                                               | Same as volume unmount                                |

**Per-entry permission errors** (single unreadable file in a readable dir) don't fail the listing — they appear as
zero-permission entries with fallback metadata.

## Persistence

- **App status** (`app-status.json`): ephemeral state — paths, focused pane, view modes, last-used paths per volume
- **Settings** (`settings.json`): preferences — hidden files, density, date format. Registry-validated.
- **Shortcuts** (`shortcuts.json`): delta-only — only customizations stored, defaults in code
- **License** (`license.json`): activation state, timestamps
- **Window state**: `@tauri-apps/plugin-window-state` for size/position per window label

Philosophy: status is "where you are" (ephemeral), settings are "how you like it" (preferences).

**Persistence timing** (what's at risk on crash):

| State                                     | Timing                                                   | Crash loss                                   |
| ----------------------------------------- | -------------------------------------------------------- | -------------------------------------------- |
| Pane paths, focused pane, view mode, sort | Debounced 200ms (`saveAppStatus`)                        | Up to 200ms of changes                       |
| Tab state (paths, sort, viewMode, pinned) | **Immediate** (no debounce)                              | None — tabs are the reliable source of truth |
| `lastUsedPath` per volume                 | **Immediate** (no debounce)                              | None                                         |
| Settings v2                               | Debounced 500ms; explicit flush on Settings window close | Up to 500ms if main window crashes           |
| Shortcuts                                 | **Immediate** (changes are rare user actions)            | None                                         |
| License                                   | **Immediate** (Rust `autoSave`)                          | None                                         |
| Window size/position                      | Debounced 500ms on resize; immediate on normal close     | Size since last resize settled               |
