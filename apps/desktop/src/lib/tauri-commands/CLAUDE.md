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

| File                  | Contents                                                                                  |
| --------------------- | ----------------------------------------------------------------------------------------- |
| `index.ts`            | Barrel re-export of everything below                                                      |
| `file-listing.ts`     | Virtual-scroll listing API, drag-and-drop, `pathExists`, `createDirectory`                |
| `file-viewer.ts`      | Viewer session, icons, context menu, macOS integrations, MCP pane state, font metrics     |
| `write-operations.ts` | Copy/move/delete, conflict resolution, scan preview, `formatBytes`/`formatDuration`       |
| `rename.ts`           | `checkRenamePermission`, `checkRenameValidity`, `renameFile`, `moveToTrash`               |
| `storage.ts`          | `listVolumes`, `getVolumeSpace`, `checkFullDiskAccess`, `openPrivacySettings`             |
| `networking.ts`       | SMB host discovery, share listing, Keychain credential ops, mounting, `feLog`             |
| `mtp.ts`              | Android MTP: device listing, connect/disconnect, file ops, transfer progress, volume copy |
| `licensing.ts`        | License status, activation, expiry, server validation                                     |
| `settings.ts`         | Port checking, file watcher debounce, indexing toggle, AI subsystem commands, `updateServiceResolveTimeout` |

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

- `PaneState` and `PaneFileEntry` types live in `file-viewer.ts` (MCP state sync is viewer-adjacent).
- `formatBytes` and `formatDuration` are co-located in `write-operations.ts` with no IPC calls.
- `feLog` (frontend → Rust timestamped logging) is in `networking.ts` for historical reasons.
- `listen` and `UnlistenFn` from `@tauri-apps/api/event` are re-exported through `write-operations.ts`.

## Dependencies

- `@tauri-apps/api/core` — `invoke`
- `@tauri-apps/api/event` — `listen`, `UnlistenFn`
- `@tauri-apps/plugin-opener` — `openExternalUrl`
- Types from `$lib/file-explorer/types`
