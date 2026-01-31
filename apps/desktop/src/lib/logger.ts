/**
 * Logging configuration using LogTape.
 *
 * Usage:
 *   import { getAppLogger } from '$lib/logger'
 *   const log = getAppLogger('myFeature')
 *   log.debug('Loading data for {userId}', { userId })
 *   log.info('Loaded {count} items', { count })
 *   log.warn('Slow operation: {ms}ms', { ms })
 *   log.error('Failed to load: {error}', { error })
 *
 * Log levels (in order): debug < info < warning < error < fatal
 *
 * Default behavior:
 *   - Dev mode: info+ for all, but specific features can enable debug
 *   - Prod mode: error+ only
 *   - Verbose logging setting: when enabled, all categories get debug level
 *
 * To enable debug logs for a feature, add it to debugCategories below,
 * or enable the "Verbose logging" setting in Developer settings.
 *
 * @module logger
 */

import { configure, getConsoleSink, getLogger as getLogTapeLogger } from '@logtape/logtape'
import type { Logger } from '@logtape/logtape'
import { load, type Store } from '@tauri-apps/plugin-store'

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
        const store: Store = await load('settings-v2.json', {
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
    const loggers: Array<{
        category: string | string[]
        lowestLevel: 'debug' | 'info' | 'warning' | 'error'
        sinks: string[]
    }> = []

    if (verbose) {
        // Verbose mode: debug for all categories
        loggers.push({
            category: 'app',
            lowestLevel: 'debug',
            sinks: ['console'],
        })
    } else {
        // Normal mode: info in dev, error in prod
        loggers.push({
            category: 'app',
            lowestLevel: isDev ? 'info' : 'error',
            sinks: ['console'],
        })

        // Add debug-level loggers for specific categories
        for (const cat of debugCategories) {
            loggers.push({
                category: ['app', cat],
                lowestLevel: 'debug',
                sinks: ['console'],
            })
        }
    }

    await configure({
        sinks: {
            console: getConsoleSink(),
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

    if (isDev) {
        const log = getLogTapeLogger(['app', 'logger'])
        if (verboseLoggingEnabled) {
            log.info('Logger initialized (verbose mode, debug+ for all)')
        } else {
            log.info('Logger initialized (dev mode, info+)')
            if (debugCategories.length > 0) {
                log.info('Debug enabled for: {categories}', { categories: debugCategories.join(', ') })
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
