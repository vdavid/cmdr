import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import {
  getDiskUsageLevel,
  getUsedPercent,
  getUsageBar,
  formatDiskSpaceStatus,
  formatDiskSpaceShort,
  formatSpaceNotes,
  formatBarTooltip,
  type BoundedSpaceInfo,
} from './disk-space-utils'
import type { SpaceInfo } from '$lib/ipc/bindings'

const mockFormatSize = (bytes: number): string => `${String(bytes)} B`

function createSpace(totalBytes: number, availableBytes: number): BoundedSpaceInfo {
  return { kind: 'bounded', totalBytes, availableBytes, usedBytes: totalBytes - availableBytes }
}

/** Storage with no ceiling: what a quota-less Nextcloud account reports. */
function createUnbounded(usedBytes: number): SpaceInfo {
  return { kind: 'unbounded', usedBytes }
}

// The sentences resolve through the i18n catalog (`tString`) and the percentage
// through `formatInteger`; pin the base locale so the asserted en copy and its
// digits are deterministic.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('getDiskUsageLevel', () => {
  it('returns OK for 0%', () => {
    const result = getDiskUsageLevel(0)
    expect(result.cssVar).toBe('--color-disk-ok')
    expect(result.severity).toBe('ok')
  })

  it('returns OK for 50%', () => {
    const result = getDiskUsageLevel(50)
    expect(result.cssVar).toBe('--color-disk-ok')
    expect(result.severity).toBe('ok')
  })

  it('returns OK for 79%', () => {
    const result = getDiskUsageLevel(79)
    expect(result.cssVar).toBe('--color-disk-ok')
    expect(result.severity).toBe('ok')
  })

  it('returns Warning for 80%', () => {
    const result = getDiskUsageLevel(80)
    expect(result.cssVar).toBe('--color-disk-warning')
    expect(result.severity).toBe('warning')
  })

  it('returns Warning for 94%', () => {
    const result = getDiskUsageLevel(94)
    expect(result.cssVar).toBe('--color-disk-warning')
    expect(result.severity).toBe('warning')
  })

  it('returns Critical for 95%', () => {
    const result = getDiskUsageLevel(95)
    expect(result.cssVar).toBe('--color-disk-danger')
    expect(result.severity).toBe('critical')
  })

  it('returns Critical for 100%', () => {
    const result = getDiskUsageLevel(100)
    expect(result.cssVar).toBe('--color-disk-danger')
    expect(result.severity).toBe('critical')
  })
})

describe('getUsedPercent', () => {
  it('calculates normal usage', () => {
    const space = createSpace(1000, 400)
    expect(getUsedPercent(space)).toBe(60)
  })

  it('returns 100 when no space available', () => {
    const space = createSpace(1000, 0)
    expect(getUsedPercent(space)).toBe(100)
  })

  it('returns 0 when all space available', () => {
    const space = createSpace(1000, 1000)
    expect(getUsedPercent(space)).toBe(0)
  })

  it('returns 0 when totalBytes is 0', () => {
    const space = createSpace(0, 0)
    expect(getUsedPercent(space)).toBe(0)
  })

  it('returns 0 when totalBytes is negative', () => {
    const space = createSpace(-100, 0)
    expect(getUsedPercent(space)).toBe(0)
  })

  it('handles very small volumes', () => {
    const space = createSpace(100, 1)
    expect(getUsedPercent(space)).toBe(99)
  })

  it('rounds to nearest integer', () => {
    // 333 of 1000 used = 33.3% -> 33
    const space = createSpace(1000, 667)
    expect(getUsedPercent(space)).toBe(33)
  })

  it('clamps to 0 when availableBytes exceeds totalBytes', () => {
    const space = createSpace(100, 200)
    expect(getUsedPercent(space)).toBe(0)
  })
})

describe('formatDiskSpaceStatus', () => {
  it('formats status text with free space and percentage', () => {
    const space = createSpace(1000, 420)
    const result = formatDiskSpaceStatus(space, mockFormatSize)
    expect(result).toBe('420 B of 1000 B free (42%)')
  })

  it('handles full disk', () => {
    const space = createSpace(1000, 0)
    const result = formatDiskSpaceStatus(space, mockFormatSize)
    expect(result).toBe('0 B of 1000 B free (0%)')
  })

  it('handles empty disk', () => {
    const space = createSpace(1000, 1000)
    const result = formatDiskSpaceStatus(space, mockFormatSize)
    expect(result).toBe('1000 B of 1000 B free (100%)')
  })
})

describe('formatDiskSpaceShort', () => {
  it('formats short text', () => {
    const space = createSpace(1000, 420)
    const result = formatDiskSpaceShort(space, mockFormatSize)
    expect(result).toBe('420 B free of 1000 B')
  })

  it('handles full disk', () => {
    const space = createSpace(1000, 0)
    const result = formatDiskSpaceShort(space, mockFormatSize)
    expect(result).toBe('0 B free of 1000 B')
  })
})

