/**
 * Brief-mode per-column width measurement. Each column shrink-wraps to its
 * widest filename; the caller caps the result at whatever max width the
 * container + backend font metrics dictate. Uses `@chenglou/pretext` for
 * pixel-accurate text measurement without DOM reflow.
 */

import * as pretext from '@chenglou/pretext'

import type { FileEntry } from '../types'
import { createPretextMeasure } from '$lib/utils/shorten-middle'
import { getEffectiveScale, onDebouncedScaleChange } from '$lib/text-size.svelte'

/** Base font size of `.brief-list` at scale 1 — multiplied by effective text scale. */
const BASE_FONT_PX = 12

function buildFont(scale: number): string {
  const px = Math.max(1, Math.round(BASE_FONT_PX * scale))
  return `${String(px)}px -apple-system, BlinkMacSystemFont, sans-serif`
}

let measureWidthCached: ((text: string) => number) | null = null
let measureUnavailable = false
let cachedScale = 0

if (typeof window !== 'undefined') {
  onDebouncedScaleChange(() => {
    measureWidthCached = null
    measureUnavailable = false
    cachedScale = 0
  })
}

function getMeasure(): ((text: string) => number) | null {
  const scale = getEffectiveScale()
  if (measureWidthCached && scale === cachedScale) return measureWidthCached
  if (measureUnavailable && scale === cachedScale) return null
  if (typeof document === 'undefined') return null
  try {
    const candidate = createPretextMeasure(buildFont(scale), pretext)
    candidate('probe')
    measureWidthCached = candidate
    cachedScale = scale
    measureUnavailable = false
    return measureWidthCached
  } catch {
    measureUnavailable = true
    cachedScale = scale
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
