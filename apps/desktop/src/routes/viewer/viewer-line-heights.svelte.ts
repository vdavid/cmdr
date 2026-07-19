import { getAppLogger } from '$lib/logging/logger'
import { getEffectiveScale } from '$lib/text-size.svelte'
import { pluralize } from '$lib/utils/pluralize'

const log = getAppLogger('viewer')

/**
 * Base viewer line height at scale 1, in CSS pixels. The CSS rule
 * `.line { height: calc(18px * var(--font-scale)) }` and `getLineHeight()`
 * (below) are paired. Keep them in sync if you change the base.
 */
const LINE_HEIGHT_BASE = 18

/**
 * Returns the viewer line height in CSS pixels at the current effective text
 * scale. Use this everywhere instead of a constant: the value changes when
 * the user moves the text-size slider or macOS Accessibility settles.
 *
 * Read inside Svelte `$derived`/`$effect` to track scale changes.
 */
export function getLineHeight(): number {
  return Math.max(1, Math.round(LINE_HEIGHT_BASE * getEffectiveScale()))
}

const MAX_LINES = 50_000

// WebKit in Tauri doesn't have requestIdleCallback. Fall back to setTimeout.
const scheduleIdle: (cb: () => void) => void =
  typeof requestIdleCallback === 'function'
    ? (cb) => {
        requestIdleCallback(() => {
          cb()
        })
      }
    : (cb) => {
        setTimeout(cb, 1)
      }

/**
 * Measures the rendered pixel height of each line when wrapped at `maxWidth`.
 * Returns one raw height per line (0 for an empty line is fine; the map clamps
 * to the minimum row height). `maxWidth` is the text column width: the scroll
 * container minus the gutter and row padding (see `viewer-text-width.svelte.ts`).
 */
export type LineHeightMeasurer = (lines: string[], maxWidth: number) => Float64Array

/**
 * DOM-truth measurer: lays every line out in a hidden probe that mirrors the
 * real `.line` (a `display:flex` row with a `.line-text` flex item) and reads
 * the wrapped height. This is the source of truth because WebKit's own layout is
 * what the viewer renders (see the flex note on the row loop). A canvas /
 * `measureText` predictor (e.g. pretext) can't match it for arbitrary bytes:
 * control, zero-width, combining, and undefined-glyph characters get advances
 * from the font metrics that diverge from what WebKit paints, so predicted wrap
 * row counts drift and the virtual-scroll positions rot (blank gaps, vanishing
 * lines). Measuring observes the wrap instead of predicting it, so it's correct
 * for binary, emoji, CJK, RTL, and ligatures alike.
 *
 * One offscreen layout + read for the whole file (~70 ms for ~2.3k lines);
 * deferred to an idle callback so it never blocks first paint.
 */
function measureLineHeightsViaDom(lines: string[], maxWidth: number): Float64Array {
  const n = lines.length
  const heights = new Float64Array(n)
  if (typeof document === 'undefined' || n === 0) return heights

  const host = document.createElement('div')
  host.setAttribute('aria-hidden', 'true')
  host.style.cssText = `position:absolute;left:-99999px;top:0;visibility:hidden;contain:layout style;width:${String(maxWidth)}px;`

  // Each row mirrors the real `.line` EXACTLY: a `display:flex` row (the text
  // column width) with the `.line-text` as a flex item. This matters because a
  // flex item's `min-width:auto` makes `overflow-wrap:break-word` ineffective on
  // an unbreakable run (no spaces): the real viewer renders such a run on one
  // row, overflowing, NOT wrapped. A plain-block probe would wrap it and
  // over-count the height, drifting the scroll. Replicating the flex context
  // makes the probe wrap identically to what's on screen.
  const rows: HTMLDivElement[] = new Array<HTMLDivElement>(n)
  const frag = document.createDocumentFragment()
  for (let i = 0; i < n; i++) {
    const row = document.createElement('div')
    row.style.cssText = 'display:flex'
    const text = document.createElement('div')
    text.style.cssText =
      'font-family:var(--font-mono);font-size:var(--font-size-sm);line-height:1.5;white-space:pre-wrap;overflow-wrap:break-word;'
    text.textContent = lines[i]
    row.appendChild(text)
    rows[i] = row
    frag.appendChild(row)
  }
  host.appendChild(frag)
  document.body.appendChild(host)

  // First read forces one layout pass; the rest are cheap reads off it.
  for (let i = 0; i < n; i++) {
    heights[i] = rows[i].getBoundingClientRect().height
  }

  document.body.removeChild(host)
  return heights
}

interface LineHeightMapOptions {
  /** Height source. Defaults to the DOM measurer; tests inject a deterministic one. */
  measure?: LineHeightMeasurer
  /** Defers the (blocking) measure pass. Defaults to an idle callback; tests run it now. */
  schedule?: (cb: () => void) => void
}

