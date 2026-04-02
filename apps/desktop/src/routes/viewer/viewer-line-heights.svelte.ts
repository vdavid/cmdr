import type { PreparedText } from '@chenglou/pretext'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('viewer')

// Dynamic import so a missing/broken dependency doesn't crash the entire viewer module.
// The viewer works fine without pretext (falls back to averaged heights).
let pretextModule: typeof import('@chenglou/pretext') | null = null
const pretextReady = import('@chenglou/pretext')
  .then((m) => {
    pretextModule = m
    log.info('pretext library loaded')
  })
  .catch((e: unknown) => {
    log.error('Failed to load pretext library, falling back to averaged heights: {error}', {
      error: String(e),
    })
  })

const LINE_HEIGHT = 18
const MAX_LINES = 50_000
const PREPARE_TIMEOUT_MS = 2_000

// WebKit in Tauri doesn't have requestIdleCallback. Fall back to setTimeout.
const scheduleIdle: (cb: IdleRequestCallback) => number =
  typeof requestIdleCallback === 'function'
    ? requestIdleCallback
    : (cb) =>
        setTimeout(() => cb({ didTimeout: false, timeRemaining: () => 10 } as IdleDeadline), 1) as unknown as number
const FONT_VALIDATION_TEST_STRING = 'ABCDabcd1234!@#$%^&*()_+-=[]{}|;:,./<>?'
const FONT_VALIDATION_TOLERANCE_PX = 1

export { LINE_HEIGHT }

/**
 * Resolves the actual font string from the viewer's CSS custom properties.
 * Creates a hidden probe element styled like viewer lines, reads getComputedStyle().font,
 * then validates that canvas measureText agrees with DOM measurement.
 * Returns the font string on success, or null if validation fails.
 */
function resolveAndValidateFont(): string | null {
  const probe = document.createElement('span')
  probe.style.fontFamily = 'var(--font-mono)'
  probe.style.fontSize = 'var(--font-size-sm)'
  probe.style.lineHeight = '1.5'
  probe.style.position = 'absolute'
  probe.style.visibility = 'hidden'
  probe.style.whiteSpace = 'pre'
  document.body.appendChild(probe)

  const computedFont = getComputedStyle(probe).font

  // Validate: canvas measureText vs DOM width for a test string
  probe.textContent = FONT_VALIDATION_TEST_STRING
  const domWidth = probe.getBoundingClientRect().width

  document.body.removeChild(probe)

  if (!computedFont) {
    log.warn('Font resolution failed: getComputedStyle().font returned empty')
    return null
  }

  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')
  if (!ctx) {
    log.warn('Font validation failed: could not create canvas 2d context')
    return null
  }
  ctx.font = computedFont
  const canvasWidth = ctx.measureText(FONT_VALIDATION_TEST_STRING).width

  const drift = Math.abs(canvasWidth - domWidth)
  if (drift > FONT_VALIDATION_TOLERANCE_PX) {
    log.warn(
      'Font validation failed: canvas width ({canvasWidth}) differs from DOM width ({domWidth}) by {drift}px for font "{font}"',
      {
        canvasWidth: canvasWidth.toFixed(2),
        domWidth: domWidth.toFixed(2),
        drift: drift.toFixed(2),
        font: computedFont,
      },
    )
    return null
  }

  log.debug('Font resolved and validated: "{font}" (drift {drift}px)', {
    font: computedFont,
    drift: drift.toFixed(3),
  })

  return computedFont
}

