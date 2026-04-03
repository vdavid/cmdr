# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and volume selector breadcrumb.

## Key files

| File                             | Purpose                                                   |
| -------------------------------- | --------------------------------------------------------- |
| `navigation-history.ts`          | Purely functional immutable history stack                 |
| `path-navigation.ts`             | Async path resolution with fallback chain                 |
| `keyboard-shortcuts.ts`          | Home/End/PageUp/PageDown handling for file lists          |
| `VolumeBreadcrumb.svelte`        | Clickable volume label + grouped dropdown                 |
| `volume-grouping.ts`             | Pure logic: group volumes by category, get volume icons   |
| `volume-space-manager.svelte.ts` | Reactive state machine for disk space fetch/retry/timeout |
| `navigation-history.test.ts`     | Full unit test coverage of history functions              |
| `path-navigation.test.ts`        | Unit tests for path resolution and timeouts               |
| `keyboard-shortcuts.test.ts`     | Unit tests for shortcut calculations                      |

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

`resolveValidPath(targetPath, options?)` — walks parent tree until an existing directory is found. Accepts optional
`{ pathExistsFn, timeoutMs }` — defaults to Tauri `pathExists` with 1s timeout per step. Used both at runtime (with
timeouts) and at startup via `app-status-store.ts`'s `resolvePersistedPath` wrapper (no timeout, injected
`pathExistsFn`). Fallback chain: parent dirs → `~` → `/` → `null` (volume unmounted).

`withTimeout(promise, ms, fallback)` — imported from `$lib/utils/timing` and re-exported. Races a promise against a
timeout, returning the fallback on expiry. Used by both functions above, and also by `VolumeBreadcrumb.svelte` (wraps
`getVolumeSpace`). `DualPaneExplorer.svelte` uses `resolvePathVolume` for startup tab restore (backend has its own 2s
timeout, no frontend wrapper needed).

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

Pure presentational component. Reads the volume list from the shared `volume-store.svelte.ts` (no fetching, no event
listeners for volume changes). Volume grouping logic and disk-space retry state are extracted into `volume-grouping.ts`
and `volume-space-manager.svelte.ts` respectively.

Props: `volumeId`, `currentPath`, `onVolumeChange?`.

`containingVolumeId` is derived separately via `resolvePathVolume(currentPath)` — the active checkmark tracks the real
containing volume, not the `volumeId` prop (which may be a favorite's virtual ID).

Keyboard/mouse mode: entering keyboard nav sets `isKeyboardMode = true`, suppressing CSS `:hover` highlights. Mouse
movement > 5px threshold exits keyboard mode.

Volumes (including MTP) come from the shared `volume-store` which is pushed by the backend via a single
`volumes-changed` event. MTP volume space is fetched via `getVolumeSpace()` like any other volume.

Exported methods for parent components: `toggle()`, `open()`, `close()`, `getIsOpen()`, `handleKeyDown(e)`.

## `volume-grouping.ts`

Pure logic for organizing volumes into display groups. No reactive state.

`groupByCategory(vols)` — groups volumes by category in display order:

1. Favorites — no checkmark shown even if current path is a favorite
2. main_volume + attached_volume — merged into one group
3. Cloud drives
4. Mobile (MTP) devices — filtered from unified volume list (`category === 'mobile_device'`)
5. Network — always includes a synthetic `'network'` entry (`smb://`) plus any mounted SMB shares

`getIconForVolume(volume)` — returns the appropriate icon path for a volume based on its category.

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

- `$lib/stores/volume-store.svelte` — `getVolumes` (backend-pushed reactive volume list)
- `$lib/tauri-commands` — `resolvePathVolume`, `pathExists`
- `$lib/utils/timing` — `withTimeout` (defense-in-depth IPC timeout wrapper)
- `$lib/app-status-store` — `getLastUsedPathForVolume`
- `../types` — `VolumeInfo`, `LocationCategory`, `NetworkHost`
