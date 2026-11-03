// Native-menu event listeners. Typed `on*` wrappers over the `tauri-specta`
// `events.viewModeChanged` / `events.menuSort` helpers. These are emitted
// directly from menu clicks (not via `execute-command`) so the FE applies the
// state change without double-toggling. `settings-changed` (the "Show hidden
// files" CheckMenuItem) is wrapped in `settings.ts` as `onSettingsChanged`,
// alongside the rest of the settings IPC.

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  events,
  type MediaIndexFolderChoice,
  type MediaIndexFolderExclusion,
  type MenuBarRebuilt,
  type MenuSort,
  type OpenWithCopyLaunched,
  type OpenWithCopyRefused,
  type OpenWithNotOnThisMac,
  type ShowSearchResultInFolder,
  type ViewModeChanged,
} from '#lib/ipc/bindings.js'

/**
 * The native menu bar was thrown away and rebuilt in a new language, so every
 * item is a NEW object and anything the frontend had pushed onto the old ones is
 * gone: custom accelerators, the pin/unpin label, the "Reopen closed tab"
 * enabled flag, the "Select all of the same kind" label, and the enable/disable
 * context. The listener's job is to push all of it back. Checked states and the
 * per-pane view modes survive the rebuild (Rust restores those itself).
 */
export function onMenuBarRebuilt(handler: (payload: MenuBarRebuilt) => void): Promise<UnlistenFn> {
  return events.menuBarRebuilt.listen((event) => {
    handler(event.payload)
  })
}

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

/** A search-result context menu asks the focused pane to reveal its primary clicked row. */
export function onShowSearchResultInFolder(handler: (payload: ShowSearchResultInFolder) => void): Promise<UnlistenFn> {
  return events.showSearchResultInFolder.listen((event) => {
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

/**
 * An "Open with" click on a file inside an archive (or a repo's `.git` snapshot)
 * couldn't copy the file out, so no app launched. The payload names the file, the
 * app, and the typed reason the toast words.
 */
export function onOpenWithCopyRefused(handler: (payload: OpenWithCopyRefused) => void): Promise<UnlistenFn> {
  return events.openWithCopyRefused.listen((event) => {
    handler(event.payload)
  })
}

/**
 * An "Open with" click on a file whose volume hands out paths no other app can open
 * right now (a direct SMB share whose macOS mount is gone, a phone, a server), so no
 * app launched. The payload names the app.
 */
export function onOpenWithNotOnThisMac(handler: (payload: OpenWithNotOnThisMac) => void): Promise<UnlistenFn> {
  return events.openWithNotOnThisMac.listen((event) => {
    handler(event.payload)
  })
}

/**
 * An "Open with" click on a file inside an archive (or a repo's `.git` snapshot) did
 * launch the app, on a read-only copy. The payload names the first copy, how many rows
 * were copies, and the app, for the toast that says changes stay in the copy.
 */
export function onOpenWithCopyLaunched(handler: (payload: OpenWithCopyLaunched) => void): Promise<UnlistenFn> {
  return events.openWithCopyLaunched.listen((event) => {
    handler(event.payload)
  })
}