describe('formatBarTooltip', () => {
  it('shows sizes and percentage when space is OK', () => {
    const space = createSpace(1000, 400) // 60% used, 40% free
    expect(formatBarTooltip(space, mockFormatSize)).toBe('400 B of 1000 B free (40%)')
  })

  it('includes yellow warning when space is somewhat low', () => {
    const space = createSpace(1000, 100) // 90% used, 10% free
    expect(formatBarTooltip(space, mockFormatSize)).toBe(
      '100 B of 1000 B free (10%). This bar is yellow to indicate that the volume is somewhat low on space.',
    )
  })

  it('includes red warning when space is low', () => {
    const space = createSpace(1000, 20) // 98% used, 2% free
    expect(formatBarTooltip(space, mockFormatSize)).toBe(
      '20 B of 1000 B free (2%). This bar is red to indicate that the volume is low on space.',
    )
  })

  it('shows 100% free for empty disk', () => {
    const space = createSpace(1000, 1000)
    expect(formatBarTooltip(space, mockFormatSize)).toBe('1000 B of 1000 B free (100%)')
  })

  it('shows 0% free for full disk with red warning', () => {
    const space = createSpace(1000, 0)
    expect(formatBarTooltip(space, mockFormatSize)).toBe(
      '0 B of 1000 B free (0%). This bar is red to indicate that the volume is low on space.',
    )
  })

  it('uses the provided formatSize function', () => {
    const space = createSpace(1073741824, 536870912)
    const customFormat = (bytes: number): string => `${String(Math.round(bytes / 1073741824))} GB`
    expect(formatBarTooltip(space, customFormat)).toBe('1 GB of 1 GB free (50%)')
  })

  it('appends the extra hint after the sizes when space is OK', () => {
    const space = createSpace(1000, 400) // 60% used
    expect(formatBarTooltip(space, mockFormatSize, 'Phones hide app data.')).toBe(
      '400 B of 1000 B free (40%). Phones hide app data.',
    )
  })

  it('appends the extra hint after a low-space warning', () => {
    const space = createSpace(1000, 100) // 90% used → yellow
    expect(formatBarTooltip(space, mockFormatSize, 'Phones hide app data.')).toBe(
      '100 B of 1000 B free (10%). This bar is yellow to indicate that the volume is somewhat low on space. Phones hide app data.',
    )
  })

  it('omits the hint when none is provided', () => {
    const space = createSpace(1000, 400)
    expect(formatBarTooltip(space, mockFormatSize, undefined)).toBe('400 B of 1000 B free (40%)')
  })

  it('reports the same percentage the status bar does, even where the two roundings diverge', () => {
    // 60.5% used / 39.5% free. Rounding the USED half and subtracting gives 39;
    // rounding the free half gives 40. The tooltip used to do the former and the
    // status bar the latter, so the same volume read two ways at once.
    const space = createSpace(1000, 395)
    expect(formatBarTooltip(space, mockFormatSize)).toBe(formatDiskSpaceStatus(space, mockFormatSize))
    expect(formatBarTooltip(space, mockFormatSize)).toBe('395 B of 1000 B free (40%)')
  })
})

// ── Storage with no ceiling ──────────────────────────────────────────
//
// ❗ The shape a stock Nextcloud account reports, which is the COMMON case for
// real users rather than an edge one. Everything here is about NOT inventing a
// denominator: no bar, no percentage, and above all no warning band, because you
// can't run out of storage that has no limit.

describe('storage with no ceiling', () => {
  it('has no bar to draw', () => {
    expect(getUsageBar(createUnbounded(64_000_000))).toBeNull()
  })

  it('still draws a bar wherever a total exists', () => {
    expect(getUsageBar(createSpace(1000, 400))).toEqual({
      usedPercent: 60,
      severity: 'ok',
      cssVar: '--color-disk-ok',
    })
  })

  it('states what is stored instead of what is free', () => {
    expect(formatDiskSpaceStatus(createUnbounded(64_000_000), mockFormatSize)).toBe('64000000 B used')
  })

  it('states the same thing in the narrow drive picker', () => {
    expect(formatDiskSpaceShort(createUnbounded(64_000_000), mockFormatSize)).toBe('64000000 B used')
  })

  it('never fires a low-space warning, however much is stored', () => {
    // The whole point. A band keyed off a percentage would compute 0 or NaN here
    // and could land anywhere; there is no honest band to be in.
    const notes = formatSpaceNotes(createUnbounded(999_999_999_999))
    expect(notes).not.toContain('low on space')
    expect(notes).toBe("This storage has no size limit, so there's no bar to fill.")
  })

  it('explains in the tooltip why there is no bar', () => {
    expect(formatBarTooltip(createUnbounded(64_000_000), mockFormatSize)).toBe(
      "64000000 B used. This storage has no size limit, so there's no bar to fill.",
    )
  })

  it('still carries the phone-storage hint after its own note', () => {
    expect(formatBarTooltip(createUnbounded(64_000_000), mockFormatSize, 'Phones hide app data.')).toBe(
      "64000000 B used. This storage has no size limit, so there's no bar to fill. Phones hide app data.",
    )
  })
})
