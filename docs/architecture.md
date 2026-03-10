# Architecture

Map of Cmdr's major subsystems. Each directory has detailed docs in their `CLAUDE.md` file!

## Frontend (Svelte 5 + TypeScript)

All under `apps/desktop/src/lib/`.

| Directory | Purpose |
|-----------|---------|
| `file-explorer/` | Dual-pane file explorer — pane orchestration, selection, navigation, sorting |
| `file-explorer/views/` | Virtual-scrolling file lists (Brief + Full modes), 100k+ file support |
| `file-explorer/drag/` | Native drag-and-drop (drag-out, drop-in, pane-to-pane, macOS image swizzle) |
| `file-explorer/rename/` | Inline rename with validation, conflict resolution, extension change |
| `file-explorer/selection/` | Space/Shift/Cmd selection, range operations |
| `file-explorer/navigation/` | Back/forward history, breadcrumb, path utilities |
| `file-explorer/network/` | Network browser UI (SMB share browsing, login form) |
| `file-operations/` | Transfer dialogs (copy/move/mkdir) with progress and conflict resolution |
| `file-viewer/` | Read-only file viewer (opens in separate window, virtual scrolling) |
| `settings/` | Settings UI + registry-based architecture, reactive state |
| `shortcuts/` | Keyboard shortcut customization, scope hierarchy, conflict detection |
| `tauri-commands/` | Typed TypeScript wrappers for all Tauri IPC commands and events |
| `command-palette/` | Fuzzy command search (~45 commands) |
| `commands/` | Command registry (~50 commands), fuzzy search engine for command palette |
| `licensing/` | License validation, commercial reminders, expiration modals |
| `logging/` | Unified logging: LogTape config, batching bridge to Rust, verbose toggle |
| `ai/` | Local LLM features (folder suggestions), download flow |
| `indexing/` | Drive index state, events, priority triggers, scan status overlay |
| `mtp/` | MTP (Android device) file browsing UI |
| `onboarding/` | Full Disk Access prompt for first-launch onboarding |
| `ui/` | Shared UI primitives: ModalDialog, Button, AlertDialog, LoadingIcon, Notification, dialog registry |
| `updates/` | Auto-updater UI |
| `utils/` | Filename validation, confirm dialog utilities |
| `font-metrics/` | Character width measurement for accurate Brief mode column sizing |

## Backend (Rust + Tauri 2)

All under `apps/desktop/src-tauri/src/`.

| Directory/file | Purpose |
|----------------|---------|
| `file_system/listing/` | Directory reading, streaming, caching, sorting — serves virtual scroll |
| `file_system/write_operations/` | Copy/move/delete with safety patterns (temp+rename, staging, rollback) |
| `file_viewer/` | Three-backend file viewer (FullLoad, ByteSeek, LineIndex) |
| `network/` | SMB: mDNS discovery, share listing (smb-rs + smbutil), mounting, Keychain |
| `mtp/` | MTP device management, file ops, event-based watching |
| `mcp/` | MCP server (19 tools, YAML resources, agent-centric API) |
| `ai/` | llama-server lifecycle, model download, inference client |
| `licensing/` | Ed25519 license verification, server validation |
| `settings/` | Settings persistence (tauri-plugin-store) |
| `indexing/` | Background drive indexing (SQLite, jwalk, FSEvents), recursive directory sizes |
| `font_metrics/` | Binary font metrics cache, per-directory width calculation |
| `volumes/` | Volume abstraction (local, network, MTP), scanner/watcher traits |
| `stubs/` | Linux compilation stubs for macOS-only modules (used by Docker E2E pipeline) |
| `menu/` | Native menu bar: platform-specific construction, dispatch mapping, accelerator sync, context-aware enable/disable |
| `drag_image_detection.rs` | macOS method swizzle for drag image size detection |
| `drag_image_swap.rs` | Rich/transparent drag image swap for self-drags |
| `commands/` | Tauri command definitions (IPC entry points) |
| `capabilities/` | Per-window Tauri API permissions — must be updated when using new Tauri APIs from a window |
| `icons/` | App icons for all platforms + macOS Tahoe Liquid Glass (Assets.car). See [CLAUDE.md](../apps/desktop/src-tauri/icons/CLAUDE.md) for regeneration steps |

