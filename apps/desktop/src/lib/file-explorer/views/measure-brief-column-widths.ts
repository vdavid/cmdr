/**
 * Brief-mode per-column width measurement. Each column shrink-wraps to its
 * widest filename; the caller caps the result at whatever max width the
 * container + backend font metrics dictate. Uses `@chenglou/pretext` for
 * pixel-accurate text measurement without DOM reflow.
 */

import * as pretext from '@chenglou/pretext'

import type { FileEntry } from '../types'
import { createPretextMeasure } from '$lib/utils/shorten-middle'

/**
 * CSS `font` shorthand matching `.brief-list` (`var(--font-system)` at 12px).
 * Kept in sync with `apps/desktop/src/app.css` — pretext warns `system-ui`
 * is unsafe for layout accuracy, so we lead with `-apple-system`.
 */
const FONT = '12px -apple-system, BlinkMacSystemFont, sans-serif'

let measureWidthCached: ((text: string) => number) | null = null
let measureUnavailable = false

function getMeasure(): ((text: string) => number) | null {
  if (measureWidthCached) return measureWidthCached
  if (measureUnavailable) return null
  if (typeof document === 'undefined') return null
  try {
    const candidate = createPretextMeasure(FONT, pretext)
    candidate('probe')
    measureWidthCached = candidate
    return measureWidthCached
  } catch {
    measureUnavailable = true
    return null
  }
}

/** Exposed for tests to inject a fake measurer. */
export function _setBriefMeasureForTests(fn: ((text: string) => number) | null): void {
  measureWidthCached = fn
  measureUnavailable = false
}

/**
 * Returns the widest filename's pixel width across `files`. The caller is
 * responsible for adding icon/gap/padding chrome and clamping between min
 * and max column widths.
 *
 * Returns 0 when no measurer is available (SSR or jsdom without canvas) —
 * the caller should fall back to the default cap in that case.
 */
export function measureWidestFilename(files: FileEntry[]): number {
  const measure = getMeasure()
  if (!measure) return 0
  let max = 0
  for (const f of files) {
    const w = measure(f.name)
    if (w > max) max = w
  }
  return max
}
