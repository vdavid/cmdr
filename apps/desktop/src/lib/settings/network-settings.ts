/**
 * Network settings helper - provides computed values for network operations.
 * Centralizes the logic for calculating timeout and cache values from settings.
 */

import { getSetting } from './settings-store'

// Timeout values in milliseconds for each mode
const timeoutModeToMs = {
    normal: 15_000, // 15 seconds
    slow: 45_000, // 45 seconds
    custom: 15_000, // Will be overridden by customTimeout
} as const

/**
 * Gets the network timeout in milliseconds based on current settings.
 * For 'normal' mode: 15 seconds
 * For 'slow' mode: 45 seconds
 * For 'custom' mode: uses the customTimeout setting
 */
export function getNetworkTimeoutMs(): number {
    const mode = getSetting('network.timeoutMode')

    if (mode === 'custom') {
        // customTimeout is in seconds, convert to ms
        const customTimeoutSec = getSetting('network.customTimeout')
        return customTimeoutSec * 1000
    }

    return timeoutModeToMs[mode]
}

/**
 * Gets the mount timeout in milliseconds.
 * Uses the advanced.mountTimeout setting which is stored in milliseconds.
 */
export function getMountTimeoutMs(): number {
    return getSetting('advanced.mountTimeout')
}

/**
 * Gets the share cache TTL (Time To Live) in milliseconds.
 * Uses the network.shareCacheDuration setting which is stored in milliseconds.
 */
export function getShareCacheTtlMs(): number {
    return getSetting('network.shareCacheDuration')
}
