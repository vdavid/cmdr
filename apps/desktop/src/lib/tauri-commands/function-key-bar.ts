import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

/** Shows the function key bar's context menu (fire-and-forget). */
export async function showFunctionKeyBarContextMenu(): Promise<void> {
  const res = await commands.showFunctionKeyBarContextMenu()
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Listens for the "Hide function key bar" context-menu item click, emitted by
 * `on_menu_event` in Rust. No payload: the frontend owns both the setting write
 * and the confirmation toast.
 */
export function onFunctionKeyBarHideRequested(handler: () => void): Promise<UnlistenFn> {
  return events.functionKeyBarHideRequested.listen(() => {
    handler()
  })
}
