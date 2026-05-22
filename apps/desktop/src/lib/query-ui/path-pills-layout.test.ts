/**
 * Round 2 R2: tests that pin the PathPills fitting algorithm. David reported the strip
 * collapsing into `…` even when there's clearly free space in the row — the per-pill
 * chrome safety margin was too conservative. These tests use a deterministic mock
 * measurer (1 px per character) so we can assert exact behaviour.
 */
import { describe, it, expect, vi } from 'vitest'
import {
  computePathPillsLayout,
  scheduleStableWidthMeasure,
  type Segment,
  type ReMeasureScheduler,
} from './path-pills-layout'

function seg(label: string, fullPath: string): Segment {
  return { label, fullPath }
}

const segs = [
  seg('Users', '/Users'),
  seg('dave', '/Users/dave'),
  seg('projects', '/Users/dave/projects'),
  seg('cmdr', '/Users/dave/projects/cmdr'),
  seg('src', '/Users/dave/projects/cmdr/src'),
  seg('lib', '/Users/dave/projects/cmdr/src/lib'),
]

const charMeasure = (s: string) => s.length // 1 px per char

const chrome = 4 // small chrome budget per R2 (was 16 px before the fix)
const sep = 5 // synthetic separator width: 1 px text + 4 px gap

describe('computePathPillsLayout (R2)', () => {
  it('renders everything uncollapsed when measurement is not ready', () => {
    const out = computePathPillsLayout(segs, {
      containerWidth: 1000,
      measureWidth: null,
      separatorWidth: sep,
      pillChrome: chrome,
    })
    expect(out.leading).toHaveLength(segs.length)
    expect(out.collapsed).toHaveLength(0)
    expect(out.trailing).toHaveLength(0)
  })

  it('renders everything uncollapsed when the container is wider than the strip', () => {
    // Strip text width: 5+4+8+4+3+3 = 27 chars, +6×4 chrome, +5×5 separators = 27 + 24 + 25 = 76.
    const out = computePathPillsLayout(segs, {
      containerWidth: 200,
      measureWidth: charMeasure,
      separatorWidth: sep,
      pillChrome: chrome,
    })
    expect(out.leading.map((s) => s.label)).toEqual(['Users', 'dave', 'projects', 'cmdr', 'src', 'lib'])
    expect(out.collapsed).toHaveLength(0)
    expect(out.trailing).toHaveLength(0)
  })

  it('R2: does NOT collapse when the strip fits with a modest chrome budget', () => {
    // With the round-1 chrome of 16 px, the strip width would be 27 + 96 + 25 = 148 px,
    // overshooting the available 100 px and triggering a collapse. With chrome = 4 the
    // strip fits at 76 px and we render everything.
    const out = computePathPillsLayout(segs, {
      containerWidth: 100,
      measureWidth: charMeasure,
      separatorWidth: sep,
      pillChrome: 4,
    })
    expect(out.collapsed).toHaveLength(0)
    expect(out.trailing).toHaveLength(0)
    expect(out.leading.map((s) => s.label)).toEqual(['Users', 'dave', 'projects', 'cmdr', 'src', 'lib'])
  })

  it('collapses the middle pills behind … when the strip genuinely overflows', () => {
    // Force a tight container so only first + … + last fits.
    // first 'Users' (5+4=9) + sep(5) + '…' (1+4=5) + sep(5) + 'lib' (3+4=7) = 31.
    const out = computePathPillsLayout(segs, {
      containerWidth: 35,
      measureWidth: charMeasure,
      separatorWidth: sep,
      pillChrome: 4,
    })
    expect(out.leading.map((s) => s.label)).toEqual(['Users'])
    expect(out.trailing.map((s) => s.label)).toEqual(['lib'])
    expect(out.collapsed.map((s) => s.label)).toEqual(['dave', 'projects', 'cmdr', 'src'])
  })

  it('adds back trailing-side pills while they fit', () => {
    // Big enough for Users + … + src + lib but not the full strip.
    // 'Users' 9 + sep 5 + '…' 5 + sep 5 + 'src' 7 + sep 5 + 'lib' 7 = 43.
    const out = computePathPillsLayout(segs, {
      containerWidth: 45,
      measureWidth: charMeasure,
      separatorWidth: sep,
      pillChrome: 4,
    })
    expect(out.leading.map((s) => s.label)).toEqual(['Users'])
    expect(out.collapsed.map((s) => s.label)).toEqual(['dave', 'projects', 'cmdr'])
    expect(out.trailing.map((s) => s.label)).toEqual(['src', 'lib'])
  })

  it('drops the first pill but keeps … and trailing pills when even first+…+last overflows', () => {
    // Tiny container: only 'lib' fits.
    const out = computePathPillsLayout(segs, {
      containerWidth: 8,
      measureWidth: charMeasure,
      separatorWidth: sep,
      pillChrome: 4,
    })
    expect(out.leading).toHaveLength(0)
    expect(out.trailing.map((s) => s.label)).toEqual(['lib'])
    // The first segment moves into collapsed (so it's still reachable via the tooltip).
    expect(out.collapsed[0]?.label).toBe('Users')
  })

  // R3 B4: pin the re-measure helper that fixes the "first the full path,
  // then collapses back" race. The contract: after the initial read, schedule
  // exactly one extra read on the next animation frame and one more ~80ms
  // later. Both follow-up reads must fire so we catch both the grid-track-
  // settling moment and the late style-recalculation moment.
  it('R3 B4: scheduleStableWidthMeasure fires re-read on next frame and ~80ms later', () => {
    const read = vi.fn()
    const cbs: { frame?: () => void; timer?: () => void } = {}
    let cancelledFrame = -1
    let clearedTimer: ReturnType<typeof setTimeout> | null = null
    const scheduler: ReMeasureScheduler = {
      requestFrame: (cb) => {
        cbs.frame = cb
        return 42
      },
      cancelFrame: (id) => {
        cancelledFrame = id
      },
      setTimer: (cb) => {
        cbs.timer = cb
        return 7 as unknown as ReturnType<typeof setTimeout>
      },
      clearTimer: (id) => {
        clearedTimer = id
      },
    }
    const cancel = scheduleStableWidthMeasure(read, scheduler)
    expect(read).toHaveBeenCalledTimes(0)
    cbs.frame?.()
    expect(read).toHaveBeenCalledTimes(1)
    cbs.timer?.()
    expect(read).toHaveBeenCalledTimes(2)
    // Cancel after the fact: both ids should flow through to the scheduler.
    cancel()
    expect(cancelledFrame).toBe(42)
    expect(clearedTimer).toBe(7 as unknown as ReturnType<typeof setTimeout>)
  })

  it('R3 B4: scheduleStableWidthMeasure cancel cleans up pending callbacks', () => {
    const read = vi.fn()
    let cancelledFrame = -1
    let clearedTimer: ReturnType<typeof setTimeout> | null = null
    const scheduler: ReMeasureScheduler = {
      requestFrame: () => 1,
      cancelFrame: (id) => {
        cancelledFrame = id
      },
      setTimer: () => 2 as unknown as ReturnType<typeof setTimeout>,
      clearTimer: (id) => {
        clearedTimer = id
      },
    }
    const cancel = scheduleStableWidthMeasure(read, scheduler)
    cancel()
    expect(cancelledFrame).toBe(1)
    expect(clearedTimer).toBe(2 as unknown as ReturnType<typeof setTimeout>)
    expect(read).toHaveBeenCalledTimes(0)
  })

  it('two-segment paths never collapse', () => {
    const out = computePathPillsLayout([seg('a', '/a'), seg('b', '/a/b')], {
      containerWidth: 1,
      measureWidth: charMeasure,
      separatorWidth: sep,
      pillChrome: 4,
    })
    expect(out.leading.map((s) => s.label)).toEqual(['a', 'b'])
    expect(out.collapsed).toHaveLength(0)
    expect(out.trailing).toHaveLength(0)
  })
})
