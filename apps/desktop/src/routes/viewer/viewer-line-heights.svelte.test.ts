import { describe, it, expect } from 'vitest'
import { createLineHeightMap, getLineHeight, type LineHeightMeasurer } from './viewer-line-heights.svelte'

// Run scheduled work synchronously so tests don't juggle timers. The production
// default defers the (blocking) measure pass to an idle callback.
const runNow = (cb: () => void) => {
  cb()
}

/** A measurer that returns a preset height per line, ignoring width. */
function presetMeasure(heights: number[]): LineHeightMeasurer {
  return (lines) => Float64Array.from(lines.map((_, i) => heights[i] ?? 0))
}

/** A width-sensitive measurer: every line is exactly `maxWidth` px tall. Lets a
 *  test prove reflow re-measures at the new width. */
const widthAsHeightMeasure: LineHeightMeasurer = (lines, maxWidth) => Float64Array.from(lines.map(() => maxWidth))

describe('createLineHeightMap', () => {
  const minH = getLineHeight() // 18 at scale 1

  it('is not ready before prepareLines and reports zero positions', () => {
    const map = createLineHeightMap({ measure: presetMeasure([50]), schedule: runNow })
    expect(map.ready).toBe(false)
    expect(map.getLineTop(3)).toBe(0)
    expect(map.getTotalHeight()).toBe(0)
    expect(map.getLineAtPosition(999)).toBe(0)
  })

  it('builds a prefix sum of variable heights and answers positions in O(1)/O(log n)', () => {
    // Real-world shape: some lines wrap to many rows, some to one.
    const map = createLineHeightMap({ measure: presetMeasure([18, 180, 18, 90]), schedule: runNow })
    map.prepareLines(['a', 'b', 'c', 'd'], 800)
    expect(map.ready).toBe(true)

    // cumulative tops: 0, 18, 198, 216 ; total 306
    expect(map.getLineTop(0)).toBe(0)
    expect(map.getLineTop(1)).toBe(18)
    expect(map.getLineTop(2)).toBe(198)
    expect(map.getLineTop(3)).toBe(216)
    expect(map.getTotalHeight()).toBe(306)

    // getLineAtPosition: largest line whose top <= y
    expect(map.getLineAtPosition(0)).toBe(0)
    expect(map.getLineAtPosition(17)).toBe(0)
    expect(map.getLineAtPosition(18)).toBe(1)
    expect(map.getLineAtPosition(197)).toBe(1)
    expect(map.getLineAtPosition(198)).toBe(2)
    expect(map.getLineAtPosition(215)).toBe(2)
    expect(map.getLineAtPosition(216)).toBe(3)
    expect(map.getLineAtPosition(100_000)).toBe(3) // clamped to last line
  })

  it('clamps each line to at least the minimum line height (empty rows keep the gutter open)', () => {
    // Pretext/DOM report 0 for an empty line, but the row still renders one line tall.
    const map = createLineHeightMap({ measure: presetMeasure([0, 5, 40]), schedule: runNow })
    map.prepareLines(['', ' ', 'wraps'], 800)
    expect(map.getLineTop(1)).toBe(minH) // 0 -> minH
    expect(map.getLineTop(2)).toBe(minH * 2) // 5 -> minH
    expect(map.getTotalHeight()).toBe(minH * 2 + 40) // 40 stays
  })

  it('reflow re-measures at the new width and rebuilds the prefix sum', () => {
    const map = createLineHeightMap({ measure: widthAsHeightMeasure, schedule: runNow })
    map.prepareLines(['a', 'b'], 100)
    expect(map.getTotalHeight()).toBe(200) // 2 * 100

    map.reflow(250)
    expect(map.getTotalHeight()).toBe(500) // 2 * 250

    // No-op when width is unchanged.
    map.reflow(250)
    expect(map.getTotalHeight()).toBe(500)
  })

  it('recomputeForLineHeightChange re-measures at the current width', () => {
    let scale = 100
    const measure: LineHeightMeasurer = (lines) => Float64Array.from(lines.map(() => scale))
    const map = createLineHeightMap({ measure, schedule: runNow })
    map.prepareLines(['a', 'b'], 800)
    expect(map.getTotalHeight()).toBe(200)

    scale = 130 // simulate a font-scale settle changing row heights
    map.recomputeForLineHeightChange()
    expect(map.getTotalHeight()).toBe(260)
  })

  it('cancel discards the map and drops back to not-ready', () => {
    const map = createLineHeightMap({ measure: presetMeasure([50, 50]), schedule: runNow })
    map.prepareLines(['a', 'b'], 800)
    expect(map.ready).toBe(true)
    map.cancel()
    expect(map.ready).toBe(false)
    expect(map.getTotalHeight()).toBe(0)
  })

  it('a stale (superseded) preparation does not flip ready', () => {
    // Capture each scheduled callback so we can fire an old one after a newer prepare.
    const scheduled: Array<() => void> = []
    const deferred = (cb: () => void) => {
      scheduled.push(cb)
    }
    const map = createLineHeightMap({ measure: presetMeasure([50]), schedule: deferred })
    map.prepareLines(['a'], 800)
    // A newer preparation supersedes the first before it runs.
    map.prepareLines(['a'], 800)
    scheduled[0]() // fire the old, now-stale callback
    expect(map.ready).toBe(false)
  })

  it('skips the map for empty input and for files past the line cap', () => {
    const map = createLineHeightMap({ measure: presetMeasure([50]), schedule: runNow })
    map.prepareLines([], 800)
    expect(map.ready).toBe(false)

    const many = Array.from({ length: 50_001 }, () => 'x')
    map.prepareLines(many, 800)
    expect(map.ready).toBe(false)
  })
})
