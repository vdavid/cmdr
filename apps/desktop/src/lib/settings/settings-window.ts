/**
 * Settings window management.
 * Creates and manages the settings window as a separate Tauri window.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'

let settingsWindow: WebviewWindow | null = null

/**
 * Opens the settings window, or focuses it if already open.
 */
export async function openSettingsWindow(): Promise<void> {
    // Check if window already exists
    if (settingsWindow) {
        try {
            await settingsWindow.setFocus()
            return
        } catch {
            // Window was closed, create a new one
            settingsWindow = null
        }
    }

    // Create new settings window
    settingsWindow = new WebviewWindow('settings', {
        url: '/settings',
        title: 'Settings',
        width: 800,
        height: 600,
        minWidth: 600,
        minHeight: 400,
        center: true,
        resizable: true,
        decorations: true,
    })

    // Listen for window close to clean up reference
    settingsWindow.once('tauri://destroyed', () => {
        settingsWindow = null
    })

    // Handle any creation errors
    settingsWindow.once('tauri://error', (e) => {
        console.error('Failed to create settings window:', e)
        settingsWindow = null
    })
}

/**
 * Closes the settings window if it's open.
 */
export async function closeSettingsWindow(): Promise<void> {
    if (settingsWindow) {
        try {
            await settingsWindow.close()
        } catch {
            // Window already closed
        }
        settingsWindow = null
    }
}

/**
 * Checks if the settings window is currently open.
 */
export function isSettingsWindowOpen(): boolean {
    return settingsWindow !== null
}
