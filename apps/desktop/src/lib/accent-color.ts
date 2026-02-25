/**
 * System accent color integration.
 *
 * Reads the macOS system accent color from the Rust backend on startup
 * and listens for live changes when the user updates their accent color
 * in System Settings. Applies the color based on the user's "App color"
 * setting: either the macOS system accent or the Cmdr brand gold.
 *
 * --color-system-accent is always set to the system color (for the
 * settings preview). --color-accent is set based on the user's choice.
 * When 'cmdr-gold' is selected, the inline --color-accent is removed
 * so the CSS fallback in app.css takes effect.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'
import { clearDirectoryIconCache } from '$lib/icon-cache'
import { getSetting, onSpecificSettingChange } from '$lib/settings'

const log = getAppLogger('accent-color')

let unlisten: UnlistenFn | undefined
let unlistenSetting: (() => void) | undefined
let lastSystemColor: string | undefined

function applySystemAccentPreview(hex: string): void {
    document.documentElement.style.setProperty('--color-system-accent', hex)
    lastSystemColor = hex
}

function applyAccentForCurrentSetting(): void {
    const appColor = getSetting('appearance.appColor')
    if (appColor === 'system' && lastSystemColor) {
        document.documentElement.style.setProperty('--color-accent', lastSystemColor)
        log.debug('Applied system accent color: {hex}', { hex: lastSystemColor })
    } else {
        // Remove inline override — CSS fallback (Cmdr gold) takes effect
        document.documentElement.style.removeProperty('--color-accent')
        log.debug('Removed accent override, using CSS fallback (Cmdr gold)')
    }
}

/**
 * Fetches the system accent color and applies it based on the user's
 * "App color" setting, then listens for both OS and setting changes.
 * Call once on app startup.
 */
export async function initAccentColor(): Promise<void> {
    // Load system accent color
    try {
        const hex = await invoke<string>('get_accent_color')
        applySystemAccentPreview(hex)
        applyAccentForCurrentSetting()
        log.info('System accent color loaded: {hex}', { hex })
    } catch (error) {
        log.warn('Could not read system accent color, using CSS fallback: {error}', { error })
    }

    // Listen for OS-level accent color changes
    try {
        unlisten = await listen<string>('accent-color-changed', (event) => {
            applySystemAccentPreview(event.payload)
            applyAccentForCurrentSetting()
            // macOS renders folder icons with the accent color baked in,
            // so we need to flush cached folder bitmaps and re-fetch them.
            void clearDirectoryIconCache()
            log.info('System accent color changed: {hex}', { hex: event.payload })
        })
    } catch (error) {
        log.warn('Could not subscribe to accent color changes: {error}', { error })
    }

    // Listen for setting changes
    unlistenSetting = onSpecificSettingChange('appearance.appColor', () => {
        applyAccentForCurrentSetting()
        // Flush folder icon cache since accent color affects folder icons
        void clearDirectoryIconCache()
    })
}

/** Cleans up event listeners. */
export function cleanupAccentColor(): void {
    unlisten?.()
    unlisten = undefined
    unlistenSetting?.()
    unlistenSetting = undefined
    log.debug('Accent color listeners cleaned up')
}
