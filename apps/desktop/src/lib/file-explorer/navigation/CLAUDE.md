# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and volume selector breadcrumb.

## Key files

| File                                   | Purpose                                                                                                                                                            |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `navigation-history.ts`                | Purely functional immutable history stack                                                                                                                          |
| `path-navigation.ts`                   | Picks initial path when switching volumes                                                                                                                          |
| `path-resolution.ts`                   | Walk-up `resolveValidPath` (split out to break cycle)                                                                                                              |
| `path-segments.ts`                     | Splits the breadcrumb display path into segments and flags any inside a `.git/...` portal (consumer: `FilePane.svelte` paints them with `--color-git-portal-text`) |
| `keyboard-shortcuts.ts`                | Home/End/PageUp/PageDown handling for file lists                                                                                                                   |
| `VolumeBreadcrumb.svelte`              | Clickable volume label + grouped dropdown                                                                                                                          |
| `volume-grouping.ts`                   | Pure logic: group volumes by category, get volume icons                                                                                                            |
| `volume-space-manager.svelte.ts`       | Reactive state machine for disk space fetch/retry/timeout                                                                                                          |
| `volume-breadcrumb-handlers.svelte.ts` | Submenu/breadcrumb-popup controllers, keyboard-mode tracker, and pure key-dispatch helpers for `VolumeBreadcrumb.svelte`                                           |
| `navigation-history.test.ts`           | Full unit test coverage of history functions                                                                                                                       |
| `path-navigation.test.ts`              | Unit tests for path resolution and timeouts                                                                                                                        |
| `keyboard-shortcuts.test.ts`           | Unit tests for shortcut calculations                                                                                                                               |
| `path-segments.test.ts`                | Unit tests for git-portal segment detection                                                                                                                        |

## `navigation-history.ts`

Purely functional: all operations return new objects, never mutate.

```
NavigationHistory = { stack: HistoryEntry[], currentIndex: number }
HistoryEntry = { volumeId: string, path: string, networkHost?: NetworkHost }
```

Key functions: `createHistory`, `push`, `pushPath`, `back`, `forward`, `getCurrentEntry`, `getCurrentPath`, `canGoBack`,
`canGoForward`, `setCurrentIndex`, `getEntryAt`.

`push` returns the **same reference** when the new entry equals the current one (deduplication). Callers can use
reference equality to skip re-renders.

Entries carry full `volumeId` (navigating back can cross volume boundaries, for example from an external drive back to
`root`).

### Gotcha: history is pushed on both listing success AND listing failure

`FilePane.svelte`'s `onPathChange?.(loadPath)` callback is the canonical place where a navigation lands in history. It
fires from two branches: `handleListingComplete` (success) AND the `listing-error` handler when the path still exists
(error pane will render). Without the second branch, navigating to a folder that fails to list (TCC-restricted, mode
0700, etc.) would show the `ErrorPane` while leaving the path absent from history; `Cmd+[` would then visually jump back
two steps because the current pane state isn't in the stack. The `listing-error` handler with the auto-fallback (path
deleted → navigate to parent) doesn't push via this callback; it relies on the fallback navigation's own
`applyPathChange` push.

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

`handleVolumeChange` in `DualPaneExplorer.svelte` uses **optimistic navigation**: it updates pane state immediately
(showing the loading spinner), then resolves the "best" path in the background. A `volumeChangeGeneration` counter
guards against stale corrections when the user navigates away before resolution completes.

`handleCancelLoading` navigates to `~` immediately on ESC (no `resolveValidPath` call). `handleNavigationAction`
(back/forward) navigates immediately; FilePane's listing error handler resolves upward if the path is gone.

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
which tries stored credentials first; if none found or they fail, the backend returns `credentialsNeeded` and the
`onSmbUpgradeLogin` callback triggers `FilePane` to show `NetworkLoginForm` inline (same pattern as `ShareBrowser`).
Submenu supports full keyboard navigation (ArrowRight to open, ArrowLeft/Escape to close, Enter to activate).

### USB link-speed indicator (MTP)

MTP volumes carry `usbSpeed: UsbSpeed` (`'low' | 'full' | 'high' | 'super' | 'super_plus'`) sourced from `mtp-rs` via
the shared `crate::usb_speed::UsbSpeed`. `describeUsbSpeed(speed)` in `$lib/file-explorer/types` maps each tier to
`{ tier, label, maxMBps }`; the breadcrumb renders a 5-tier rainbow dot (red → orange → yellow → light green → dark
green; dark green is `--color-allow`, same shade as SMB direct) on the right of both the closed chip and each dropdown
row. Tooltip shows `<label> (Max. <N> MB/s)\nNegotiated for this cable, port, and device` (the global tooltip CSS uses
`white-space: pre-line`, so `\n` becomes a real line break). The dot is the only visual — no inline text in the chip and
no extra line under the disk-space bar, by design.

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

Pure logic for organizing volumes into display groups. No reactive state.

`groupByCategory(vols)`: groups volumes by category in display order:

1. Favorites: no checkmark shown even if current path is a favorite
2. main_volume + attached_volume: merged into one group
3. Cloud drives
4. Mobile (MTP) devices: filtered from unified volume list (`category === 'mobile_device'`)
5. Network: always includes a synthetic `'network'` entry (`smb://`) plus any mounted SMB shares. The synthetic entry's
   name flips to `"Network (disabled)"` when `options.networkEnabled === false`. `VolumeBreadcrumb` reads
   `getNetworkEnabled()` from reactive settings to set the option, and intercepts clicks on the disabled entry to open
   Settings → File systems → SMB/Network shares (via `openSettingsWindow(['File systems', 'SMB/Network shares'])`)
   instead of navigating.

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
