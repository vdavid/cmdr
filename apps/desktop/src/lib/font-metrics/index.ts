// Font metrics management for calculating text widths

import { storeFontMetrics, hasFontMetrics } from '$lib/tauri-commands'
import { measureCharWidths } from './measure'
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('fontMetrics')

/**
 * Gets the current font configuration ID.
 * For now, this is hardcoded to the system font at 12px.
 * When font settings become user-configurable, this will read from settings.
 */
export function getCurrentFontId(): string {
    // Hardcoded for now - matches CSS --font-system at --font-size-sm (12px)
    return 'system-400-12'
}

/**
 * Ensures font metrics are loaded and available for width calculations.
 * If metrics are not cached, measures all characters in the background
 * and sends them to Rust for storage.
 *
 * This function is non-blocking and runs asynchronously.
 */
export async function ensureFontMetricsLoaded(): Promise<void> {
    const fontId = getCurrentFontId()

    // Check if metrics are already available
    const hasMetrics = await hasFontMetrics(fontId)
    if (hasMetrics) {
        log.debug('Metrics already loaded for {fontId}', { fontId })
        return
    }

    log.debug('Metrics not found for {fontId}, starting measurement...', { fontId })
    const startTime = performance.now()

    // Run measurement asynchronously using requestIdleCallback for non-blocking behavior
    // Fallback to setTimeout if requestIdleCallback is not available
    const runWhenIdle = (callback: () => Promise<void>) => {
        if ('requestIdleCallback' in window) {
            requestIdleCallback(() => {
                void callback()
            })
        } else {
            setTimeout(() => {
                void callback()
            }, 0)
        }
    }

    runWhenIdle(async () => {
        try {
            // Parse font ID (format: "fontFamily-weight-size")
            const parts = fontId.split('-')
            const fontFamily = parts[0] || 'system'
            const fontWeight = Number.parseInt(parts[1] || '400', 10)
            const fontSize = Number.parseInt(parts[2] || '12', 10)

            // Resolve system font to actual font family for measurement
            // The actual font used by the browser for "system" is the system default
            const actualFontFamily =
                fontFamily === 'system'
                    ? '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif'
                    : fontFamily

            // Measure character widths
            const widths = measureCharWidths(actualFontFamily, fontSize, fontWeight)

            // Send to Rust backend
            await storeFontMetrics(fontId, widths)

            const elapsed = performance.now() - startTime
            const widthCount = Object.keys(widths).length
            log.info('Measurement complete in {elapsed}ms, stored {widthCount} character widths', {
                elapsed: elapsed.toFixed(0),
                widthCount,
            })
        } catch (error) {
            log.error('Failed to measure or store font metrics: {error}', { error })
        }
    })
}