## Other apps

| Directory | Purpose |
|-----------|---------|
| `apps/license-server/` | Cloudflare Worker + Hono. Paddle webhooks, Ed25519 key generation. See [CLAUDE.md](../apps/license-server/CLAUDE.md) (technical reference) and [README](../apps/license-server/README.md) (first-time setup) |
| `apps/website/` | getcmdr.com marketing site (Astro + Tailwind v4). See [README](../apps/website/README.md) and [CLAUDE.md](../apps/website/CLAUDE.md) |
| `scripts/check/` | Go unified check runner (~40 checks, parallel with dependency graph) |

## Cross-cutting patterns

### Data flow: frontend ↔ backend

File data lives in Rust (`LISTING_CACHE`). Frontend fetches visible ranges on-demand via IPC (`getFileRange`).
This avoids serializing 50k+ entries. Virtual scrolling renders only ~50 visible items.

### Navigation lifecycle

User navigates → old listing cleaned up → new listing started → events stream back → UI updates.

**Three navigation types, same cleanup/load sequence:**

| Type | Entry point | Who moves history? | Timing |
|------|------------|--------------------|--------|
| **Enter on folder** | `FilePane.handleNavigate` → `loadDirectory` | `DualPaneExplorer.applyPathChange` pushes history AFTER `listing-complete` | History push on success only |
| **Back/forward** | `DualPaneExplorer.handleNavigationAction` → `setPanePath` → FilePane `$effect` → `loadDirectory` | `updatePaneAfterHistoryNavigation` moves history BEFORE load | Optimistic — if path is gone, error handler resolves upward |
| **Volume switch** | `VolumeBreadcrumb.onVolumeChange` → `FilePane.loadDirectory` + `DualPaneExplorer.handleVolumeChange` | Pushed immediately in `handleVolumeChange` | Optimistic — `determineNavigationPath` may correct to a better path in background |

**Old listing cleanup** (in `FilePane.loadDirectory`, every navigation):

1. `++loadGeneration` — invalidates all in-flight events
2. `cancelListing(oldId)` → sets `AtomicBool` in Rust → background task stops within ~100ms
3. `listDirectoryEnd(oldId)` → stops file watcher, removes from `LISTING_CACHE`
4. Unlisten all 6 event listeners
5. Generate new `listingId` (frontend `crypto.randomUUID()`), subscribe new listeners, call `listDirectoryStart`

**Volume switch specifics**: `handleVolumeChange` saves the old volume's `lastUsedPath` immediately (no debounce),
then runs `determineNavigationPath` in background (each check has 500ms timeout). A `volumeChangeGeneration` counter
guards against stale corrections if the user switches again.

### Listing lifecycle

```
Frontend (FilePane)                    Rust backend (streaming.rs)
   |                                        |
   |-- loadDirectory(path)                  |
   |   listingId = randomUUID()             |
   |   listDirectoryStart(...) ------------>| spawn tokio task → spawn_blocking
   |<-- { listingId, status: Loading }      |
   |                                        |-- emit listing-opening
   |                                        |-- volume.list_directory_with_progress()
   |<-- listing-progress (every 200ms) -----|   (on separate OS thread, polls cancel every 100ms)
   |<-- listing-read-complete --------------|-- sort + enrich + cache insert (atomic with cancel check)
   |                                        |-- start file watcher
   |<-- listing-complete ------------------|
   |   handleListingComplete()              |
   |   onPathChange → push history          |
   |                                        |
   |== ACTIVE: getFileRange on demand =====>|== LISTING_CACHE serves ranges
   |<= directory-diff events ===============|== file watcher detects changes
   |                                        |
   |== CLEANUP (next nav or destroy) =======|
   |-- cancelListing(id) ----------------->|-- AtomicBool → task exits
   |-- listDirectoryEnd(id) -------------->|-- stop watcher, remove from cache
```

Multiple listings coexist (two panes, rapid navigation). Each keyed by unique `listingId` in all global state:
`LISTING_CACHE`, `STREAMING_STATE`, `WATCHER_MANAGER`. Events carry `listingId`; listeners filter by it.