export function createLineHeightMap(options: LineHeightMapOptions = {}) {
  const measure = options.measure ?? measureLineHeightsViaDom
  const schedule = options.schedule ?? scheduleIdle

  let ready = $state(false)
  // Bumped every time the prefix-sum is rebuilt (by buildPrefixSum or reset).
  // Read by getters so Svelte's $derived expressions track changes to the underlying data.
  let version = $state(0)
  let generation = 0
  let currentLines: string[] = []
  let cumHeight: Float64Array = new Float64Array(0)
  let currentMaxWidth = 0

  function reset() {
    ready = false
    // No version++ here: when ready is false, getters return 0 regardless.
    // Bumping version from inside cancel() (called by effects) would create
    // an infinite reactive loop: effect -> cancel -> version++ -> $derived dirty -> effect.
    currentLines = []
    cumHeight = new Float64Array(0)
    currentMaxWidth = 0
  }

  /**
   * Measures every line at `maxWidth` and builds the prefix-sum array, where
   * cumHeight[i] = sum of heights for lines 0..i-1, so:
   * - cumHeight[0] = 0
   * - cumHeight[n] = total height through line n-1
   * - cumHeight[lines.length] = total height
   *
   * Each line is clamped to at least one row: the DOM renders every `.line` at
   * least one line tall (the gutter number keeps the row open) even when the
   * text is empty, so the prefix sum must match what's on screen.
   */
  function buildPrefixSum(maxWidth: number) {
    const heights = measure(currentLines, maxWidth)
    const n = currentLines.length
    const sums = new Float64Array(n + 1)
    const minHeight = getLineHeight()
    let acc = 0
    for (let i = 0; i < n; i++) {
      sums[i] = acc
      acc += Math.max(heights[i], minHeight)
    }
    sums[n] = acc
    cumHeight = sums
    currentMaxWidth = maxWidth
    version++
  }

  /** O(1): returns the Y offset of the top edge of line n. */
  function getLineTop(n: number): number {
    void version // Reactive dependency: ensures $derived expressions recompute after reflow
    if (!ready || n < 0) return 0
    if (n >= cumHeight.length) return cumHeight[cumHeight.length - 1]
    return cumHeight[n]
  }

  /** O(log n) binary search: returns the line index at scroll position y. */
  function getLineAtPosition(y: number): number {
    void version // Reactive dependency: ensures $derived expressions recompute after reflow
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
    void version // Reactive dependency: ensures $derived expressions recompute after reflow
    if (!ready || cumHeight.length === 0) return 0
    return cumHeight[cumHeight.length - 1]
  }

  /**
   * Re-measures all lines at a new width and rebuilds the prefix-sum. The caller
   * debounces this to the resize-settle: a full DOM re-measure is ~70 ms, too
   * slow to run on every ResizeObserver frame during a live drag.
   */
  function reflow(newWidth: number) {
    if (!ready || currentLines.length === 0) return
    if (newWidth === currentMaxWidth) return
    buildPrefixSum(newWidth)
  }

  /**
   * Force a re-measure at the current width, used when the line height itself
   * changed (e.g. text-size slider settled) but the container width hasn't.
   * Row heights scale with the font, so the prefix sum needs rebuilding.
   */
  function recomputeForLineHeightChange() {
    if (!ready || currentLines.length === 0) return
    buildPrefixSum(currentMaxWidth)
  }

  /**
   * Schedules an idle measure pass for all lines and flips `ready` when done.
   * A generation counter discards a preparation that a newer one superseded.
   */
  function prepareLines(lines: string[], maxWidth: number) {
    generation++
    const thisGeneration = generation
    ready = false

    if (lines.length === 0) return
    if (lines.length > MAX_LINES) {
      log.debug('Skipping line height map: {count} {linesNoun} exceeds limit of {max}', {
        count: lines.length,
        linesNoun: pluralize(lines.length, 'line'),
        max: MAX_LINES,
      })
      return
    }

    schedule(() => {
      if (thisGeneration !== generation) return // superseded
      const startTime = performance.now()
      currentLines = lines
      buildPrefixSum(maxWidth)
      ready = true
      log.info('Line height map ready: {count} {linesNoun}, total height {height}px, measured in {ms}ms', {
        count: lines.length,
        linesNoun: pluralize(lines.length, 'line'),
        height: getTotalHeight().toFixed(0),
        ms: (performance.now() - startTime).toFixed(1),
      })
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
    getLineTop,
    getLineAtPosition,
    getTotalHeight,
    reflow,
    recomputeForLineHeightChange,
    prepareLines,
    cancel,
  }
}
