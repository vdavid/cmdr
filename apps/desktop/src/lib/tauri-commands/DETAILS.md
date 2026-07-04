# Tauri commands: details

Depth. `CLAUDE.md` holds the must-knows; this file holds the per-file command inventory, the routing map for new
commands, and notable non-obvious placements.

## Per-file inventory

- **`ipc-types.ts`**: `TimedOut<T>`, `IpcError`, `isIpcError()`, `getIpcErrorMessage()`: shared timeout-aware types.
- **`index.ts`**: barrel re-export of everything below.
- **`file-listing.ts`**: virtual-scroll listing API, batch accessors (`getPathsAtIndices`, `getFilesAtIndices`),
  drag-and-drop, `pathExists`, `createDirectory`, `createFile`, sync status, font metrics, `getBriefColumnTextWidths`
  (Brief-view column measurement).
- **`file-viewer.ts`**: viewer session only: open, seek, search (with `useRegex` / `caseSensitive` modes), close, word
  wrap menu, encoding pickers (`viewerSetEncoding` / `viewerGetEncodingOptions`), tail mode (`viewerSetTailMode`),
  `viewerReload`.
- **`file-actions.ts`**: open file/URL, Finder reveal, Quick Look, Get Info, context menu (file / breadcrumb /
  volume-selector-row / parent-row), clipboard, open in editor, cloud actions (`cloudMakeAvailableOffline` /
  `cloudRemoveDownload`, iCloud Drive only).
- **`favorites.ts`**: user-editable switcher favorites: `addFavorite`, `removeFavorite`, `renameFavorite`,
  `reorderFavorites`, plus `stripFavoritePrefix` (recover the bare id from a `fav-…` switcher id). Listing rides
  `listVolumes` / `volumes-changed`; there's no `listFavorites`.
- **`icons.ts`**: icon fetching (`getIcons`, `getCustomFolderIconIds`, `refreshDirectoryIcons`) and cache invalidation.
- **`app-state.ts`**: MCP pane state, dialog open/close tracking, menu context, view settings, `showMainWindow`,
  child-window rect persistence (`get/setChildWindowRect`), `updateMenuAccelerator`.
- **`write-operations.ts`**: copy/move/delete, conflict resolution, scan preview, `formatBytes` / `formatDuration`.
- **`rename.ts`**: `checkRenamePermission`, `checkRenameValidity`, `renameFile`, `moveToTrash`.
- **`storage.ts`**: `listVolumes`, `getVolumeSpace`, `watchVolumeSpace` / `unwatchVolumeSpace`, `ejectVolume`,
  `getBusyVolumeIds` (bootstrap for the eject-busy gate), `onVolumeContextAction`, `checkFullDiskAccess`,
  `checkFullDiskAccessQuiet`, `getMacosMajorVersion`, `openPrivacySettings`, `openSystemSettingsUrl`.
- **`networking.ts`**: SMB host discovery, share listing, Keychain credential ops, mounting, direct-connection upgrade,
  in-place `reconnectSmbVolume` and per-volume `disconnectSmbVolume`.
- **`mtp.ts`**: Android MTP: device listing, connect/disconnect, file ops, transfer progress, volume copy.
- **`licensing.ts`**: license status, activation, expiry, server validation.
- **`settings.ts`**: port checking, file watcher debounce, indexing toggle, MCP server control, AI subsystem commands.
- **`tab.ts`**: tab context menu: `showTabContextMenu`, `onTabContextAction`.
- **`clipboard-files.ts`**: clipboard file operations: copy/cut files to system clipboard, read/paste, clear cut state.
- **`indexing.ts`**: drive-indexing commands (status reads `getIndexStatus` / `getVolumeIndexStatusById`, lifecycle
  `enable/disable/forget/rescan/clearDriveIndex`) plus the event listeners: typed `on*` wrappers over the `tauri-specta`
  `events.index*` helpers (scan/replay/aggregation progress + complete, rescan notification, dir-updated, memory
  warning).
- **`ai.ts`**: AI lifecycle event listeners
  (`onAi{DownloadProgress,Starting,ServerReady,Verifying,Installing,InstallComplete,Extracting}`) over the `events.ai*`
  helpers.
- **`appearance.ts`**: one-shot OS environment reads (`getAccentColor`, `getShouldReduceTransparency`,
  `getSystemTextSizeMultiplier`, `getLocalizedSystemStrings`) plus `onAccentColorChanged` /
  `onReduceTransparencyChanged` / `onSystemTextSizeChanged` over the OS appearance / text-size events.