### Concurrency guards

| Guard | Defined in | Incremented by | Checked by | Prevents |
|-------|-----------|---------------|------------|----------|
| `loadGeneration` | `FilePane` (per-instance) | `loadDirectory()`, `adoptListing()` | All 6 listing event handlers; post-`listDirectoryStart` staleness check | Stale listing events from previous navigation applied to current |
| `listingId` match | `FilePane` (per-instance) | Set to new UUID each `loadDirectory()` | Every event handler (belt-and-suspenders with `loadGeneration`) | Wrong listing's events applied to a different navigation |
| `volumeChangeGeneration` | `DualPaneExplorer` (singleton) | `handleVolumeChange()` | `determineNavigationPath` callback | Stale path corrections applied after user switched volumes again |
| `cacheGeneration` | `FilePane` → prop to `FullList`/`BriefList` | `refreshView()`, `adoptListing()`, `directory-diff` handler | Virtual scroll `$effect` via `shouldResetCache()` | Stale frontend scroll cache after sort/filter/watcher changes |
| `AtomicBool` (Rust) | `StreamingListingState` per listing | `cancel_listing()` IPC | Per-entry during read, every 100ms poll, at cache-insert (under write lock) | Cancelled listing inserting into cache or continuing I/O |

### Cancellation patterns

Three architectural patterns used consistently:

1. **`AtomicBool` flag (Rust)**: Every long-running backend task (listing, copy/move/delete, scan, search, indexing,
   AI download) uses an `AtomicBool` checked at iteration boundaries. Stored in global `HashMap` keyed by operation ID.
2. **Generation counter (TypeScript)**: Incremented on each new request. Stale responses silently discarded.
   Lightweight, no backend coordination needed. Used for `loadGeneration` and `currentFetchId` (viewer).
3. **Tauri event feedback**: Backend emits terminal events (`listing-cancelled`, `write-cancelled`, etc.) after
   cancellation completes. For rollback operations, the frontend waits for this event before closing the dialog.

| Operation | User trigger | Frontend | Backend | Coordinated? |
|-----------|-------------|----------|---------|-------------|
| Directory listing | ESC / new navigation | `loadGeneration++` + `cancelListing` IPC | `AtomicBool` per-entry, 100ms poll | Yes |
| File copy/move | Cancel button in dialog | `cancelWriteOperation` IPC | `AtomicBool` per-file; `CopyTransaction.rollback()` if requested | Yes (waits for `write-cancelled` on rollback) |
| File delete/trash | Cancel button in dialog | `cancelWriteOperation` IPC | `AtomicBool` per-file; **no rollback** (already deleted) | Yes, but irreversible |
| Scan preview | Dialog close | `cancelScanPreview` IPC | `AtomicBool` per-entry | Yes |
| File viewer search | New query / ESC / close | `viewerSearchCancel` IPC | `AtomicBool` in search loop | Yes |
| Drive indexing | App shutdown / volume unmount | `stopDriveIndex` IPC | `AtomicBool` on jwalk walker; incomplete scan detected on next startup | Yes |
| AI model download | Cancel button | `cancelAiDownload` IPC | Flag checked per HTTP chunk; partial file kept for resume | Yes |
| Viewer line fetch | Rapid scroll | `currentFetchId++` (discard stale) | None — backend serves all | No, frontend-only |
| Inline rename | ESC / navigate away | `rename.cancel()` resets state | None — purely frontend until submit | No, frontend-only |

**Known gap**: On stuck network mounts, the OS `read_dir` syscall blocks the I/O thread. The `AtomicBool` check runs
between entries, not during the syscall. Mitigation: I/O runs on a separate OS thread; the main task polls via
`mpsc::channel` every 100ms and can respond to cancellation without waiting for the syscall.

### Volume mount/unmount chain

