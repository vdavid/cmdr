// Native-menu event listeners. Typed `on*` wrappers over the `tauri-specta`
// `events.viewModeChanged` / `events.menuSort` helpers. These are emitted
// directly from menu clicks (not via `execute-command`) so the FE applies the
// state change without double-toggling. `settings-changed` is wrapped in
// `lib/settings-store.ts` (`subscribeToSettingsChanges`) because it's typed
// against the settings shape there.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events, type MenuSort, type ViewModeChanged } from '$lib/ipc/bindings'

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