- **`menu-events.ts`**: `onViewModeChanged` / `onMenuSort` over the direct (non-`execute-command`) native-menu events.
- **`directory-watcher.ts`**: `onDirectoryDiff` / `onDirectoryDeleted` over the file-watcher events (`onDirectoryDiff`
  casts the generated payload to the FE `DirectoryDiff` whose `entry` is the FE `FileEntry`).
- **`native-drag.ts`**: `onDragImageSize` / `onDragModifiers` (macOS drag overlay) + `onDragOutSessionStarted` /
  `onDragOutSessionComplete` (drag-out-to-Finder toasts).
- **`quick-look.ts`**: `onQuickLookKey` / `onQuickLookClosed` over the Quick Look panel events.
- **`downloads.ts`**: downloads-watcher commands (`downloadsWatcherStatus`, `goToLatestDownload`,
  `setGlobalGoToLatestShortcut`, `recheckDownloadsWatcherGate`) plus `onDownloadDetected` / `onGlobalShortcutFired` over
  the downloads-watcher + global-hotkey events.
- **`restricted-paths.ts`**: `onRestrictedPathsChanged` over the TCC-restricted-path-set event.
- **`dialog-events.ts`**: window-management events: `onExecuteCommand` + `emitExecuteCommand` (the unified
  menu/cross-window relay), the MCP `dialog` lifecycle (`on{Open,Focus,Close}Settings` / `…FileViewer` / `…About` /
  `…Confirmation`, `onCloseAllFileViewers`, `onMcpSettingsClose`), `requestOpenSettings` (emit `open-settings` so the
  main window opens Settings on behalf of a window without window-creation perms), `onViewerWordWrapToggled`,
  `onPersistRestrictedSetting`.
- **`git.ts`**: git-browser commands (`getGitRepoInfo`, `subscribeGitState` / `unsubscribeGitState`,
  `getGitStatusForPaths`) plus `onGitStateChanged` over the per-repo `git-state-changed` event.
- **`go-to-path.ts`**: ⌘G path resolution (`resolveGoToPath`) and the persisted recent-paths list (`getRecentPaths`,
  `addRecentPath`, `removeRecentPath`).
- **`tags.ts`**: macOS Finder color tags: `toggleTags` (toggle a color across paths) and `enrichTags` (patch fresh tag
  data into a cached listing).
- **`updates.ts`**: macOS custom updater: `checkForUpdate` / `downloadUpdate` / `installUpdate` (see
  `$lib/updates/updater.svelte.ts` for the full flow and the non-macOS Tauri-plugin fallback).
- **`debug.ts`**: dev/benchmark IPC: `benchmarkLog` (join a frontend timing into the Rust benchmark timeline).

## Where to put new commands

- Viewer session (anything prefixed `viewer_*`) → `file-viewer.ts`.
- File listing display (listing API, sync status, font metrics) → `file-listing.ts`.
- Single-file actions (open, reveal, preview, context menu) → `file-actions.ts`.
- Icons (fetch, refresh, cache clear) → `icons.ts`.
- MCP pane/dialog state, menu sync, window lifecycle → `app-state.ts`.
- Copy/move/delete operations → `write-operations.ts`.
- Rename/trash → `rename.ts`.
- Volumes/disk access → `storage.ts`.
- Network/SMB → `networking.ts`.
- MTP/Android → `mtp.ts`.
- Licensing → `licensing.ts`.
- Settings/AI → `settings.ts`.
- Clipboard file operations (copy/cut/paste files via system clipboard) → `clipboard-files.ts`.
- Drive indexing (status, enable/disable/rescan) → `indexing.ts`.
- Git browser (repo info, live state subscription, per-path status) → `git.ts`.
- Downloads watcher (status, go-to-latest, global hotkey) → `downloads.ts`.
- ⌘G path resolution and recent paths → `go-to-path.ts`.
- macOS Finder color tags → `tags.ts`.
- App updater → `updates.ts`.
- OS appearance/environment reads → `appearance.ts`.
- Dev/benchmark IPC → `debug.ts`.

## Notable non-obvious placements

- `formatBytes` and `formatDuration` are co-located in `write-operations.ts` with no IPC calls.
- `listen` and `UnlistenFn` from `@tauri-apps/api/event` are re-exported through `write-operations.ts`.
- `getSyncStatus` and font metrics (`storeFontMetrics`, `hasFontMetrics`) live in `file-listing.ts` because they
  directly support file list rendering.
