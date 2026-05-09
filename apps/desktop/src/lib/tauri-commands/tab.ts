import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

/** Shows a native context menu for a tab (fire-and-forget). */
export async function showTabContextMenu(
  isPinned: boolean,
  canClose: boolean,
  hasOtherUnpinnedTabs: boolean,
): Promise<void> {
  const res = await commands.showTabContextMenu(isPinned, canClose, hasOtherUnpinnedTabs)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Listens for the tab context menu action event emitted by `on_menu_event` in Rust.
 * The event fires asynchronously after `popup()` returns because muda queues the
 * `MenuEvent` through the event loop.
 */
export function onTabContextAction(handler: (action: string) => void): Promise<UnlistenFn> {
  return listen<{ action: string }>('tab-context-action', (event) => {
    handler(event.payload.action)
  })
}
