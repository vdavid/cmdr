# Navigation details

Pull-tier docs for `apps/desktop/src/lib/file-explorer/navigation/`: architecture, flows, and decision rationale.
Must-know invariants and gotchas live in [CLAUDE.md](CLAUDE.md).

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and volume selector breadcrumb.

## Key files

- **`navigation-history.ts`**: Purely functional immutable history stack
- **`path-navigation.ts`**: Picks initial path when switching volumes
- **`navigate-and-select.ts`**: Shared "jump into a pane" primitives (`navigateToDirInPane` / `navigateToFileInPane`)
  used by Go-to-path; awaits `navigate()`'s `NavigateResult` (bails on `'refused'`, awaits `settled` otherwise). Plus
  the pane-reuse variants the downloads flow uses (`revealFileInBestPane` / `navigateToDirInBestPane`): reuse a pane
  already showing the target dir (volume-safe, active tab only) instead of duplicating the view; the plain primitives
  keep their always-navigate-the-given-pane contract for Go-to-path
- **`path-resolution.ts`**: Walk-up `resolveValidPath` (split out to break cycle)
- **`path-segments.ts`**: Splits the breadcrumb display path into segments and flags any inside a `.git/...` portal
  (consumer: `FilePane.svelte` paints them with `--color-git-portal-text`)
- **`keyboard-shortcuts.ts`**: Home/End/PageUp/PageDown handling for file lists
- **`VolumeBreadcrumb.svelte`**: Clickable volume label + grouped dropdown
- **`volume-grouping.ts`**: Pure logic: group volumes by category, get volume icons
- **`volume-space-manager.svelte.ts`**: Reactive state machine for disk space fetch/retry/timeout
- **`volume-breadcrumb-handlers.svelte.ts`**: Submenu/breadcrumb-popup controllers, keyboard-mode tracker, and pure
  key-dispatch helpers for `VolumeBreadcrumb.svelte`
- **`favorites-controller.svelte.ts`**: `createFavoritesController(deps)`: the favorites INTERACTION layer (rename
  start/cancel/commit + key guard, pointer-drag + keyboard reorder, remove, the local-first `optimisticFavoriteIds`
  override and its reconciliation `$effect`) extracted from `VolumeBreadcrumb.svelte`. Owns the rename/drag `$state`;
  exposes it via getters and exposes the handlers as methods plus a `destroy()` (window drag listeners).
  `effectiveVolumes` / `favorites` stay in the component and read `fav.optimisticFavoriteIds`
- **`eject-predicate.ts`**: Pure `isVolumeEjectable(volume)` used by the eject button gate. Returns true when NSURL says
  ejectable OR the volume has any SMB connection state
- **`DriveIndexBadge.svelte`**: Per-drive index freshness dot (gray/blue/green/yellow) + its click menu (see § Drive
  index freshness badge)
- **`ImageIndexDriveBadge.svelte`** + **`image-index-drive-state.ts`**: the second per-drive dot, for IMAGE-search
  indexing (gray/yellow/green), and its pure state/coverage mapping (`image-index-drive-state.test.ts`). See §
  Image-index drive dot
- **`drive-index-status.ts`**: Pure mapping for the badge: `VolumeIndexStatus` → state/color, menu items per state, the
  "N min, S s" duration formatter (`drive-index-status.test.ts`)
- **`drive-index-manager.svelte.ts`**: Reactive `volumeId → VolumeIndexStatus` map; fetches on demand and subscribes to
  the indexing events to stay live. `isDriveRow(volume)` is the badge-eligibility predicate
- **`navigation-history.test.ts`**: Full unit test coverage of history functions
- **`path-navigation.test.ts`**: Unit tests for path resolution and timeouts
- **`keyboard-shortcuts.test.ts`**: Unit tests for shortcut calculations
- **`path-segments.test.ts`**: Unit tests for git-portal segment detection

## `navigation-history.ts`

Purely functional: all operations return new objects, never mutate.

```
NavigationHistory = { stack: HistoryEntry[], currentIndex: number }
HistoryEntry = { volumeId: string, path: string, networkHost?: NetworkHost }
PushResult = { history: NavigationHistory, droppedEntries: HistoryEntry[] }
```

Key functions: `createHistory`, `push`, `pushPath`, `back`, `forward`, `getCurrentEntry`, `getCurrentPath`, `canGoBack`,
`canGoForward`, `setCurrentIndex`, `getEntryAt`. Plus the constant `MAX_HISTORY_PER_TAB = 100`.

`push` returns `{ history, droppedEntries }`. `history` is the new stack; `droppedEntries` aggregates every entry the
push removed: the truncated-forward tail (when pushing after `back()`) and the oldest entries evicted to honor
`MAX_HISTORY_PER_TAB`. Callers that need to release per-entry resources iterate `droppedEntries`; the search-results
snapshot store (`lib/search/snapshot-store.svelte.ts`) is the only consumer today. When the new entry equals the current
entry, `push()` returns the **same `history` reference** with an empty `droppedEntries`, so callers using `===`
deduplication still work.

