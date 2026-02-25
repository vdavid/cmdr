import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/** Shows a native context menu for a tab (fire-and-forget). */
export async function showTabContextMenu(
    isPinned: boolean,
    canClose: boolean,
    hasOtherUnpinnedTabs: boolean,
): Promise<void> {
    await invoke<void>('show_tab_context_menu', {
        isPinned,
        canClose,
        hasOtherUnpinnedTabs,
    })
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
