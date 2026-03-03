/**
 * Settings window management.
 * Creates and manages the settings window as a separate Tauri window.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emitTo } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('settings')

const SETTINGS_WIDTH = 800
const SETTINGS_HEIGHT = 600
const SETTINGS_MAX_WIDTH = 852
const SETTINGS_MIN_WIDTH = 600
const SETTINGS_MIN_HEIGHT = 400

/**
 * Opens the settings window, or focuses it if already open.
 * Uses `WebviewWindow.getByLabel` to reliably detect an existing window
 * instead of a module-level JS reference that can go stale.
 */
export async function openSettingsWindow(): Promise<void> {
    const existing = await WebviewWindow.getByLabel('settings')
    if (existing) {
        // Emit to the settings window so it can self-focus. Cross-window setFocus()
        // doesn't reliably bring a window to front on macOS.
        await emitTo('settings', 'focus-self')
        return
    }

    log.debug('Creating new settings window')

    new WebviewWindow('settings', {
        url: '/settings',
        title: 'Settings',
        width: SETTINGS_WIDTH,
        height: SETTINGS_HEIGHT,
        minWidth: SETTINGS_MIN_WIDTH,
        minHeight: SETTINGS_MIN_HEIGHT,
        maxWidth: SETTINGS_MAX_WIDTH,
        center: true,
        resizable: true,
        decorations: true,
        focus: true,
    })
}