```
Detection:
  macOS: FSEvents on /Volumes (non-recursive)
  Linux: inotify on /proc/mounts + /run/user/<uid>/gvfs/
  MTP:   nusb USB hotplug stream (separate system, own events)

→ State diff against KNOWN_VOLUMES (implicit debounce — multiple FSEvents, one diff)

→ Rust processing:
    Mount:   register LocalPosixVolume with VolumeManager, emit "volume-mounted"
    Unmount: unregister from VolumeManager, emit "volume-unmounted"

→ Frontend listeners (both fire independently):
    VolumeBreadcrumb: clear space cache, reload volume list (dropdown refresh)
    DualPaneExplorer: mount → refresh list; unmount → handleVolumeUnmount()
```

**`handleVolumeUnmount`**: Hard redirect — any pane on the unmounted volume switches to `~` on root volume.
No parent-walking (entire volume is gone). Both panes checked independently. State persisted immediately.

**Safety nets against races**: FilePane's 2-second `dirExistsPollInterval` also detects missing paths. If the
volume root itself is gone, it defers to the unmount handler (avoids double-navigation). If only a subdirectory is
gone but the volume exists, it resolves upward via `resolveValidPath`.

**MTP differs**: Detection via USB hotplug (not filesystem). Three events: `mtp-device-detected`, `mtp-device-removed`,
`mtp-device-connected`. VolumeManager registration happens on explicit `connect()`, not on detection.
SMB shares on macOS appear under `/Volumes` and use the same path as local drives.

### Error recovery

| Scenario | Detection | User sees | Recovery | Cleanup |
|----------|----------|-----------|----------|---------|
| **Path deleted** | `listing-error` + `pathExists` check; watcher `directory-deleted`; 2s poll | Brief spinner → auto-navigates to parent | `resolveValidPath`: walk parents → `~` → `/` (each step 1s frontend + 2s Rust timeout) | `cancelListing` + `listDirectoryEnd` |
| **Permission denied** | Rust `PermissionDenied` → `listing-error` | `PermissionDeniedPane` with OS-specific fix instructions | None (manual fix required) | `listingId` cleared, no cache/watcher |
| **Network slow/dead** | Frontend timeouts (500ms/1s); Rust timeout (2s); ESC cancel | "Opening folder..." → progress → "Press ESC to cancel" | ESC navigates back; timeouts cause graceful fallback | `AtomicBool` cancellation |
| **Mid-stream I/O error** | Rust error through channel → `listing-error` | Spinner → auto-navigates to parent | Same as "path deleted" | No partial cache (listing is atomic — all or nothing) |
| **Volume unmounted** | `volume-unmounted` Tauri event (dedicated handler) | Pane switches to home directory | Hard switch to root volume + `~` | Full pane state overwrite + persist |
| **MTP disconnect** | `mtp-device-removed` event | Falls back to default volume | `handleMtpFatalError` → root volume + `~` | Same as volume unmount |

**Per-entry permission errors** (single unreadable file in a readable dir) don't fail the listing — they appear as
zero-permission entries with fallback metadata.

### Persistence

- **App status** (`app-status.json`): ephemeral state — paths, focused pane, view modes, last-used paths per volume
- **Settings** (`settings.json`): preferences — hidden files, density, date format. Registry-validated.
- **Shortcuts** (`shortcuts.json`): delta-only — only customizations stored, defaults in code
- **License** (`license.json`): activation state, timestamps
- **Window state**: `@tauri-apps/plugin-window-state` for size/position per window label

Philosophy: status is "where you are" (ephemeral), settings are "how you like it" (preferences).