`pushPath` is a thin delegate that calls `push` and returns just the new history (discarding `droppedEntries`). It's
backwards-compatible for callers that don't care about released resources. Callers that need refcount-decrements (the
tab-state manager) must use `push()` directly — or, more conveniently, the `pushHistoryEntry` helper exposed by
`lib/file-explorer/tabs/tab-state-manager.svelte.ts`, which wraps the `push()` call and releases search-results snapshot
refs in one step.

The cap (100) applies to every volume — local, network, MTP, search-results — uniformly. Tightening below 100 would
start to hurt power users who navigate deeply and rely on `⌘[` for orientation. Bumping above 100 isn't necessary; each
`HistoryEntry` is three string fields, so the memory headroom is comfortable.

Entries carry full `volumeId` (navigating back can cross volume boundaries, for example from an external drive back to
`root`).

### Gotcha: history is pushed on both listing success AND listing failure

`FilePane.svelte`'s `onPathChange?.(loadPath)` callback is the canonical place where a navigation lands in history. It
fires from two branches: `handleListingComplete` (success) AND the `listing-error` handler when the path still exists
(error pane will render). Without the second branch, navigating to a folder that fails to list (TCC-restricted, mode
0700, etc.) would show the `ErrorPane` while leaving the path absent from history; `Cmd+[` would then visually jump back
two steps because the current pane state isn't in the stack. The `listing-error` handler with the auto-fallback (path
deleted → navigate to parent) doesn't push via this callback; it relies on the fallback navigation's own
`commitPathFromListing` push (the in-place `history: 'push-path'` commit in `pane/navigate.ts`).

## `path-navigation.ts`

`determineNavigationPath(volumeId, volumePath, targetPath, otherPane)`: picks best initial path when switching volumes.
Runs checks **in parallel** with 500ms frontend timeouts per check. Priority:

1. Favorite path (when `targetPath !== volumePath`)
2. Other pane's path (if same volume and path exists)
3. Stored `lastUsedPath` for this volume
4. Default: `~` for `DEFAULT_VOLUME_ID`, else volume root

`withTimeout(promise, ms, fallback)`: imported from `$lib/utils/timing` and re-exported. Races a promise against a
timeout, returning the fallback on expiry. Used by `determineNavigationPath` and also by `VolumeBreadcrumb.svelte`
(wraps `getVolumeSpace`). `DualPaneExplorer.svelte` uses `resolvePathVolume` for startup tab restore (backend has its
own 2s timeout, no frontend wrapper needed).

## `path-resolution.ts`

`resolveValidPath(targetPath, options?)`: walks parent tree until an existing directory is found. Accepts optional
`{ pathExistsFn, timeoutMs }`: defaults to Tauri `pathExists` with 1s timeout per step. Used both at runtime (with
timeouts) and at startup via `app-status-store.ts`'s `resolvePersistedPath` wrapper (no timeout, injected
`pathExistsFn`). Fallback chain: parent dirs → `~` → `/` → `null` (volume unmounted).

Lives in its own module so `app-status-store.ts` can import it without forming a cycle; `path-navigation.ts` itself
imports `getLastUsedPathForVolume` from `app-status-store.ts`.

### Non-blocking navigation pattern

All `pathExists` calls are guarded by two timeout layers:

- **Rust-side**: `blocking_with_timeout` wraps filesystem syscalls in `tokio::time::timeout` (2 seconds). Prevents
  kernel syscalls on hung network mounts from blocking the Tauri async runtime.
- **Frontend-side**: `withTimeout` races each `pathExists` IPC call (500ms for `determineNavigationPath`, 1s for
  `resolveValidPath`). The faster timeout wins.

`navigate()`'s volume-switch arm (in `pane/navigate.ts`) uses **optimistic navigation**: `commitVolumeSwitch` commits
the new volumeId + path + history synchronously (showing the loading spinner), then `scheduleVolumePathCorrection`
resolves the "best" path in the background via `determineNavigationPath`. A single GLOBAL `correctionGen` counter (the
caller-owned holder in `NavigateDeps`, shared by both panes) guards against stale corrections: a later volume change on
either pane bumps it, so a pending correction whose generation was superseded is dropped.

`handleCancelLoading` (`DualPaneExplorer.svelte`) folds onto `navigate()`: on ESC it walks history back via
`navigate({ to: { history: 'back' }, source: 'cancel' })`, or for a tab with no history resolves the nearest valid
parent and routes a terminal `source: 'fallback'` commit. Back/forward go through `navigate({ to: { history } })`;
parent (`{ history: 'parent' }`) delegates to `FilePane.navigateToParent`, whose `onPathChange` re-enters
`commitPathFromListing`. FilePane's listing-error handler resolves upward if the path is gone.

