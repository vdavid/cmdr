# Tauri commands

Typed TypeScript wrappers for every Tauri IPC command and event. This is the canonical import path for all backend
communication — never import from sub-files directly.

```ts
// Correct
import { listDirectoryStart, copyFiles } from '$lib/tauri-commands'

// Wrong — imports from sub-files directly
import { listDirectoryStart } from '$lib/tauri-commands/file-listing'
```

## Files

| File                  | Contents                                                                                              |
| --------------------- | ----------------------------------------------------------------------------------------------------- |
| `index.ts`            | Barrel re-export of everything below                                                                  |
| `file-listing.ts`     | Virtual-scroll listing API, drag-and-drop, `pathExists`, `createDirectory`, sync status, font metrics |
| `file-viewer.ts`      | Viewer session only: open, seek, search, close, word wrap menu                                        |
| `file-actions.ts`     | Open file/URL, Finder reveal, Quick Look, Get Info, context menu, clipboard, open in editor           |
| `icons.ts`            | Icon fetching (`getIcons`, `refreshDirectoryIcons`) and cache invalidation                            |
| `app-state.ts`        | MCP pane state, dialog open/close tracking, menu context, view settings, `showMainWindow`             |
| `write-operations.ts` | Copy/move/delete, conflict resolution, scan preview, `formatBytes`/`formatDuration`                   |
| `rename.ts`           | `checkRenamePermission`, `checkRenameValidity`, `renameFile`, `moveToTrash`                           |
| `storage.ts`          | `listVolumes`, `getVolumeSpace`, `checkFullDiskAccess`, `openPrivacySettings`                         |
| `networking.ts`       | SMB host discovery, share listing, Keychain credential ops, mounting                                  |
| `mtp.ts`              | Android MTP: device listing, connect/disconnect, file ops, transfer progress, volume copy             |
| `licensing.ts`        | License status, activation, expiry, server validation                                                 |
| `settings.ts`         | Port checking, file watcher debounce, indexing toggle, AI subsystem commands                          |

## Where to put new commands

- **Viewer session** (anything prefixed `viewer_*`) → `file-viewer.ts`
- **File listing display** (listing API, sync status, font metrics) → `file-listing.ts`
- **Single-file actions** (open, reveal, preview, context menu) → `file-actions.ts`
- **Icons** (fetch, refresh, cache clear) → `icons.ts`
- **MCP pane/dialog state, menu sync, window lifecycle** → `app-state.ts`
- **Copy/move/delete operations** → `write-operations.ts`
- **Rename/trash** → `rename.ts`
- **Volumes/disk access** → `storage.ts`
- **Network/SMB** → `networking.ts`
- **MTP/Android** → `mtp.ts`
- **Licensing** → `licensing.ts`
- **Settings/AI** → `settings.ts`

## Key patterns

**Every function** wraps `invoke<T>(commandName, args)` with camelCase args matching Rust's
`serde(rename_all = "camelCase")`.

**Event listeners** return `UnlistenFn`. Callers must call it in `onDestroy` to avoid leaks:

```ts
const unlisten = await onWriteProgress((event) => { ... })
onDestroy(() => { unlisten() })
```

**macOS-only commands** (e.g. `quickLook`, `getInfo`, `showInFinder`, `openPrivacySettings`) are wrapped in try/catch
returning safe empty/null fallbacks so the same code runs on other platforms.

## Notable non-obvious placements

- `formatBytes` and `formatDuration` are co-located in `write-operations.ts` with no IPC calls.
- `listen` and `UnlistenFn` from `@tauri-apps/api/event` are re-exported through `write-operations.ts`.
- `getSyncStatus` and font metrics (`storeFontMetrics`, `hasFontMetrics`) live in `file-listing.ts` because they
  directly support file list rendering.

## Dependencies

- `@tauri-apps/api/core` — `invoke`
- `@tauri-apps/api/event` — `listen`, `UnlistenFn`
- `@tauri-apps/plugin-opener` — `openFile`, `openExternalUrl`
- Types from `$lib/file-explorer/types`
