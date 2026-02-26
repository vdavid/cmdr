import { describe, it, expect } from 'vitest'
import {
    getDiskUsageLevel,
    getUsedPercent,
    formatDiskSpaceStatus,
    formatDiskSpaceShort,
    formatBarTooltip,
} from './disk-space-utils'
import type { VolumeSpaceInfo } from '$lib/tauri-commands/storage'

const mockFormatSize = (bytes: number): string => `${String(bytes)} B`

function createSpace(totalBytes: number, availableBytes: number): VolumeSpaceInfo {
    return { totalBytes, availableBytes }
}

describe('getDiskUsageLevel', () => {
    it('returns OK for 0%', () => {
        const result = getDiskUsageLevel(0)
        expect(result.cssVar).toBe('--color-disk-ok')
        expect(result.label).toBe('OK')
    })

    it('returns OK for 50%', () => {
        const result = getDiskUsageLevel(50)
        expect(result.cssVar).toBe('--color-disk-ok')
        expect(result.label).toBe('OK')
    })

    it('returns OK for 79%', () => {
        const result = getDiskUsageLevel(79)
        expect(result.cssVar).toBe('--color-disk-ok')
        expect(result.label).toBe('OK')
    })

    it('returns Warning for 80%', () => {
        const result = getDiskUsageLevel(80)
        expect(result.cssVar).toBe('--color-disk-warning')
        expect(result.label).toBe('Warning')
    })

    it('returns Warning for 94%', () => {
        const result = getDiskUsageLevel(94)
        expect(result.cssVar).toBe('--color-disk-warning')
        expect(result.label).toBe('Warning')
    })

    it('returns Critical for 95%', () => {
        const result = getDiskUsageLevel(95)
        expect(result.cssVar).toBe('--color-disk-danger')
        expect(result.label).toBe('Critical')
    })

    it('returns Critical for 100%', () => {
        const result = getDiskUsageLevel(100)
        expect(result.cssVar).toBe('--color-disk-danger')
        expect(result.label).toBe('Critical')
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
            "100 B of 1000 B free (10%). This bar is yellow to indicate that it's somewhat low on space.",
        )
    })

    it('includes red warning when space is low', () => {
        const space = createSpace(1000, 20) // 98% used, 2% free
        expect(formatBarTooltip(space, mockFormatSize)).toBe(
            "20 B of 1000 B free (2%). This bar is red to indicate that it's low on space.",
        )
    })

    it('shows 100% free for empty disk', () => {
        const space = createSpace(1000, 1000)
        expect(formatBarTooltip(space, mockFormatSize)).toBe('1000 B of 1000 B free (100%)')
    })

    it('shows 0% free for full disk with red warning', () => {
        const space = createSpace(1000, 0)
        expect(formatBarTooltip(space, mockFormatSize)).toBe(
            "0 B of 1000 B free (0%). This bar is red to indicate that it's low on space.",
        )
    })

    it('uses the provided formatSize function', () => {
        const space = createSpace(1073741824, 536870912)
        const customFormat = (bytes: number): string => `${String(Math.round(bytes / 1073741824))} GB`
        expect(formatBarTooltip(space, customFormat)).toBe('1 GB of 1 GB free (50%)')
    })
})
