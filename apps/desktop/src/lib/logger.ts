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
 *
 * To enable debug logs for a feature, add it to debugCategories below.
 *
 * @module logger
 */

import { configure, getConsoleSink, getLogger as getLogTapeLogger } from '@logtape/logtape'
import type { Logger } from '@logtape/logtape'

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
    'copyProgress', // Enable to debug copy operation progress events
]

/**
 * Initialize the logging system. Call once at app startup.
 */
export async function initLogger(): Promise<void> {
    // Build logger configs: base config + debug overrides for specific features
    const loggers: Array<{
        category: string | string[]
        lowestLevel: 'debug' | 'info' | 'warning' | 'error'
        sinks: string[]
    }> = [
        // Base config: info in dev, error in prod
        {
            category: 'app',
            lowestLevel: isDev ? 'info' : 'error',
            sinks: ['console'],
        },
    ]

    // Add debug-level loggers for specific categories
    for (const cat of debugCategories) {
        loggers.push({
            category: ['app', cat],
            lowestLevel: 'debug',
            sinks: ['console'],
        })
    }

    await configure({
        sinks: {
            console: getConsoleSink(),
        },
        loggers,
    })

    if (isDev) {
        const log = getLogTapeLogger(['app', 'logger'])
        log.info('Logger initialized (dev mode, info+)')
        if (debugCategories.length > 0) {
            log.info('Debug enabled for: {categories}', { categories: debugCategories.join(', ') })
        }
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