**Persistence timing** (what's at risk on crash):

| State | Timing | Crash loss |
|-------|--------|------------|
| Pane paths, focused pane, view mode, sort | Debounced 200ms (`saveAppStatus`) | Up to 200ms of changes |
| Tab state (paths, sort, viewMode, pinned) | **Immediate** (no debounce) | None — tabs are the reliable source of truth |
| `lastUsedPath` per volume | **Immediate** (no debounce) | None |
| Settings v2 | Debounced 500ms; explicit flush on Settings window close | Up to 500ms if main window crashes |
| Shortcuts | **Immediate** (changes are rare user actions) | None |
| License | **Immediate** (Rust `autoSave`) | None |
| Window size/position | Debounced 500ms on resize; immediate on normal close | Size since last resize settled |

### Platform constraints

Rules that cut across many modules. All existing commands follow these — apply them to new code too.

1. **Tauri IPC threading.** Synchronous `#[tauri::command]` functions block the IPC handler thread.
   If one command hangs (e.g., a filesystem syscall on a dead network mount), ALL subsequent IPC
   calls from the frontend queue behind it and the app appears frozen. All filesystem-touching
   commands are `async` with `blocking_with_timeout` (2s default). When adding new commands that
   touch the filesystem, follow this pattern — see `commands/file_system.rs` for examples.

2. **Network mount blocking syscalls.** `statfs`, `readdir`, `metadata()`, NSURL resource queries,
   and `realpath` can all block indefinitely on slow/hung network mounts (kernel waits 30–120s).
   Every Tauri command that calls these is wrapped in `blocking_with_timeout`. New commands MUST
   do the same. See `docs/specs/blocking-ipc-hardening-plan.md` for the full audit.

3. **Two-layer timeout defense.** Backend: `blocking_with_timeout` (2–15s) wraps syscalls in
   `tokio::time::timeout`. Frontend: `withTimeout` (500ms–3s) races IPC calls and returns a
   fallback on expiry. Both layers are applied for critical paths (volume switching, path
   resolution, volume space queries). Apply both when adding new IPC calls to slow paths.

### macOS specifics

- **Full Disk Access**: checked via `~/Library/Mail` readability (<5ms). Prompt on first launch.
- **Keychain**: stores network credentials and trial state. Uses `security-framework` crate.
- **copyfile(3)**: preserves xattrs, ACLs, resource forks. `COPYFILE_CLONE` for instant APFS clones.
- **ptpcamerad**: auto-claims USB devices. MTP shows workaround dialog with Terminal command.

### Dev mode

- `pnpm dev` at repo root for hot-reloading Tauri app
- AI disabled unless `CMDR_REAL_AI=1` (prevents large downloads)
- License mock via `CMDR_MOCK_LICENSE=commercial`
- MCP server available at `localhost:9224` for agent testing
- `withGlobalTauri: true` in dev mode — security risk if loading remote content

### Checker script

Go-based unified runner (`scripts/check/`). Parallel execution with dependency graph.
Coverage: 70% threshold enforced, `coverage-allowlist.json` exempts Tauri/DOM-dependent files.

## Tooling and infrastructure

Dev workflow docs and external service references. All in `docs/tooling/`.

### Dev workflow

| Doc | Purpose |
|-----|---------|
| [logging.md](tooling/logging.md) | Unified logging, `RUST_LOG` recipes for every subsystem |
| [css-health-checks.md](tooling/css-health-checks.md) | Stylelint + Go-based unused CSS checker |

The check runner and E2E testing docs live colocated with their code:
- Check runner: [`scripts/check/CLAUDE.md`](../scripts/check/CLAUDE.md)
- E2E overview (why three suites, fixtures): [`apps/desktop/test/CLAUDE.md`](../apps/desktop/test/CLAUDE.md)
- Linux E2E (Docker, VNC, Ubuntu VM): [`apps/desktop/test/e2e-linux/CLAUDE.md`](../apps/desktop/test/e2e-linux/CLAUDE.md)
- macOS E2E (CrabNebula): [`apps/desktop/test/e2e-macos/CLAUDE.md`](../apps/desktop/test/e2e-macos/CLAUDE.md)

### External services

| Doc | Purpose |
|-----|---------|
| [hetzner-vps.md](tooling/hetzner-vps.md) | Production VPS: SSH access, layout, deploy commands |
| [umami.md](tooling/umami.md) | Website analytics: API access, DB queries, troubleshooting |
| [cloudflare.md](tooling/cloudflare.md) | DNS, Workers, API token, download tracking (Analytics Engine) |
| [posthog.md](tooling/posthog.md) | Session replay and heatmaps (EU instance), API access |
| [paddle.md](tooling/paddle.md) | Payments API (live + sandbox), common operations |
| [ngrok.md](tooling/ngrok.md) | Tunnels for webhook testing |
| [monitoring.md](tooling/monitoring.md) | UptimeRobot: uptime checks, alerts |

ONLY do read-only operations with these services unless specifically asked to make changes.