## `keyboard-shortcuts.ts`

`handleNavigationShortcut(event, context): NavigationResult | null`

`NavigationContext` fields:

- `currentIndex`, `totalCount`: always required
- `itemsPerColumn`, `visibleColumns`: Brief mode only; presence enables Brief branch
- `visibleItems`: Full mode PageUp/PageDown page size

Handled keys:

| Key               | Brief mode                                                     | Full mode              |
| ----------------- | -------------------------------------------------------------- | ---------------------- |
| Option+Up / Home  | First item                                                     | First item             |
| Option+Down / End | Last item                                                      | Last item              |
| PageUp            | Bottom of column (visibleColumns-1) to the left, or first item | Up by visibleItems-1   |
| PageDown          | Bottom of column (visibleColumns-1) to the right, or last item | Down by visibleItems-1 |

Meta+Home/End is intentionally not handled (passes to OS). Returns `null` for unhandled keys.

Brief PageUp/PageDown lands on the **bottom row** of the target column (TUI convention).

`NavigationResult` also carries an `overflow: boolean` field: `true` when the requested step was clamped at a list
boundary (intended distance > actual distance). Home/End are always overflow. PageUp/PageDown are overflow when the page
step would cross 0 or `totalCount - 1`. Callers wiring keyboard Shift+nav use this to decide whether to include the
landing item in the toggle-and-fill range — see `file-explorer/CLAUDE.md` § Selection.

## `VolumeBreadcrumb.svelte`

Pure presentational component. Reads the volume list from the shared `volume-store.svelte.ts` (no fetching, no event
listeners for volume changes). Volume grouping logic and disk-space retry state are extracted into `volume-grouping.ts`
and `volume-space-manager.svelte.ts` respectively.

Props: `volumeId`, `currentPath`, `onVolumeChange?`.

