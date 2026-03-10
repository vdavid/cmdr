/**
 * Logging configuration using LogTape.
 *
 * Usage:
 *   import { getAppLogger } from '$lib/logging/logger'
 *   const log = getAppLogger('myFeature')
 *   log.debug('Loading data for {userId}', { userId })
 *   log.info('Loaded {count} items', { count })
 *   log.warn('Slow operation: {ms}ms', { ms })
 *   log.error('Failed to load: {error}', { error })
 *
 * Log levels (in order): debug < info < warning < error < fatal
 *
 * Default behavior:
 *   - Dev mode: info+ in browser console, debug+ sent to Rust (filtered by RUST_LOG)
 *   - Prod mode: error+ only (both sinks)
 *   - Verbose logging setting: when enabled, all categories get debug level in both sinks
 *
 * To enable debug logs for a feature in browser devtools, add it to debugCategories below.
 * To enable debug logs in the terminal, use RUST_LOG: `RUST_LOG=FE:fileExplorer=debug,info`
 * Or enable the "Verbose logging" setting in Developer settings for both.
 *
 * @module logger
 */

import { configure, getConsoleSink, getLogger as getLogTapeLogger, withFilter } from '@logtape/logtape'
import type { Logger } from '@logtape/logtape'
import { invoke } from '@tauri-apps/api/core'
import { load, type Store } from '@tauri-apps/plugin-store'
import { getTauriBridgeSink, startBridge } from './log-bridge'

// Re-export getLogger for convenience
export { getLogger } from '@logtape/logtape'
export type { Logger } from '@logtape/logtape'

const isDev = import.meta.env.DEV

/**
 * Features that should have debug logging enabled even in dev mode.
 * Add category names here to enable verbose logging for specific features.
 *
 * Example: ['fileExplorer', 'dragDrop'] enables debug for those features.
 */
const debugCategories: string[] = [
    // 'fileExplorer',
    // 'dragDrop',
    // 'licensing',
    // 'copyProgress', // Enable to debug copy operation progress events
    // 'viewer', // Enable to debug file viewer streaming/caching
    'settings', // Enable to debug settings dialog initialization and persistence
    'reactive-settings', // Enable to debug reactive settings updates
    'shortcuts', // Enable to debug keyboard shortcut persistence
    'mtp', // Enable to debug MTP device operations
]

// Track if verbose logging is enabled for reconfiguration
let verboseLoggingEnabled = false
let loggerInitialized = false

/**
 * Read the verbose logging setting directly from the store file.
 * This is needed because the logger initializes before the full settings system.
 */
async function getVerboseLoggingSetting(): Promise<boolean> {
    try {
        // Use empty defaults since we just want to read existing values
        const store: Store = await load('settings.json', {
            autoSave: false,
            defaults: {},
        })
        const value = await store.get<boolean>('developer.verboseLogging')
        return value === true
    } catch {
        // Store doesn't exist yet or can't be read - use default
        return false
    }
}

/**
 * Build and apply logger configuration.
 * @param verbose - Whether to enable debug logging for all categories
 * @param isReset - Whether this is a reconfiguration (requires reset flag)
 */
async function applyLoggerConfig(verbose: boolean, isReset: boolean): Promise<void> {
    // The tauriBridge sink always passes debug+ to Rust in dev, where RUST_LOG controls final filtering.
    // This lets `RUST_LOG=FE:fileExplorer=debug,info` work without needing to touch debugCategories.
    // The console sink (browser devtools) is gated at info+ by default to avoid noise.
    // debugCategories lowers the console gate to debug for specific features.
    const consoleLevel: 'debug' | 'info' | 'error' = verbose ? 'debug' : isDev ? 'info' : 'error'

    const loggers: Array<{
        category: string | string[]
        lowestLevel: 'debug' | 'info' | 'warning' | 'error'
        sinks: string[]
    }> = [
        // Single logger at debug level — sink-level filters handle the rest
        {
            category: 'app',
            lowestLevel: isDev || verbose ? 'debug' : 'error',
            sinks: ['console', 'tauriBridge'],
        },
    ]

    // debugCategories lower the console gate to debug for specific features
    if (!verbose) {
        for (const cat of debugCategories) {
            loggers.push({
                category: ['app', cat],
                lowestLevel: 'debug',
                sinks: ['console', 'tauriBridge'],
            })
        }
    }

    await configure({
        sinks: {
            // Console: filtered to info+ by default (debugCategories override at logger level)
            console: withFilter(getConsoleSink(), consoleLevel),
            // Bridge: passes everything to Rust — RUST_LOG handles filtering there
            tauriBridge: getTauriBridgeSink(),
        },
        loggers,
        reset: isReset,
    })
}

/**
 * Initialize the logging system. Call once at app startup.
 */
export async function initLogger(): Promise<void> {
    if (loggerInitialized) {
        return
    }

    // Read verbose logging setting from store (before full settings system is up)
    verboseLoggingEnabled = await getVerboseLoggingSetting()

    await applyLoggerConfig(verboseLoggingEnabled, false)
    loggerInitialized = true
    startBridge()

    if (isDev) {
        const log = getLogTapeLogger(['app', 'logger'])
        if (verboseLoggingEnabled) {
            log.debug('Logger initialized (verbose mode, debug+ for all)')
        } else {
            log.debug('Logger initialized (dev mode, info+)')
            if (debugCategories.length > 0) {
                log.debug('Debug enabled for: {categories}', { categories: debugCategories.join(', ') })
            }
        }
    }
}

/**
 * Enable or disable verbose logging at runtime.
 * Called when the developer.verboseLogging setting changes.
 */
export async function setVerboseLogging(enabled: boolean): Promise<void> {
    if (enabled === verboseLoggingEnabled) {
        return
    }

    verboseLoggingEnabled = enabled
    await applyLoggerConfig(enabled, true)

    // Also update the Rust-side log level to match
    try {
        await invoke('set_log_level', { level: enabled ? 'debug' : 'info' })
    } catch {
        // Backend may not be ready during early startup — silently ignore
    }

    const log = getLogTapeLogger(['app', 'logger'])
    if (enabled) {
        log.info('Verbose logging enabled - debug level for all categories')
    } else {
        log.info('Verbose logging disabled - returning to normal log levels')
    }
}

/**
 * Get a logger for a specific feature.
 * Categories are hierarchical, for example ['app', 'fileExplorer', 'selection'].
 *
 * @example
 * const log = getAppLogger('fileExplorer')
 * log.debug('Selected {count} items', { count })
 */
export function getAppLogger(feature: string): Logger {
    return getLogTapeLogger(['app', feature])
}
