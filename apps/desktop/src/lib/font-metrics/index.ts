// Font metrics management for calculating text widths.
//
// Owns the lifecycle: which window measures, when the eager set is measured,
// how the backend's fill-in requests are served, and the IPC to Rust. The
// measuring itself happens in `worker-client.ts`, off the main thread.

import { storeFontMetrics, extendFontMetrics, hasFontMetrics } from '$lib/tauri-commands'
import { parseFontId } from './measure'
import { eagerCodePoints, isMeasurable } from './ranges'
import { measureOffMainThread } from './worker-client'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fontMetrics')

/** Base font size that the file list (Brief mode) renders text at, at scale 1. */
const BASE_FONT_SIZE_PX = 12

/**
 * Whether this window measures font metrics at all.
 *
 * Only the window that renders Brief mode needs them, but every window runs
 * `initTextSize`, so without this gate a text-size change made all of them
 * measure the same font concurrently — duplicating the work on a thread they
 * share. Set from `initTextSize({ measuresFontMetrics })`.
 */
let measuresFontMetrics = false

/** In-flight eager measurements, keyed by font ID. Collapses concurrent callers. */
const inFlight = new Map<string, Promise<void>>()

/**
 * Code points already sent to the backend per font ID, so a fill-in request
 * repeated across listings measures once. Entries are removed again if the
 * measurement fails, so a transient failure doesn't strand them at the average
 * width forever.
 */
const filled = new Map<string, Set<number>>()

/**
 * Declares whether this window is responsible for measuring. Called by
 * `initTextSize`; defaults to `false` so a newly added window is opted out
 * until it says otherwise.
 */
export function setMeasuresFontMetrics(owns: boolean): void {
  measuresFontMetrics = owns
}

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
 * Ensures the eager code-point set is measured and stored for the current font
 * size. Resolves once Rust holds the widths (or immediately if it already
 * does).
 *
 * Concurrent callers share one measurement: `DualPaneExplorer` on mount, the
 * text-size debounce, and `BriefList`'s not-ready retry all land here, and
 * before the in-flight map they could each start a full pass.
 */
export function ensureFontMetricsLoaded(): Promise<void> {
  if (!measuresFontMetrics) return Promise.resolve()

  const fontId = getCurrentFontId()

  const existing = inFlight.get(fontId)
  if (existing) return existing

  // Deliberately not `async`: the in-flight entry has to be registered in the
  // same tick as the lookup above. With an `await` before the `set`, every
  // concurrent caller gets past the check and starts its own pass, which is
  // the duplicate work this map exists to prevent.
  const run = loadIfNeeded(fontId).finally(() => {
    inFlight.delete(fontId)
  })
  inFlight.set(fontId, run)
  return run
}

/** Measures the eager set unless Rust already holds this font's widths. */
async function loadIfNeeded(fontId: string): Promise<void> {
  if (await hasFontMetrics(fontId)) {
    log.debug('Metrics already loaded for {fontId}', { fontId })
    return
  }
  await measureAndStore(fontId)
}

/** Measures the eager set for `fontId` and hands it to Rust. */
async function measureAndStore(fontId: string): Promise<void> {
  const codePoints = eagerCodePoints()
  log.debug('Measuring {count} eager code points for {fontId}', { count: codePoints.length, fontId })

  try {
    const measureStart = performance.now()
    const { widths, via } = await measureOffMainThread(parseFontId(fontId), codePoints)
    const measureMs = performance.now() - measureStart

    const storeStart = performance.now()
    await storeFontMetrics(fontId, Array.from(codePoints), Array.from(widths))
    const storeMs = performance.now() - storeStart

    log.info('Measured {count} code points for {fontId} in {measureMs}ms via {via}, stored in {storeMs}ms', {
      count: codePoints.length,
      fontId,
      measureMs: measureMs.toFixed(0),
      via,
      storeMs: storeMs.toFixed(0),
    })
  } catch (error) {
    log.error('Failed to measure or store font metrics for {fontId}: {error}', { fontId, error })
  }
}

/**
 * Measures code points the backend reported as unmeasured and merges them into
 * the cache.
 *
 * The backend answers a width query immediately, substituting the average
 * width for anything it hasn't got, and reports what was missing. This fills
 * those gaps in the background so the next query — and every one after — uses
 * real widths. See `DETAILS.md` § On-demand fill-in.
 *
 * @returns `true` when new widths were stored, meaning the caller should
 *   re-query. `false` when there was nothing new to do.
 */
export async function fillMissingFontMetrics(fontId: string, codePoints: readonly number[]): Promise<boolean> {
  if (!measuresFontMetrics || codePoints.length === 0) return false

  let alreadySent = filled.get(fontId)
  if (!alreadySent) {
    alreadySent = new Set<number>()
    filled.set(fontId, alreadySent)
  }

  const todo = new Uint32Array(codePoints.filter((cp) => isMeasurable(cp) && !alreadySent.has(cp)))
  if (todo.length === 0) return false
  for (const cp of todo) alreadySent.add(cp)

  try {
    const measureStart = performance.now()
    const { widths, via } = await measureOffMainThread(parseFontId(fontId), todo)
    const measureMs = performance.now() - measureStart

    await extendFontMetrics(fontId, Array.from(todo), Array.from(widths))

    log.info('Filled {count} previously unmeasured code points for {fontId} in {measureMs}ms via {via}', {
      count: todo.length,
      fontId,
      measureMs: measureMs.toFixed(0),
      via,
    })
    return true
  } catch (error) {
    // Let a later report retry these rather than leaving them on the average.
    for (const cp of todo) alreadySent.delete(cp)
    log.error('Failed to fill missing font metrics for {fontId}: {error}', { fontId, error })
    return false
  }
}

/** Test seam: clears the dedup state between cases. */
export function resetFontMetricsStateForTests(): void {
  inFlight.clear()
  filled.clear()
  measuresFontMetrics = false
}
