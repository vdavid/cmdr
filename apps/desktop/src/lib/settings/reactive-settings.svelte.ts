/**
 * Reactive settings state for Svelte components.
 * Provides $state-based values that update immediately when settings change.
 */

import {
    getSetting,
    onSettingChange,
    initializeSettings,
    type UiDensity,
    type DateTimeFormat,
    type FileSizeFormat,
    densityMappings,
} from '$lib/settings'
import { formatDateTimeWithFormat, formatFileSizeWithFormat } from './format-utils'
import { getAppLogger } from '$lib/logger'
import { clearExtensionIconCache } from '$lib/icon-cache'

const log = getAppLogger('reactive-settings')

// Reactive state for settings that affect UI rendering
let uiDensity = $state<UiDensity>('comfortable')
let dateTimeFormat = $state<DateTimeFormat>('iso')
let customDateTimeFormat = $state<string>('YYYY-MM-DD HH:mm')
let fileSizeFormat = $state<FileSizeFormat>('binary')
let useAppIconsForDocuments = $state<boolean>(true)

let initialized = false
let unsubscribe: (() => void) | undefined

/**
 * Initialize reactive settings. Call once on app startup.
 */
export async function initReactiveSettings(): Promise<void> {
    if (initialized) return

    log.debug('Initializing reactive settings')

    try {
        await initializeSettings()

        // Load initial values
        uiDensity = getSetting('appearance.uiDensity')
        dateTimeFormat = getSetting('appearance.dateTimeFormat')
        customDateTimeFormat = getSetting('appearance.customDateTimeFormat')
        fileSizeFormat = getSetting('appearance.fileSizeFormat')
        useAppIconsForDocuments = getSetting('appearance.useAppIconsForDocuments')

        // Subscribe to changes (including cross-window changes)
        unsubscribe = onSettingChange((id, value) => {
            log.debug('Received setting change: {id} = {value}', { id, value })

            switch (id) {
                case 'appearance.uiDensity':
                    log.info('Applying UI density change: {value}', { value })
                    uiDensity = value as UiDensity
                    break
                case 'appearance.dateTimeFormat':
                    log.info('Applying date/time format change: {value}', { value })
                    dateTimeFormat = value as DateTimeFormat
                    break
                case 'appearance.customDateTimeFormat':
                    log.debug('Applying custom date format change: {value}', { value })
                    customDateTimeFormat = value as string
                    break
                case 'appearance.fileSizeFormat':
                    log.info('Applying file size format change: {value}', { value })
                    fileSizeFormat = value as FileSizeFormat
                    break
                case 'appearance.useAppIconsForDocuments':
                    log.info('Applying app icons for documents change: {value}', { value })
                    useAppIconsForDocuments = value as boolean
                    // Clear the icon cache so icons are re-fetched with the new setting
                    void clearExtensionIconCache()
                    break
            }
        })

        initialized = true
        log.info('Reactive settings initialized')
    } catch (error) {
        log.error('Failed to initialize reactive settings: {error}', { error })
    }
}

/**
 * Cleanup reactive settings.
 */
export function cleanupReactiveSettings(): void {
    unsubscribe?.()
    unsubscribe = undefined
    initialized = false
}

// ============================================================================
// Getters for reactive values (use these in components)
// ============================================================================

/** Get current row height based on density */
export function getRowHeight(): number {
    return densityMappings[uiDensity].rowHeight
}

/** Get whether the current density is compact */
export function getIsCompactDensity(): boolean {
    return uiDensity === 'compact'
}

/** Get current "use app icons for documents" setting */
export function getUseAppIconsForDocuments(): boolean {
    return useAppIconsForDocuments
}

// ============================================================================
// Formatting utilities that use reactive settings
// ============================================================================

/**
 * Format a timestamp according to current settings.
 * @param timestamp Unix timestamp in seconds
 */
export function formatDateTime(timestamp: number | undefined): string {
    return formatDateTimeWithFormat(timestamp, dateTimeFormat, customDateTimeFormat)
}

/**
 * Format bytes as human-readable string according to current settings.
 * @param bytes Number of bytes
 */
export function formatFileSize(bytes: number): string {
    return formatFileSizeWithFormat(bytes, fileSizeFormat)
}