export function createLineHeightMap() {
  let ready = $state(false)
  // Bumped every time the prefix-sum is rebuilt (by buildPrefixSum or reset).
  // Read by getters so Svelte's $derived expressions track changes to the underlying data.

  let version = $state(0)
  let generation = 0
  let preparedTexts: PreparedText[] = []
  let cumHeight: Float64Array = new Float64Array(0)
  let currentFont: string | null = null
  let currentMaxWidth = 0
  let currentLayoutFn: typeof import('@chenglou/pretext').layout | null = null

  function reset() {
    ready = false
    // No version++ here — when ready is false, getters return 0 regardless.
    // Bumping version from inside cancel() (called by effects) would create
    // an infinite reactive loop: effect → cancel → version++ → $derived dirty → effect.
    preparedTexts = []
    cumHeight = new Float64Array(0)
    currentFont = null
    currentMaxWidth = 0
    currentLayoutFn = null
  }

  /**
   * Builds the prefix-sum array from prepared texts at the given width.
   * cumHeight[i] = sum of heights for lines 0..i-1, so:
   * - cumHeight[0] = 0
   * - cumHeight[n] = total height through line n-1
   * - cumHeight[lines.length] = total height
   */
  function buildPrefixSum(maxWidth: number) {
    if (!currentLayoutFn) return
    const n = preparedTexts.length
    const sums = new Float64Array(n + 1)
    let acc = 0
    for (let i = 0; i < n; i++) {
      sums[i] = acc
      const result = currentLayoutFn(preparedTexts[i], maxWidth, LINE_HEIGHT)
      acc += result.height
    }
    sums[n] = acc
    cumHeight = sums
    currentMaxWidth = maxWidth
    version++
  }

  /** O(1) — returns the Y offset of the top edge of line n. */
  function getLineTop(n: number): number {
    void version // Reactive dependency — ensures $derived expressions recompute after reflow
    if (!ready || n < 0) return 0
    if (n >= cumHeight.length) return cumHeight[cumHeight.length - 1]
    return cumHeight[n]
  }

  /** O(log n) binary search — returns the line index at scroll position y. */
  function getLineAtPosition(y: number): number {
    void version // Reactive dependency — ensures $derived expressions recompute after reflow
    if (!ready || cumHeight.length <= 1) return 0
    const maxLine = cumHeight.length - 2 // last valid line index
    if (y <= 0) return 0
    if (y >= cumHeight[cumHeight.length - 1]) return maxLine

    // Binary search: find the largest i where cumHeight[i] <= y
    let lo = 0
    let hi = maxLine
    while (lo < hi) {
      const mid = (lo + hi + 1) >>> 1
      if (cumHeight[mid] <= y) {
        lo = mid
      } else {
        hi = mid - 1
      }
    }
    return lo
  }

  /** Returns the total height of all lines. */
  function getTotalHeight(): number {
    void version // Reactive dependency — ensures $derived expressions recompute after reflow
    if (!ready || cumHeight.length === 0) return 0
    return cumHeight[cumHeight.length - 1]
  }

  /**
   * Re-runs layout() on all prepared texts with a new width and rebuilds the
   * prefix-sum. This is fast (~0.0002ms per line) because prepare() data is cached.
   */
  function reflow(newWidth: number) {
    if (!ready || preparedTexts.length === 0) return
    if (newWidth === currentMaxWidth) return
    buildPrefixSum(newWidth)
  }

  /**
   * Asynchronously prepares all lines via requestIdleCallback.
   * Uses a generation counter to discard stale preparations.
   * Waits for the pretext dynamic import before starting work.
   */
  function prepareLines(lines: string[], maxWidth: number) {
    generation++
    const thisGeneration = generation
    ready = false

    if (lines.length === 0) return
    if (lines.length > MAX_LINES) {
      log.debug('Skipping line height map: {count} lines exceeds limit of {max}', {
        count: lines.length,
        max: MAX_LINES,
      })
      return
    }

    // Wait for pretext to load, then start preparation
    void pretextReady.then(() => {
      if (thisGeneration !== generation) return // stale
      if (!pretextModule) return // pretext failed to load

      const { prepare, layout: layoutFn } = pretextModule

      const resolvedFont = resolveAndValidateFont()
      if (!resolvedFont) return

      const font: string = resolvedFont
      currentFont = font
      // Stash layoutFn for reflow
      currentLayoutFn = layoutFn
      const prepared: PreparedText[] = new Array<PreparedText>(lines.length)
      let index = 0
      const startTime = performance.now()

      function processBatch(deadline: IdleDeadline) {
        if (thisGeneration !== generation) return // stale

        while (index < lines.length) {
          if (performance.now() - startTime > PREPARE_TIMEOUT_MS) {
            log.warn('Line height preparation timed out after {ms}ms at line {index}/{total}', {
              ms: PREPARE_TIMEOUT_MS,
              index,
              total: lines.length,
            })
            return // abandon — ready stays false
          }

          prepared[index] = prepare(lines[index], font, { whiteSpace: 'pre-wrap' })
          index++

          // Yield back to the browser if we've used up the idle time
          if (deadline.timeRemaining() < 1) {
            scheduleIdle(processBatch)
            return
          }
        }

        // All lines prepared — check generation is still current
        if (thisGeneration !== generation) return

        preparedTexts = prepared
        buildPrefixSum(maxWidth)
        ready = true

        log.info('Line height map ready: {count} lines, total height {height}px, prepared in {ms}ms', {
          count: lines.length,
          height: getTotalHeight().toFixed(0),
          ms: (performance.now() - startTime).toFixed(1),
        })
      }

      scheduleIdle(processBatch)
    })
  }

  /** Increments the generation counter, discarding any in-flight preparation. */
  function cancel() {
    generation++
    reset()
  }

  return {
    get ready() {
      return ready
    },
    get font() {
      return currentFont
    },
    getLineTop,
    getLineAtPosition,
    getTotalHeight,
    reflow,
    prepareLines,
    cancel,
  }
}