`containingVolumeId` is derived separately via `resolvePathVolume(currentPath)`: the active checkmark tracks the real
containing volume, not the `volumeId` prop (which may be a favorite's virtual ID).

Keyboard/mouse mode: entering keyboard nav sets `isKeyboardMode = true`, suppressing CSS `:hover` highlights. Mouse
movement > 5px threshold exits keyboard mode.

Volumes (including MTP) come from the shared `volume-store` which is pushed by the backend via a single
`volumes-changed` event. MTP volume space is fetched via `getVolumeSpace()` like any other volume.

Exported methods for parent components: `toggle()`, `open()`, `close()`, `getIsOpen()`, `handleKeyDown(e)`.

### Restricted-folder indicator (TCC)

Sidebar entries whose path is in the runtime "TCC-restricted" set carry an italic + opacity-0.6 label plus a Lucide
`info` icon. The hover tooltip points the user at both Full Disk Access and the per-folder Files & Folders pane in
System Settings. State is owned by `crate::restricted_paths` in the backend and exposed via
`$lib/stores/restricted-paths-store.svelte` (`isRestricted(path)`). The backend records `PermissionDenied` on paths
matching a hard-coded "possibly TCC-restricted on macOS" list (Downloads/Documents/Desktop/Pictures/Movies/Music,
`~/Library/Safari/Mail/Messages`, iCloud Drive, `~/Library/CloudStorage`, Containers, network volumes) and re-probes
every entry whenever the app regains focus (NSApplicationDidBecomeActive observer), which is how the styling clears
without polling after the user grants permission in System Settings. The same `tcc_paths::is_potentially_tcc_restricted`
predicate also drives the dedicated "This folder is restricted by macOS" `FriendlyError` shown in `ErrorPane` for
permission-denied listings on those paths.

### SMB connection indicator

SMB volumes with an active `SmbVolume` in the backend carry `smbConnectionState: 'direct' | 'os_mount'`. The component
renders a small colored circle (green = direct smb2 session, yellow = OS mount fallback) both in the dropdown and in the
closed breadcrumb label. Yellow state has a submenu trigger in the dropdown and a clickable button (circle + down arrow)
in the breadcrumb, both opening a "Connect directly for faster access" menu item. Clicking it calls `upgradeToSmbVolume`
which tries stored credentials first; if none found or they fail, the backend returns `credentialsNeeded`. Before
showing the login form, `tryUseSavedPassword` runs a prompt-free probe (`systemHasSavedSmbPassword`): if macOS/Finder
already saved a password for this share, it offers to reuse it via a native primer dialog ("Use the saved password?" —
the cushion before the system Keychain consent dialog, whose own text we can't customize). On "Use saved password" it
calls `upgradeToSmbVolumeUsingSavedPassword` (consent read → direct smb2 → copied into Cmdr's store); on "Enter it
instead", a denied consent, or no saved password, it falls through to the `onSmbUpgradeLogin` callback that triggers
`FilePane` to show `NetworkLoginForm` inline (same pattern as `ShareBrowser`). Submenu supports full keyboard navigation
(ArrowRight to open, ArrowLeft/Escape to close, Enter to activate).

### Eject button + row context menu

Ejectable volumes (USB, SD, DMG, MTP, SMB — see `eject-predicate.ts`) show a small `⏏`-shaped icon button on the right
of each dropdown row and on the right of the closed/header chip. Clicking it calls `ejectVolume(id)` which dispatches in
the backend: SMB → `diskutil unmount`, MTP → connection manager disconnect, physical / DMG → `diskutil eject`. Clicking
the inline button does NOT close the dropdown (`handleEjectClick` no longer flips `isOpen`), so the user can eject
several drives in a row; each ejected volume vanishes from the list via the existing `volume-unmounted` /
`mtp-device-disconnected` flow — no extra success toast.

Right-clicking a dropdown row opens a NATIVE (muda) context menu via `show_volume_row_context_menu`: a favorite row gets
`Rename` + `Remove`, an ejectable volume row gets `Eject ({name})`, anything else has no menu. Right-clicking the closed
header opens the native breadcrumb menu (`show_breadcrumb_context_menu`) that adds `Eject ({name})` alongside "Copy
path" when the pane's volume is ejectable. All these picks route back through the one `volume-context-action` Tauri
event (`action` ∈ `eject` / `rename-favorite` / `remove-favorite`): `eject` is handled in `DualPaneExplorer.svelte`
(calls `ejectVolume`); `rename-favorite` / `remove-favorite` land in `VolumeBreadcrumb.handleVolumeContextAction`, which
only acts when its own dropdown `isOpen` (both panes' breadcrumbs receive the global event, but only the open one owns
the menu it spawned). Going native means the webview is frozen while the menu tracks, so the dropdown's
`highlightedIndex` can't drift onto another row under the cursor or arrow keys — the menu always acts on the
right-clicked row.

**Busy gating.** While a copy / move / delete reads from or writes to a volume, ejecting it is blocked so a disconnect
can't truncate an in-flight file. `$lib/stores/volume-busy-store.svelte`'s `isVolumeBusy(id)` (fed by the backend
`volumes-busy-changed` event) disables the header eject button and the dropdown-row eject button, each with a "Can't
eject while operations are in progress on this device" tooltip; `handleEjectClick` also early-returns on a busy volume.
The native row / breadcrumb eject items are gated backend-side: `show_volume_row_context_menu` /
`show_breadcrumb_context_menu` pass the volume ID, and the Rust builder renders the `Eject` item disabled with a
` (busy)` suffix. The real safety net is the `eject_volume` backend guard, which refuses a busy volume even if the UI is
stale or an MCP caller bypasses it. See `src-tauri/src/file_system/write_operations/CLAUDE.md` § "Busy-volumes set".

### Editable favorites

The "Favorites" group in the switcher is user-owned: add, remove, rename, reorder. Favorites arrive from `volume-store`
as `VolumeInfo` with `category: 'favorite'` and `id: 'fav-<favoriteId>'` (the backend `favorites/` store is the source
of truth; see `src-tauri/src/favorites/CLAUDE.md`). All mutations go through the typed `commands.*` wrappers in
`$lib/tauri-commands/favorites.ts`; each re-emits `volumes-changed`, so the switcher re-renders live with no manual
refresh. `stripFavoritePrefix(locationId)` recovers the bare favorite id (the remove / rename / reorder commands take
the bare id, not the `fav-…` switcher id).

- **Add** has three surfaces: the `favorites.add` command (palette + the Go menu's "Add to favorites", with NO default
  shortcut since adding a favorite is infrequent; it's assignable in Settings > Keyboard shortcuts, and the Go-menu
  item's accelerator syncs to whatever the user binds) favorites the focused pane's current dir (handler in
  `routes/(main)/command-handlers/misc-handlers.ts`); the folder-row and `..` context menus favorite a SPECIFIC path.
  The context-menu add is handled entirely in Rust (`menu/menu_handlers.rs` intercepts `FAVORITES_ADD_CONTEXT_ID` and
  favorites `MenuState.context.path`), so it never routes through `favorites.add` (which would favorite the wrong dir).
  The folder-row item lives in `build_context_menu` (directories only, not on search-results panes); the `..` row gets
  its own one-item menu via `show_parent_row_context_menu` (`FilePane.handleContextMenu` calls it with the parent dir
  path). The favorites INTERACTION layer (rename, pointer-drag + keyboard reorder, remove, and the local-first
  optimistic-order override + its reconciliation `$effect`) lives in `favorites-controller.svelte.ts`
  (`createFavoritesController(deps)`), instantiated as `fav` at the top of `VolumeBreadcrumb.svelte`'s script. The
  component keeps the template, the shared `highlightedIndex`, and the `effectiveVolumes` / `favorites` deriveds (which
  read `fav.optimisticFavoriteIds`); it calls `fav.*` for everything below. The deps in are getters/callbacks
  (`getFavorites`, `getVolumes`, `getDropdownRef`, `getRenameInputRef`, `navigate`); the controller imports
  `reorderFavorites` / `removeFavorite` / `renameFavorite` / `addToast` / the `favorites-reorder.ts` helpers directly.
  Pinned by `favorites-controller.svelte.test.ts` (the pointer-drag / rename / remove unit tests) plus the
  component-level `VolumeBreadcrumb.svelte.test.ts`.

- **Remove / Rename** are per-item. Right-clicking a favorite opens the NATIVE row menu (`show_volume_row_context_menu`,
  see § Eject button + row context menu); picking `Rename` / `Remove` routes back over `volume-context-action` to
  `VolumeBreadcrumb.handleVolumeContextAction`, which calls `fav.startRename` / `fav.remove` on the open dropdown.
  Rename swaps the label for an inline `<input>` (Enter commits, Escape/blur cancels). Both strip the `fav-` prefix
  before calling the command. `fav.handleRenameKeyDown` calls `e.stopPropagation()` on EVERY key: the focused input owns
  its keystrokes, and the pane's Space-selection / type-to-jump DOM listeners aren't covered by the dispatch-level
  guard, so a leaked Space would select the file under the cursor while the user types into the box. Enter commits,
  Escape cancels, everything else edits the text. While a rename is active, `VolumeBreadcrumb.handleKeyDown` also bails
  (`fav.renamingFavoriteId !== null`) so the dropdown's list-nav keys (arrows / Home / End) don't steal them. The
  broader keystroke-leak guard lives one level up: while ANY pane's switcher dropdown is open,
  `DualPaneExplorer.routeToVolumeChooser` swallows the key from the pane behind it (returns true even when the dropdown
  ignores the key), and `+page.svelte`'s `isModalDialogOpen()` reads `explorerRef.isVolumeChooserOpen()` to suppress
  centralized webview-keydown dispatch.
- **Reorder** is pointer-drag within the section AND keyboard (Option+Up / Option+Down, since the app is keyboard-first;
  the row tooltip reads `⌥↑ / ⌥↓` on macOS, `Alt+↑ / Alt+↓` elsewhere, built by the pure `favorite-tooltip.ts`).
  - **Pointer drag** uses `onmousedown` on the row + `window` `mousemove`/`mouseup` listeners (armed on mousedown,
    removed on mouseup and on `fav.destroy()`, called from the component's `onDestroy`), NOT HTML5 drag-and-drop. A
    reorder begins only once the pointer moves past a small threshold (`DRAG_THRESHOLD_PX`); below it, a mouseup is a
    plain click that navigates. So favorite rows skip the `onclick` navigate path (it would double-fire with the
    mouseup) and the controller's `navigate` dep (the component's `handleVolumeSelect`) runs from mouseup instead.
    During the drag, `favoriteRowMidpoints()` feeds two pure helpers from `favorites-reorder.ts`: the CUE uses the raw
    `pointerInsertionSlot()` (the visual gap, `0..length`) so the drop-line sits at the right gap — `is-drag-over` (top
    border) for an in-list gap, `is-drag-over-end` (bottom border on the last row) for dropping past the end — while the
    DROP uses `pointerReorderTarget()` (that slot adjusted for the grabbed item being removed first). Driving the cue
    off the move-target instead put the line one row too high on downward drags. The grabbed row carries `is-dragging`.
  - **Local-first / optimistic:** both reorder paths persist through the controller, which sets its
    `optimisticFavoriteIds` override (that the component's `effectiveVolumes` / `favorites` derive from via
    `fav.optimisticFavoriteIds`) SYNCHRONOUSLY, then persists via `reorderFavorites` in the background. The list
    re-renders instantly, and a rapid second Alt+↑/↓ computes against the fresh order instead of racing the
    `volumes-changed` round-trip (which would move the wrong item). A reconciliation `$effect` clears the override once
    the store catches up (or the favorite set changes elsewhere); a failed persist drops it, reverting to the store
    truth with a toast. Don't make the reorder await the IPC before updating the UI.
  - **Why pointer and not HTML5 DnD:** under Tauri's `dragDropEnabled` (on by default), macOS intercepts drag gestures
    at the OS layer before the WKWebView sees `dragstart`/`dragover`/`drop`, so an HTML5-`draggable` reorder silently
    never fires (the events don't arrive). This is the same reason the native file-list drag (`views/FullList.svelte`)
    is `onmousedown`-based, not `draggable`. Don't reintroduce HTML5 drag here; it'll look wired-up and do nothing.
    Synthetic MCP/test events bypass the OS interception, so "it works under MCP" is not proof it works with a real
    mouse.
  - **Keyboard** (Alt+↑ / Alt+↓) is handled in the exported `handleKeyDown`, BEFORE `handleDropdownKey` consumes the
    bare arrows, and acts on the highlighted favorite (`allVolumes[highlightedIndex]`) since the rows aren't DOM-focused
    (the dropdown navigates by a virtual `highlightedIndex`). It calls `fav.reorderHighlighted(volume, delta)`, which
    returns the favorite's new index (or null at an edge); the component then sets `highlightedIndex` to it so repeated
    Alt+↓ keeps moving the same item. `highlightedIndex` stays in the component (the dropdown's general nav uses it
    too); the controller never touches it.
  - Both paths compute the new order with the pure `favorites-reorder.ts` helpers (`moveItem`, `clampedReorderTarget`,
    `pointerReorderTarget`) and persist the FULL order via `reorderFavorites(bareIds)`. The favorite row's tooltip leads
    with the PATH (then the reorder hint) so a renamed favorite still reveals where it points.
- **Empty state** is a real state (the user can remove every favorite). The `favorite` group in `volume-grouping.ts`
  always renders (unlike every other group, which hides when empty), and the switcher shows a single disabled,
  non-focusable placeholder row: "(Your favorites will show here)".

### USB link-speed indicator (MTP)

MTP volumes carry `usbSpeed: UsbSpeed` (`'low' | 'full' | 'high' | 'super' | 'super_plus'`) sourced from `mtp-rs` via
the shared `crate::usb_speed::UsbSpeed`. `describeUsbSpeed(speed)` in `$lib/file-explorer/types` maps each tier to
`{ tier, label, maxMBps }`; the breadcrumb renders a 5-tier rainbow dot (red → orange → yellow → light green → dark
green; dark green is `--color-allow`, same shade as SMB direct) on the right of both the closed chip and each dropdown
row. Tooltip shows `<label> (Max. <N> MB/s)\nNegotiated for this cable, port, and device` (the global tooltip CSS uses
`white-space: pre-line`, so `\n` becomes a real line break). The dot is the only visual — no inline text in the chip and
no extra line under the disk-space bar, by design.

### Drive index freshness badge

Each real drive carries a small index-freshness dot (`DriveIndexBadge.svelte`) in TWO placements: always-visible next to
the dropdown trigger (reflecting the ACTIVE drive), and per-row inside the dropdown. Both reuse the same colored-dot +
`use:tooltip` shape as the SMB light and USB-speed ring. The four states map from the backend `VolumeIndexStatus`
(`commands.getVolumeIndexStatusById`): gray = `disabled` (no live index, `enabled: false` or `freshness: null`), blue =
`scanning`, green = `fresh`, yellow = `stale`. The mapping, the menu items per state, and the "N min, S s" duration
formatter are the pure `drive-index-status.ts` (unit-tested). Blue pulses (gated behind `prefers-reduced-motion`).

- **Eligibility is `isDriveRow(volume)`** (in `drive-index-manager.svelte.ts`): every entry except favorites, the
  synthetic `network` / `search-results` ids, and mounted disk images (`isDiskImage`). SMB shares
  (`category: 'network'`, real id) and the local disk (`root`) DO get a badge; the synthetic "Network" group entry and
  `.dmg` mounts do not. Disk images are excluded here because they're transient install-style mounts we deliberately
  never index — and since this predicate also gates the first-connect prompt and the status fetch, one exclusion covers
  all three. The badge is gray for any drive without a registered index, so it's safe to query for every eligible row.
- **Status stays live by SUBSCRIPTION, not polling** (`drive-index-manager.svelte.ts`): it listens to
  `index-freshness-changed`, `index-scan-started`, and `index-scan-complete`, refetching the named volume's status on
  each (the events alone don't carry the last-scan facts). The active-drive badge also refetches when the active drive
  changes; dropdown rows refetch on open.
- **A scanning badge shows the SHARED live status body**, not a bespoke string — the same `IndexingDriveRow` body the
  corner indicator renders (heading off), so the two surfaces match exactly (count + elapsed for a first scan, or
  bar+percent+ETA for a calibrated rescan; see `$lib/indexing` DETAILS § Status indicator tooltip content). The badge
  reads ONLY its own volume's live activity from `index-state` via `getVolumeActivity(volumeId)` (+
  `getVolumeAggregation`) — `index-state` is the single live-activity source; the manager carries no progress map. For
  the `scanning` state the tooltip switches from the text variant to the `contentEl` DOM tooltip: the body lives in a
  `<div hidden>` host and the INNER element is handed to the tooltip as `contentEl` (an adopted element keeps its own
  `hidden`, so the host can't be passed — mirrors `IndexingStatusIndicator`). Fallback: for a non-root (SMB/MTP) volume,
  `index-state` only hydrates on the next ~500 ms progress tick, so in the window between the freshness flip to
  `scanning` and that tick there's no activity — the badge then shows a static "Scanning your drive…" text tooltip
  (`indexing.scan.label`), never an empty one. Non-scanning states (disabled/fresh/stale) keep their text tooltips.
- **The badge is a focusable `<button>`** with an `aria-label` (state ariaLabel + the tooltip text) and
  `aria-haspopup="menu"`; clicking opens a small themed popover menu (NOT a native menu) anchored to the badge. Menu
  actions (`enable`/`rescan`/`disable`/`stop`) call back to `VolumeBreadcrumb`'s `handleDriveIndexAction`, which runs
  the per-drive IPC. ❌ Don't put `role="img"` on the button (axe rejects it; the button role + label already convey
  it).
- **Coalesced "macOS lost track of changes" signals ride in the TOOLTIP, never in the dot's color**
  (`driveIndexCoalescedNote`, pure + unit-tested). When `VolumeIndexStatus.coalescedSignalsSinceSweep > 0`, a second
  paragraph joins the state line (the tooltip is `white-space: pre-line`, so a `\n` renders) saying how many times macOS
  lost track, over how many hours, and when the next full check lands. The badge deliberately stays GREEN: once-a-day
  sweeping is the designed operating state, and a badge that's yellow all day trains people to ignore it. Four
  deliberate silences: count 0; any state but `fresh`/`stale` (while scanning the sweep may be the scan in flight;
  disabled/failed have no live index to describe); no `scanCompletedAt` (nothing anchors the time window); and no
  `nextSweepDueAt` or a sweep already due, which swaps in the `…NoNextCheck` variant that drops the "next full check in
  N hours" clause. `nextSweepDueAt` is null for every volume WITHOUT a daily sweep (an external drive runs a 45-second
  debounce, which promises nothing), so never render a zero there — it would be a lie about a USB drive. Both hour spans
  round UP with a floor of one, so the tooltip never reads "in the last 0 hours" and the window it names always covers
  what happened. Hours come from `scanCompletedAt` (the FE's only honest last-full-check anchor); don't reconstruct them
  from `nextSweepDueAt` minus the window, that would duplicate the backend's policy constant.
- **Refused enable/rescan is classified by TYPED variant** (`SmbIndexGateReason`), never message text:
  `credentials_needed` routes into the existing direct-connect/login flow (`handleSubmenuAction`); the others show a
  friendly toast.
- **The dropdown-row menu can be clipped by the dropdown's `overflow-y: auto`** (unlike the breadcrumb placement). The
  breadcrumb badge is the primary surface (D3) and isn't clipped; the row menu is a convenience. If this becomes a
  problem, switch the row menu to `position: fixed` from `getBoundingClientRect()` like the connection submenu.

### Image-index drive dot

A SECOND small dot (`ImageIndexDriveBadge.svelte`) sits immediately after the filesystem `DriveIndexBadge` at both
placements (active-drive breadcrumb + each dropdown drive row), reporting IMAGE-search indexing per drive. Three states,
mapped by the pure `image-index-drive-state.ts::imageIndexDriveState` (unit-tested): gray `off` (image search disabled,
or the volume isn't image-indexed), pulsing yellow `indexing` (a pass is actively enriching OR the covered set isn't
fully enriched), green `done` (idle and every covered image enriched). The dot reuses the freshness dot's 10px shape +
pulse.

- **State inputs**: `getMediaIndexEnabled()` (master toggle, reactive), the per-volume `mediaIndexVolumeState`
  (`enabled` + the covered/enriched counts), and `getVolumeEnrichActivity(volumeId)` (live, reactive; a PAUSED pass
  reads `indexing`, not `done`). The tooltip's `done / total` comes from `imageIndexDriveCoverage`: total is
  `coveredQualifyingCount ?? qualifyingCount` (the in-scope denominator, so a narrow scope can still reach `done`), done
  is `enrichedCount` clamped to total. It hides itself entirely when `qualifyingCount` is `0` or `null`, so drives with
  no images (or not scored yet) stay clean.
- **Data + refresh**: `VolumeBreadcrumb` owns a `SvelteMap<volumeId, MediaIndexVolumeState>` filled by
  `mediaIndexVolumeState` — the active drive on change, dropdown rows once on open — and refetched on that volume's
  `media-enrich-progress` / `media-enrich-terminal` (only for volumes already in the map, so off-screen drives aren't
  tracked). Bounded to the shown drives, so no poll. Enrich listeners are registered in `onMount`, cleaned in
  `onDestroy`.
- **Non-interactive**: unlike the freshness badge (a `<button>` with a menu), this is a focusable `role="img"` span with
  an `aria-label` + `use:tooltip` (the sanctioned status-glyph pattern, mirroring the corner indicator). There's no
  menu: the image-search on/off + scope controls live in Settings.

### Dropdown and submenu UI patterns

These patterns emerged during the volume picker implementation and should be followed in future dropdown/submenu work:

- **CSS triangles for arrows/chevrons**, not font characters. Font-based arrows (`▾`, `›`) render at inconsistent sizes
  across fonts and OS versions. Use the CSS border trick
  (`border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 5px solid currentcolor`) for
  pixel-perfect control.
- **Single cursor rule.** When a submenu opens, suppress the main menu highlight. Exactly one cursor should be visible
  at all times. Use a state flag (like `submenuVolumeId`) to conditionally remove the `is-focused-and-under-cursor`
  class from the main menu.
- **Elements with independent actions must be outside their parent's click area.** If a button inside another button has
  a different action (like "Volume options" inside "Volume selector"), it must be a sibling, not a child. Otherwise
  `stopPropagation` fights with the parent's click handler.
- **Fixed positioning for submenus inside scrollable containers.** A submenu inside a `overflow-y: auto` dropdown gets
  clipped. Use `position: fixed` with coordinates calculated from `getBoundingClientRect()` of the trigger element.
- **Tooltip dismissal.** Pass empty string to the `use:tooltip` directive when the element's popup is open. The
  directive's `update` handler calls `hideTooltip()` automatically.
- **macOS-native menu feel.** Submenu overlaps the parent slightly (~5px). Hovering the row opens the submenu (not just
  the arrow). Submenu highlight appears only on direct interaction (mouse hover on the item, or keyboard navigation),
  not automatically when the submenu opens via row hover.

## `volume-grouping.ts`

Logic for organizing volumes into display groups. No own reactive state, but group LABELS and the synthetic Network
entry's NAME are resolved from the message catalog
(`tString('fileExplorer.navigation.group*' / '.networkVolume[Disabled]')`), which reads the active locale.
`VolumeBreadcrumb` calls `groupByCategory` from a `$derived`, so the labels track a locale switch. To change a section
heading or the Network entry's name, edit the catalog, not this file.

`groupByCategory(vols)`: groups volumes by category in display order:

1. Favorites: no checkmark shown even if current path is a favorite
2. main_volume + attached_volume: merged into one group
3. Cloud drives
4. Mobile (MTP) devices: filtered from unified volume list (`category === 'mobile_device'`)
5. Network: always includes a synthetic `'network'` entry (`smb://`) plus any mounted SMB shares. The synthetic entry's
   name flips from the `networkVolume` catalog key to `networkVolumeDisabled` ("Network (disabled)") when
   `options.networkEnabled === false`. `VolumeBreadcrumb` reads `getNetworkEnabled()` from reactive settings to set the
   option, and intercepts clicks on the disabled entry to open Settings → File systems → SMB/Network shares (via
   `openSettingsWindow(['File systems', 'SMB/Network shares'])`) instead of navigating.

`getIconForVolume(volume)`: returns the appropriate icon path for a volume based on its category.

## `volume-space-manager.svelte.ts`

Reactive state machine for fetching, retrying, and caching disk space info per volume. Created via
`createVolumeSpaceManager()` (functional factory, no classes).

`getVolumeSpace()` returns `TimedOut<T>` wrappers. The manager tracks timeout state and exposes reactive sets for the
component to render inline indicators (no toasts):

- **Volume space timeout** (`spaceTimedOutSet`): Three-state cycle with per-volume tracking:
  - **Idle**: Dashed-outline placeholder bar with "?" icon, "Unavailable" text, tooltip "Couldn't fetch disk space --
    click to retry". After a retry has been attempted, tooltip changes to "Still unavailable -- click to retry".
  - **Retrying** (`spaceRetryingSet`): Spinner replaces "?", text shows "Retrying", tooltip "Retrying..." (manual) or
    "Retrying automatically..." (auto). Clicks are debounced (ignored while in-flight).
  - **Failed**: Brief shake animation (300ms), then returns to idle with "Still unavailable" tooltip.
  - **Auto-retry**: 5s after initial timeout, an automatic retry fires with full visual feedback (spinner + shake on
    failure). Tracked via `spaceAutoRetryingSet` for tooltip distinction.
  - All retry sets are cleared via `clearAll()` on volume mount/unmount events. Auto-retry timers are cleaned up via
    `destroy()`.
  - Reduced motion: spinner degrades to pulsing opacity, shake degrades to opacity flash.
- **Volume list timeout** (`volumesTimedOut`): Tracked in `volume-store.svelte.ts` (not in the manager). The component
  reads it via `getVolumesTimedOut()` and shows a warning row with a retry button at the bottom of the dropdown.

## Dependencies

- `$lib/stores/volume-store.svelte`: `getVolumes` (backend-pushed reactive volume list)
- `$lib/tauri-commands`: `resolvePathVolume`, `pathExists`
- `$lib/utils/timing`: `withTimeout` (defense-in-depth IPC timeout wrapper)
- `$lib/app-status-store`: `getLastUsedPathForVolume`
- `../types`: `VolumeInfo`, `LocationCategory`, `NetworkHost`
