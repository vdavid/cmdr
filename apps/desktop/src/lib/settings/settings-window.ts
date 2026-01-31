/**
 * Settings window management.
 * Creates and manages the settings window as a separate Tauri window.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('settings')

let settingsWindow: WebviewWindow | null = null

const SETTINGS_WIDTH = 800
const SETTINGS_HEIGHT = 600
const SETTINGS_MAX_WIDTH = 852
const SETTINGS_MIN_WIDTH = 600
const SETTINGS_MIN_HEIGHT = 400

/**
 * Opens the settings window, or focuses it if already open.
 * Window always opens centered on screen.
 */
export async function openSettingsWindow(): Promise<void> {
    log.debug('openSettingsWindow called')

    // Check if window already exists
    if (settingsWindow) {
        log.debug('Settings window already exists, attempting to focus')
        try {
            await settingsWindow.setFocus()
            log.debug('Focused existing settings window')
            return
        } catch (error) {
            // Window was closed, create a new one
            log.debug('Failed to focus existing window (likely closed), creating new: {error}', { error })
            settingsWindow = null
        }
    }

    log.info('Creating new settings window with url=/settings')

    // Create new settings window, centered on screen
    settingsWindow = new WebviewWindow('settings', {
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
    })

    // Listen for window creation success
    void settingsWindow.once('tauri://created', () => {
        log.info('Settings window created successfully')
    })

    // Listen for window close to clean up reference
    void settingsWindow.once('tauri://destroyed', () => {
        log.debug('Settings window destroyed, cleaning up reference')
        settingsWindow = null
    })

    // Handle any creation errors
    void settingsWindow.once('tauri://error', (e) => {
        log.error('Failed to create settings window: {error}', { error: e })
        settingsWindow = null
    })
}

/**
 * Closes the settings window if it's open.
 */
export async function closeSettingsWindow(): Promise<void> {
    log.debug('closeSettingsWindow called, windowExists={exists}', { exists: settingsWindow !== null })

    if (settingsWindow) {
        try {
            await settingsWindow.close()
            log.info('Settings window closed')
        } catch (error) {
            log.debug('Failed to close settings window (likely already closed): {error}', { error })
        }
        settingsWindow = null
    }
}

/**
 * Checks if the settings window is currently open.
 */
export function isSettingsWindowOpen(): boolean {
    const isOpen = settingsWindow !== null
    log.debug('isSettingsWindowOpen() = {isOpen}', { isOpen })
    return isOpen
}
