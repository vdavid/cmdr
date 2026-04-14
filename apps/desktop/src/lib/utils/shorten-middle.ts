import type { PreparedTextWithSegments } from '@chenglou/pretext'

export interface ShortenMiddleOptions {
  /** Snap truncation to nearest occurrence of this character (for example, '/' for paths). */
  preferBreakAt?: string
  /** How much of the width budget goes to the start portion. 0–1, default 0.5. */
  startRatio?: number
  /** Ellipsis string to insert. Default: '…' */
  ellipsis?: string
}

/**
 * Truncates text in the middle, keeping both the start and end visible.
 * Uses the injected `measureWidth` for pixel-accurate measurement.
 */
export function shortenMiddle(
  text: string,
  maxWidthPx: number,
  measureWidth: (text: string) => number,
  options?: ShortenMiddleOptions,
): string {
  const ellipsis = options?.ellipsis ?? '…'
  const startRatio = options?.startRatio ?? 0.5
  const preferBreakAt = options?.preferBreakAt

  const textWidth = measureWidth(text)
  if (textWidth <= maxWidthPx) return text

  const ellipsisWidth = measureWidth(ellipsis)
  // If the text is no wider than the ellipsis, truncating can't help — return as-is.
  if (textWidth <= ellipsisWidth) return text
  // If even the ellipsis alone exceeds the budget, return it as a best-effort indicator.
  if (ellipsisWidth >= maxWidthPx) return ellipsis

  const availableBudget = maxWidthPx - ellipsisWidth
  const startBudget = availableBudget * startRatio
  const endBudget = availableBudget - startBudget

  // Binary search for the longest prefix fitting startBudget
  let startLen = findLongestFitting(0, text.length, startBudget, (len) => measureWidth(text.slice(0, len)))

  // Binary search for the longest suffix fitting endBudget
  let endLen = findLongestFitting(0, text.length, endBudget, (len) => measureWidth(text.slice(text.length - len)))

  // Snap to preferBreakAt boundaries if configured
  if (preferBreakAt && preferBreakAt.length > 0) {
    startLen = snapToBreak(text, startLen, preferBreakAt, 'start', startBudget, measureWidth)
    endLen = snapToBreak(text, endLen, preferBreakAt, 'end', endBudget, measureWidth)
  }

  return text.slice(0, startLen) + ellipsis + text.slice(text.length - endLen)
}

/**
 * Binary search for the longest count of characters (from start or end) whose
 * measured width fits within `budget`.
 */
function findLongestFitting(lo: number, hi: number, budget: number, measure: (len: number) => number): number {
  let bestLen = lo
  while (lo <= hi) {
    const mid = (lo + hi) >>> 1
    if (measure(mid) <= budget) {
      bestLen = mid
      lo = mid + 1
    } else {
      hi = mid - 1
    }
  }
  return bestLen
}

const minBudgetUsageRatio = 0.4

/**
 * Snaps a cut length inward to the nearest `breakChar` boundary.
 * Only snaps if the snapped length still uses at least 40% of its sub-budget.
 */
function snapToBreak(
  text: string,
  rawLen: number,
  breakChar: string,
  side: 'start' | 'end',
  budget: number,
  measureWidth: (text: string) => number,
): number {
  if (rawLen === 0) return 0

  if (side === 'start') {
    // Search inward (backward) from rawLen for the nearest breakChar
    const region = text.slice(0, rawLen)
    const breakIdx = region.lastIndexOf(breakChar)
    if (breakIdx < 0) return rawLen

    // Snap to just after the break char
    const snappedLen = breakIdx + 1
    const snappedWidth = measureWidth(text.slice(0, snappedLen))
    if (snappedWidth >= budget * minBudgetUsageRatio) return snappedLen
    return rawLen
  }

  // side === 'end': search inward (forward from the suffix start) for the nearest breakChar
  const suffixStart = text.length - rawLen
  const region = text.slice(suffixStart)
  const breakIdx = region.indexOf(breakChar)
  if (breakIdx < 0) return rawLen

  // Snap to start at the break char
  const snappedLen = rawLen - breakIdx
  const snappedWidth = measureWidth(text.slice(text.length - snappedLen))
  if (snappedWidth >= budget * minBudgetUsageRatio) return snappedLen
  return rawLen
}

/**
 * Creates a `measureWidth` function using pretext's pixel-accurate text shaping.
 * Requires a CSS font string (for example, '12px "SF Mono"') and the pretext module
 * (obtained via dynamic `import('@chenglou/pretext')`).
 */
export function createPretextMeasure(
  font: string,
  pretext: {
    prepareWithSegments: typeof import('@chenglou/pretext').prepareWithSegments
    measureNaturalWidth: typeof import('@chenglou/pretext').measureNaturalWidth
  },
): (text: string) => number {
  const cache = new Map<string, PreparedTextWithSegments>()

  return (text: string): number => {
    let prepared = cache.get(text)
    if (!prepared) {
      prepared = pretext.prepareWithSegments(text, font)
      cache.set(text, prepared)
    }
    return pretext.measureNaturalWidth(prepared)
  }
}
