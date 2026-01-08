/**
 * Window state persistence on resize.
 *
 * By default, tauri-plugin-window-state only saves window state when the app closes.
 * This module adds saving on resize so that hot reloads preserve the window size.
 */

import { getCurrentWindow } from '@tauri-apps/api/window'
import { saveWindowState, StateFlags } from '@tauri-apps/plugin-window-state'
import type { UnlistenFn } from '@tauri-apps/api/event'

// Debounce delay in ms - wait for resize to settle before saving
const RESIZE_DEBOUNCE_MS = 500

let unlisten: UnlistenFn | null = null
let debounceTimer: ReturnType<typeof setTimeout> | null = null

/**
 * Starts listening for window resize events and saves state after resize settles.
 * Call this once when the app initializes.
 */
export async function initWindowStateListener(): Promise<void> {
    // Avoid double-initialization
    if (unlisten !== null) {
        return
    }

    const currentWindow = getCurrentWindow()
    unlisten = await currentWindow.onResized(() => {
        // Debounce: clear any pending save and schedule a new one
        if (debounceTimer !== null) {
            clearTimeout(debounceTimer)
        }

        debounceTimer = setTimeout(() => {
            void saveWindowState(StateFlags.ALL)
            debounceTimer = null
        }, RESIZE_DEBOUNCE_MS)
    })
}
