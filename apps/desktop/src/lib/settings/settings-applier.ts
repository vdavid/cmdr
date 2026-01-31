/**
 * Settings applier - applies settings changes to the UI in real-time.
 * Updates CSS variables and other DOM properties when settings change.
 */

import { getSetting, onSettingChange, initializeSettings, type UiDensity, densityMappings } from '$lib/settings'
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('settings-applier')

let initialized = false
let unsubscribe: (() => void) | undefined

/**
 * Applies UI density settings to CSS custom properties.
 */
function applyDensity(density: UiDensity): void {
    const values = densityMappings[density]
    document.documentElement.style.setProperty('--row-height', `${String(values.rowHeight)}px`)
    document.documentElement.style.setProperty('--icon-size', `${String(values.iconSize)}px`)
    document.documentElement.style.setProperty('--density-spacing', `${String(values.spacing)}px`)
    log.debug('Applied density: {density}', { density })
}

/**
 * Applies all settings that affect the UI.
 */
function applyAllSettings(): void {
    // UI Density
    const density = getSetting('appearance.uiDensity')
    applyDensity(density)

    log.debug('Applied all settings')
}

/**
 * Handles setting changes and applies them to the UI.
 */
function handleSettingChange(id: string, value: unknown): void {
    log.debug('Setting changed: {id} = {value}', { id, value })

    switch (id) {
        case 'appearance.uiDensity':
            applyDensity(value as UiDensity)
            break
        // Other settings that need immediate UI updates can be added here
        // Date/time format and file size format are read on-demand when rendering,
        // so they don't need to trigger a re-render here
    }
}

/**
 * Initialize the settings applier.
 * Call this once on app startup in the main window.
 */
export async function initSettingsApplier(): Promise<void> {
    if (initialized) {
        log.debug('Settings applier already initialized')
        return
    }

    log.info('Initializing settings applier')

    try {
        // Ensure settings store is initialized
        await initializeSettings()

        // Apply current settings
        applyAllSettings()

        // Subscribe to future changes
        unsubscribe = onSettingChange(handleSettingChange)
        initialized = true

        log.info('Settings applier initialized successfully')
    } catch (error) {
        log.error('Failed to initialize settings applier: {error}', { error })
    }
}

/**
 * Cleanup the settings applier.
 */
export function cleanupSettingsApplier(): void {
    if (unsubscribe) {
        unsubscribe()
        unsubscribe = undefined
    }
    initialized = false
    log.debug('Settings applier cleaned up')
}
