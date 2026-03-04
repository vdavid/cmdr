# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and volume selector breadcrumb.

## Key files

| File                         | Purpose                                          |
| ---------------------------- | ------------------------------------------------ |
| `navigation-history.ts`      | Purely functional immutable history stack        |
| `path-navigation.ts`         | Async path resolution with fallback chain        |
| `keyboard-shortcuts.ts`      | Home/End/PageUp/PageDown handling for file lists |
| `VolumeBreadcrumb.svelte`    | Clickable volume label + grouped dropdown        |
| `navigation-history.test.ts` | Full unit test coverage of history functions     |
| `path-navigation.test.ts`    | Unit tests for path resolution and timeouts      |
| `keyboard-shortcuts.test.ts` | Unit tests for shortcut calculations             |

## `navigation-history.ts`

Purely functional — all operations return new objects, never mutate.

```
NavigationHistory = { stack: HistoryEntry[], currentIndex: number }
HistoryEntry = { volumeId: string, path: string, networkHost?: NetworkHost }
```

Key functions: `createHistory`, `push`, `pushPath`, `back`, `forward`, `getCurrentEntry`, `getCurrentPath`, `canGoBack`,
`canGoForward`, `setCurrentIndex`, `getEntryAt`.

`push` returns the **same reference** when the new entry equals the current one (deduplication). Callers can use
reference equality to skip re-renders.

Entries carry full `volumeId` — navigating back can cross volume boundaries (e.g. from an external drive back to
`root`).

## `path-navigation.ts`

`determineNavigationPath(volumeId, volumePath, targetPath, otherPane)` — picks best initial path when switching volumes.
Runs checks **in parallel** with 500ms frontend timeouts per check. Priority:

1. Favorite path (when `targetPath !== volumePath`)
2. Other pane's path (if same volume and path exists)
3. Stored `lastUsedPath` for this volume
4. Default: `~` for `DEFAULT_VOLUME_ID`, else volume root

`resolveValidPath(targetPath)` — walks parent tree until an existing directory is found. Each step has a **1-second
frontend timeout**. Fallback chain: parent dirs → `~` → `/` → `null` (volume unmounted).

`withTimeout(promise, ms, fallback)` — imported from `$lib/utils/timing` and re-exported. Races a promise against a
timeout, returning the fallback on expiry. Used by both functions above, and also by `VolumeBreadcrumb.svelte` (wraps
`getVolumeSpace`) and `DualPaneExplorer.svelte` (wraps `findContainingVolume` during startup tab restore).

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

- `currentIndex`, `totalCount` — always required
- `itemsPerColumn`, `visibleColumns` — Brief mode only; presence enables Brief branch
- `visibleItems` — Full mode PageUp/PageDown page size

Handled keys:

| Key               | Brief mode                                                     | Full mode              |
| ----------------- | -------------------------------------------------------------- | ---------------------- |
| Option+Up / Home  | First item                                                     | First item             |
| Option+Down / End | Last item                                                      | Last item              |
| PageUp            | Bottom of column (visibleColumns-1) to the left, or first item | Up by visibleItems-1   |
| PageDown          | Bottom of column (visibleColumns-1) to the right, or last item | Down by visibleItems-1 |

Meta+Home/End is intentionally not handled (passes to OS). Returns `null` for unhandled keys.

Brief PageUp/PageDown lands on the **bottom row** of the target column (TUI convention).

## `VolumeBreadcrumb.svelte`

Clickable label that opens a grouped dropdown of all available volumes.

Volume groups (in display order):

1. Favorites — no checkmark shown even if current path is a favorite
2. main_volume + attached_volume — merged into one group
3. Cloud drives
4. Mobile (MTP) devices
5. Network — always includes a synthetic `'network'` entry (`smb://`) plus any mounted SMB shares

Props: `volumeId`, `currentPath`, `onVolumeChange?`.

`containingVolumeId` is derived separately via `findContainingVolume(currentPath)` — the active checkmark tracks the
real containing volume, not the `volumeId` prop (which may be a favorite's virtual ID).

Keyboard/mouse mode: entering keyboard nav sets `isKeyboardMode = true`, suppressing CSS `:hover` highlights. Mouse
movement > 5px threshold exits keyboard mode.

MTP volumes are refreshed with a 100ms delay after hotplug events (`mtp-device-detected`, `mtp-device-connected`,
`mtp-device-removed`) to let `mtp-store`'s own event handler finish first.

### Timeout-aware UI

Both `listVolumes()` and `getVolumeSpace()` return `TimedOut<T>` wrappers. The component tracks timeout state and
renders inline indicators (no toasts):

- **Volume list timeout** (`volumesTimedOut`): Shows a warning row at the bottom of the dropdown ("Some volumes may be
  missing") with a retry button that re-calls `listVolumes()`. The warning auto-clears on successful retry. The dropdown
  still opens with whatever partial data was returned.
- **Volume space timeout** (`spaceTimedOutSet`): Three-state cycle with per-volume tracking:
    - **Idle**: Dashed-outline placeholder bar with "?" icon, "Unavailable" text, tooltip "Couldn't fetch disk space --
      click to retry". After a retry has been attempted, tooltip changes to "Still unavailable -- click to retry".
    - **Retrying** (`spaceRetryingSet`): Spinner replaces "?", text shows "Retrying", tooltip "Retrying..." (manual) or
      "Retrying automatically..." (auto). Clicks are debounced (ignored while in-flight).
    - **Failed**: Brief shake animation (300ms), then returns to idle with "Still unavailable" tooltip.
    - **Auto-retry**: 5s after initial timeout, an automatic retry fires with full visual feedback (spinner + shake on
      failure). Tracked via `spaceAutoRetryingSet` for tooltip distinction.
    - All retry sets are cleared on volume mount/unmount events. Auto-retry timers are cleaned up on destroy.
    - Reduced motion: spinner degrades to pulsing opacity, shake degrades to opacity flash.

Exported methods for parent components: `toggle()`, `open()`, `close()`, `getIsOpen()`, `handleKeyDown(e)`.

## Dependencies

- `$lib/tauri-commands` — `listVolumes`, `findContainingVolume`, `listen`, `pathExists`
- `$lib/utils/timing` — `withTimeout` (defense-in-depth IPC timeout wrapper)
- `$lib/app-status-store` — `getLastUsedPathForVolume`
- `$lib/mtp` — `getMtpVolumes`, `initialize`, `scanDevices`
- `../types` — `VolumeInfo`, `LocationCategory`, `NetworkHost`
