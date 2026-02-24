/**
 * Settings applier - applies settings changes to the UI and Rust backend in real-time.
 * Updates CSS variables, DOM properties, and syncs backend configurations when settings change.
 */

import { getSetting, onSettingChange, initializeSettings, type UiDensity, densityMappings } from '$lib/settings'
import { getAppLogger, setVerboseLogging } from '$lib/logging/logger'
import { updateFileWatcherDebounce, updateServiceResolveTimeout, setIndexingEnabled } from '$lib/tauri-commands'

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
 * Applies Rust backend settings that need to be synced on startup.
 */
async function applyBackendSettings(): Promise<void> {
    // File watcher debounce
    const debounceMs = getSetting('advanced.fileWatcherDebounce')
    await updateFileWatcherDebounce(debounceMs)

    // Service resolve timeout
    const resolveTimeoutMs = getSetting('advanced.serviceResolveTimeout')
    await updateServiceResolveTimeout(resolveTimeoutMs)

    log.debug('Applied backend settings: debounce={debounce}ms, resolveTimeout={timeout}ms', {
        debounce: debounceMs,
        timeout: resolveTimeoutMs,
    })
}

/**
 * Applies all settings that affect the UI.
 */
function applyAllSettings(): void {
    // UI Density
    const density = getSetting('appearance.uiDensity')
    applyDensity(density)

    // Backend settings (async, fire-and-forget for startup)
    void applyBackendSettings()

    log.debug('Applied all settings')
}

/**
 * Handles setting changes and applies them to the UI or backend.
 */
function handleSettingChange(id: string, value: unknown): void {
    log.debug('Setting changed: {id} = {value}', { id, value })

    switch (id) {
        case 'appearance.uiDensity':
            applyDensity(value as UiDensity)
            break
        case 'developer.verboseLogging':
            // Reconfigure logger when verbose logging setting changes
            void setVerboseLogging(value as boolean)
            break
        case 'advanced.fileWatcherDebounce':
            // Update Rust backend file watcher debounce
            void updateFileWatcherDebounce(value as number)
            break
        case 'advanced.serviceResolveTimeout':
            // Update Rust backend Bonjour resolve timeout
            void updateServiceResolveTimeout(value as number)
            break
        case 'indexing.enabled':
            // Start or stop drive indexing
            void setIndexingEnabled(value as boolean)
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
