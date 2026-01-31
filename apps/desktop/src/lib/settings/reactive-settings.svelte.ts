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
    densityMappings,
} from '$lib/settings'
import { formatDateTimeWithFormat } from './format-utils'
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('reactive-settings')

// Reactive state for settings that affect UI rendering
let uiDensity = $state<UiDensity>('comfortable')
let dateTimeFormat = $state<DateTimeFormat>('iso')
let customDateTimeFormat = $state<string>('YYYY-MM-DD HH:mm')

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
