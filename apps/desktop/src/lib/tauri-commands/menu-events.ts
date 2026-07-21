// Native-menu event listeners. Typed `on*` wrappers over the `tauri-specta`
// `events.viewModeChanged` / `events.menuSort` helpers. These are emitted
// directly from menu clicks (not via `execute-command`) so the FE applies the
// state change without double-toggling. `settings-changed` is wrapped in
// `lib/settings-store.ts` (`subscribeToSettingsChanges`) because it's typed
// against the settings shape there.

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  events,
  type MediaIndexFolderChoice,
  type MediaIndexFolderExclusion,
  type MenuSort,
  type ViewModeChanged,
} from '$lib/ipc/bindings'

/**
 * A per-pane view-mode CheckMenuItem (Full / Brief) flipped from the native
 * menu. The payload carries the target `pane` and the new `mode` so the FE
 * updates that pane without changing focus.
 */
export function onViewModeChanged(handler: (payload: ViewModeChanged) => void): Promise<UnlistenFn> {
  return events.viewModeChanged.listen((event) => {
    handler(event.payload)
  })
}

/**
 * A Sort-by menu item clicked. The payload's `action` is `'sortBy'` (then
 * `value` is a column name) or `'sortOrder'` (then `value` is `'asc'` /
 * `'desc'`).
 */
export function onMenuSort(handler: (payload: MenuSort) => void): Promise<UnlistenFn> {
  return events.menuSort.listen((event) => {
    handler(event.payload)
  })
}

/**
 * A folder's "Don't index images in this folder" / "Index images here again"
 * context-menu item clicked. The payload carries the right-clicked folder's
 * absolute path and the target `excluded` state; the FE persists
 * `mediaIndex.excludedFolders` and live-applies it via
 * `media_index_set_excluded_folder`.
 */
export function onMediaIndexFolderExclusion(
  handler: (payload: MediaIndexFolderExclusion) => void,
): Promise<UnlistenFn> {
  return events.mediaIndexFolderExclusion.listen((event) => {
    handler(event.payload)
  })
}

/**
 * A folder's "Add to indexed folders" / "Remove from indexed folders" context-menu
 * item clicked. The payload carries the right-clicked folder's absolute path and the
 * target `chosen` state; the FE persists `mediaIndex.alwaysIndexFolders` and
 * live-applies it via `media_index_set_always_index_folder` (adding kicks a pass).
 */
export function onMediaIndexFolderChoice(handler: (payload: MediaIndexFolderChoice) => void): Promise<UnlistenFn> {
  return events.mediaIndexFolderChoice.listen((event) => {
    handler(event.payload)
  })
}
