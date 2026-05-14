// Font metrics management for calculating text widths

import { storeFontMetrics, hasFontMetrics } from '$lib/tauri-commands'
import { measureCharWidths } from './measure'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fontMetrics')

/** Base font size that the file list (Brief mode) renders text at, at scale 1. */
const BASE_FONT_SIZE_PX = 12

/**
 * Reads the effective text scale set by `lib/text-size.svelte.ts` on `:root`.
 *
 * We read the CSS variable rather than importing `getEffectiveScale` to avoid
 * a circular import (text-size re-triggers `ensureFontMetricsLoaded` after
 * each scale change). The DOM is the single contract both modules agree on,
 * and text-size always writes `--font-scale` before notifying us.
 */
function readEffectiveScale(): number {
  if (typeof window === 'undefined') return 1
  const raw = getComputedStyle(document.documentElement).getPropertyValue('--font-scale').trim()
  const parsed = Number.parseFloat(raw)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 1
}

/**
 * Gets the current font configuration ID.
 *
 * The size component scales with the effective text-size multiplier (system
 * Accessibility × user setting). The Rust cache keys metrics by this exact
 * string, so a new scale produces a new cache miss and re-measure. The Rust
 * side keeps multiple sizes in memory side-by-side (no eviction needed).
 */
export function getCurrentFontId(): string {
  const size = Math.max(1, Math.round(BASE_FONT_SIZE_PX * readEffectiveScale()))
  return `system-400-${String(size)}`
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
        fontFamily === 'system' ? '-apple-system, BlinkMacSystemFont, system-ui, sans-serif' : fontFamily

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
