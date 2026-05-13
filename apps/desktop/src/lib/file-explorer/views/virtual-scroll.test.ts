/**
 * Tests for virtual-scroll.ts
 */
import { describe, it, expect } from 'vitest'
import {
  calculateVirtualWindow,
  calculateVirtualWindowVariable,
  getScrollToPosition,
  getScrollToPositionVariable,
  type VirtualScrollConfig,
} from './virtual-scroll'

/** Helper: build a prefix-sum array from per-item widths. */
function prefixSumsFrom(widths: number[]): number[] {
  const sums = new Array<number>(widths.length + 1)
  sums[0] = 0
  for (let i = 0; i < widths.length; i++) {
    sums[i + 1] = sums[i] + widths[i]
  }
  return sums
}

describe('calculateVirtualWindow', () => {
  const baseConfig: VirtualScrollConfig = {
    direction: 'vertical',
    itemSize: 20,
    bufferSize: 5,
    containerSize: 400, // 20 items visible
    scrollOffset: 0,
    totalItems: 1000,
  }

  describe('basic calculations', () => {
    it('calculates correct start index at scroll position 0', () => {
      const result = calculateVirtualWindow(baseConfig)
      expect(result.startIndex).toBe(0)
    })

    it('calculates correct end index for viewport', () => {
      const result = calculateVirtualWindow(baseConfig)
      // 400px / 20px = 20 items visible + 5 buffer on each side = 30
      // But starts at 0 so no buffer before
      // End should be 0 + 20 (visible) + 5 (buffer) = 25
      expect(result.endIndex).toBeLessThanOrEqual(30)
      expect(result.endIndex).toBeGreaterThan(20)
    })

    it('returns correct visibleCount', () => {
      const result = calculateVirtualWindow(baseConfig)
      expect(result.visibleCount).toBe(result.endIndex - result.startIndex)
    })

    it('calculates correct totalSize', () => {
      const result = calculateVirtualWindow(baseConfig)
      expect(result.totalSize).toBe(1000 * 20)
    })

    it('calculates correct offset at scroll position 0', () => {
      const result = calculateVirtualWindow(baseConfig)
      expect(result.offset).toBe(0)
    })
  })

  describe('scrolled positions', () => {
    it('calculates correct range when scrolled partway', () => {
      const config = { ...baseConfig, scrollOffset: 200 } // Scrolled 10 items
      const result = calculateVirtualWindow(config)

      // First visible index = 200/20 = 10
      // startIndex = 10 - 5 = 5
      expect(result.startIndex).toBe(5)
    })

    it('applies buffer correctly when scrolled', () => {
      const config = { ...baseConfig, scrollOffset: 500 } // Scrolled 25 items
      const result = calculateVirtualWindow(config)

      // First visible index = 500/20 = 25
      // startIndex = 25 - 5 = 20
      expect(result.startIndex).toBe(20)
      // endIndex = 20 + 20 (visible) + 10 (buffer both sides) = 50 or clamped
      expect(result.endIndex).toBeGreaterThan(40)
    })

    it('calculates correct offset when scrolled', () => {
      const config = { ...baseConfig, scrollOffset: 500 }
      const result = calculateVirtualWindow(config)

      // offset = startIndex * itemSize
      expect(result.offset).toBe(result.startIndex * 20)
    })
  })

  describe('edge cases', () => {
    it('handles empty list', () => {
      const config = { ...baseConfig, totalItems: 0 }
      const result = calculateVirtualWindow(config)

      expect(result.startIndex).toBe(0)
      expect(result.endIndex).toBe(0)
      expect(result.visibleCount).toBe(0)
      expect(result.totalSize).toBe(0)
    })

    it('handles list smaller than viewport', () => {
      const config = { ...baseConfig, totalItems: 5 } // Only 5 items
      const result = calculateVirtualWindow(config)

      expect(result.startIndex).toBe(0)
      expect(result.endIndex).toBe(5)
      expect(result.visibleCount).toBe(5)
    })

    it('clamps startIndex to 0 with large buffer', () => {
      const config = { ...baseConfig, bufferSize: 20, scrollOffset: 100 }
      const result = calculateVirtualWindow(config)

      // First visible = 100/20 = 5
      // With buffer of 20, would be 5 - 20 = -15, clamped to 0
      expect(result.startIndex).toBe(0)
    })

    it('clamps endIndex to totalItems', () => {
      const config = { ...baseConfig, scrollOffset: 19500, totalItems: 1000 }
      const result = calculateVirtualWindow(config)

      expect(result.endIndex).toBe(1000)
    })

    it('handles scrollOffset near end of list', () => {
      const config = { ...baseConfig, scrollOffset: 19800, totalItems: 1000 }
      const result = calculateVirtualWindow(config)

      // First visible = 19800/20 = 990
      // startIndex = 990 - 5 = 985
      expect(result.startIndex).toBe(985)
      expect(result.endIndex).toBe(1000)
    })

    it('handles fractional scroll positions', () => {
      const config = { ...baseConfig, scrollOffset: 155 } // Not aligned to item size
      const result = calculateVirtualWindow(config)

      // First visible = floor(155/20) = 7
      // startIndex = 7 - 5 = 2
      expect(result.startIndex).toBe(2)
    })

    it('handles very large item size', () => {
      const config = { ...baseConfig, itemSize: 500, containerSize: 400 }
      const result = calculateVirtualWindow(config)

      // itemsInView = ceil(400/500) = 1
      // visibleCount = 1 + 5*2 = 11
      expect(result.visibleCount).toBeLessThanOrEqual(11)
    })

    it('handles buffer size of 0', () => {
      const config = { ...baseConfig, bufferSize: 0 }
      const result = calculateVirtualWindow(config)

      // No buffer means exactly the visible items
      expect(result.startIndex).toBe(0)
      // itemsInView = ceil(400/20) = 20
      expect(result.endIndex).toBe(20)
    })
  })

  describe('horizontal scrolling', () => {
    it('works the same for horizontal direction', () => {
      const config: VirtualScrollConfig = {
        direction: 'horizontal',
        itemSize: 150, // Column width
        bufferSize: 2,
        containerSize: 600, // 4 columns visible
        scrollOffset: 300, // 2 columns scrolled
        totalItems: 100,
      }
      const result = calculateVirtualWindow(config)

      // First visible = floor(300/150) = 2
      // startIndex = 2 - 2 = 0
      expect(result.startIndex).toBe(0)

      // itemsInView = ceil(600/150) = 4
      // visibleCount = 4 + 2*2 = 8
      // endIndex = 0 + 8 = 8
      expect(result.endIndex).toBe(8)
    })
  })
})

describe('getScrollToPosition', () => {
  const itemSize = 20
  const containerSize = 400

  describe('item is visible', () => {
    it('returns undefined when item is in middle of viewport', () => {
      // Scrolled to 200 (items 10-30 visible)
      const result = getScrollToPosition(15, itemSize, 200, containerSize)
      expect(result).toBeUndefined()
    })

    it('returns undefined when item is at top of viewport', () => {
      // Scrolled to 200, item 10 is at top
      const result = getScrollToPosition(10, itemSize, 200, containerSize)
      expect(result).toBeUndefined()
    })

    it('returns undefined when item is at bottom of viewport', () => {
      // Scrolled to 200, item 29 has bottom at 600 which equals viewport bottom
      const result = getScrollToPosition(29, itemSize, 200, containerSize)
      expect(result).toBeUndefined()
    })
  })

  describe('item is above viewport', () => {
    it('returns scroll position to show item at top', () => {
      // Scrolled to 400 (items 20-40 visible), item 10 is above
      const result = getScrollToPosition(10, itemSize, 400, containerSize)
      expect(result).toBe(200) // Item 10 starts at 200
    })

    it('returns 0 for first item when scrolled', () => {
      const result = getScrollToPosition(0, itemSize, 400, containerSize)
      expect(result).toBe(0)
    })
  })

  describe('item is below viewport', () => {
    it('returns scroll position to show item at bottom', () => {
      // Scrolled to 0 (items 0-20 visible), item 30 is below
      const result = getScrollToPosition(30, itemSize, 0, containerSize)
      // Item 30 bottom is at 620, need to scroll so viewport bottom = 620
      // scrollOffset = 620 - 400 = 220
      expect(result).toBe(220)
    })

    it('scrolls to show last item at bottom', () => {
      // Last item (index 99) when scrolled to 0
      const result = getScrollToPosition(99, itemSize, 0, containerSize)
      // Item 99 bottom is at 2000, scroll so viewport bottom = 2000
      // scrollOffset = 2000 - 400 = 1600
      expect(result).toBe(1600)
    })
  })

  describe('edge cases', () => {
    it('handles item size of 1', () => {
      const result = getScrollToPosition(500, 1, 0, 100)
      // Item 500 bottom is 501, need scroll = 501 - 100 = 401
      expect(result).toBe(401)
    })

    it('handles large item that fills viewport', () => {
      const largeItemSize = 400 // Same as container
      const result = getScrollToPosition(5, largeItemSize, 0, containerSize)
      // Item 5 is at 2000-2400
      // Bottom = 2400, viewport at 400, need scroll = 2400 - 400 = 2000
      expect(result).toBe(2000)
    })

    it('handles container larger than total content', () => {
      // Small list with large container
      const result = getScrollToPosition(5, 20, 0, 1000)
      // Item 5 bottom is 120, viewport bottom is 1000
      // Item is visible
      expect(result).toBeUndefined()
    })

    it('handles scroll position exactly at item boundary', () => {
      // Scroll offset exactly at item 10 start
      const result = getScrollToPosition(10, itemSize, 200, containerSize)
      expect(result).toBeUndefined()
    })

    it('returns scroll position when item is exactly one pixel above', () => {
      // Item 9 ends at 200, viewport starts at 201
      const result = getScrollToPosition(9, itemSize, 201, containerSize)
      // Item 9 starts at 180
      expect(result).toBe(180)
    })

    it('returns scroll position when item is exactly one pixel below', () => {
      // Item 30 starts at 600, viewport ends at 599
      const result = getScrollToPosition(30, itemSize, 199, containerSize)
      // Item 30 bottom is 620, scroll = 620 - 400 = 220
      expect(result).toBe(220)
    })
  })
})

describe('calculateVirtualWindowVariable', () => {
  describe('basic calculations', () => {
    it('returns all zeros for empty widths', () => {
      const result = calculateVirtualWindowVariable([0], 5, 600, 0, 0)
      expect(result.startIndex).toBe(0)
      expect(result.endIndex).toBe(0)
      expect(result.visibleCount).toBe(0)
      expect(result.totalSize).toBe(0)
      expect(result.offset).toBe(0)
    })

    it('handles a single column wider than the container', () => {
      // One column 800px wide in a 600px viewport, scrolled to 0.
      const widths = [800]
      const prefixSums = prefixSumsFrom(widths) // [0, 800]
      const result = calculateVirtualWindowVariable(prefixSums, 0, 600, 0, widths.length)
      expect(result.startIndex).toBe(0)
      expect(result.endIndex).toBe(1)
      expect(result.visibleCount).toBe(1)
      expect(result.totalSize).toBe(800)
      expect(result.offset).toBe(0)
    })

    it('handles a single column wider than the container, scrolled mid-column', () => {
      const widths = [800]
      const prefixSums = prefixSumsFrom(widths)
      // Scrolled to 200 — the (only) column still starts at 0 (off-left), so it's the first visible.
      const result = calculateVirtualWindowVariable(prefixSums, 0, 600, 200, widths.length)
      expect(result.startIndex).toBe(0)
      expect(result.endIndex).toBe(1)
      expect(result.offset).toBe(0)
    })

    it('finds the correct range for many small columns scrolled to the middle', () => {
      // 20 columns of 100px each. Container 300px, scrolled to 1000 (column 10 starts at 1000).
      const widths = new Array<number>(20).fill(100)
      const prefixSums = prefixSumsFrom(widths)
      const result = calculateVirtualWindowVariable(prefixSums, 0, 300, 1000, widths.length)
      expect(result.startIndex).toBe(10)
      expect(result.endIndex).toBe(13) // 1000 + 300 = 1300; column 13 starts at 1300
      expect(result.totalSize).toBe(2000)
      expect(result.offset).toBe(prefixSums[result.startIndex])
    })

    it('worked example from plan review round 3', () => {
      // prefixSums=[0,100,200,350,500,700], scrollLeft=150, containerWidth=300, buffer=0
      // → startIndex=1 (col 1 starts at 100), endIndex=4 (col 3 ends at 500, intersects 150..450).
      const prefixSums = [0, 100, 200, 350, 500, 700]
      const result = calculateVirtualWindowVariable(prefixSums, 0, 300, 150, 5)
      expect(result.startIndex).toBe(1)
      expect(result.endIndex).toBe(4)
      expect(result.visibleCount).toBe(3)
      expect(result.totalSize).toBe(700)
      expect(result.offset).toBe(100)
    })
  })

  describe('boundaries', () => {
    it('treats an item whose right edge exactly equals viewport right as fully visible', () => {
      // 4 columns × 100px in a 200px viewport, scrolled to 100. prefixSums = [0,100,200,300,400].
      // viewportEnd = 300 = prefixSums[3]. The loop stops at j=3 because prefixSums[3] >= 300.
      // So columns 1 and 2 are visible (endIndex=3), not column 3.
      const widths = [100, 100, 100, 100]
      const prefixSums = prefixSumsFrom(widths)
      const result = calculateVirtualWindowVariable(prefixSums, 0, 200, 100, widths.length)
      expect(result.startIndex).toBe(1)
      expect(result.endIndex).toBe(3)
      expect(result.offset).toBe(100)
    })

    it('treats an item whose left edge exactly equals viewport left as the first visible', () => {
      // Scroll to 200, the boundary aligns with column 2's left edge. Column 2 is the first visible.
      const widths = [100, 100, 100, 100]
      const prefixSums = prefixSumsFrom(widths) // [0,100,200,300,400]
      const result = calculateVirtualWindowVariable(prefixSums, 0, 200, 200, widths.length)
      expect(result.startIndex).toBe(2)
      expect(result.endIndex).toBe(4)
      expect(result.offset).toBe(200)
    })

    it('handles totalSize correctly when scrolled to the far right', () => {
      const widths = [100, 100, 100, 100, 100]
      const prefixSums = prefixSumsFrom(widths) // total 500
      const result = calculateVirtualWindowVariable(prefixSums, 0, 200, 300, widths.length)
      // viewport 300..500 → first visible is column 3, walk to end of list
      expect(result.startIndex).toBe(3)
      expect(result.endIndex).toBe(5)
      expect(result.totalSize).toBe(500)
    })
  })

  describe('buffer', () => {
    it('applies buffer symmetrically in the middle of the list', () => {
      // 20 columns × 100px, container 300px, scrolled to 1000 (column 10 start), buffer 2.
      // Without buffer: start=10, end=13. With buffer 2: start=8, end=15.
      const widths = new Array<number>(20).fill(100)
      const prefixSums = prefixSumsFrom(widths)
      const result = calculateVirtualWindowVariable(prefixSums, 2, 300, 1000, widths.length)
      expect(result.startIndex).toBe(8)
      expect(result.endIndex).toBe(15)
      expect(result.visibleCount).toBe(7)
    })

    it("doesn't shrink the right buffer when the left buffer clamps at 0 (off-by-buffer guard)", () => {
      // 20 columns × 100px, container 300px, scrolled to 0, buffer 5.
      // Without the guard, a naive `endIndex = startIndex + visibleCount + 2 * bufferSize` style
      // would lose the 5 left-buffer slots and end at firstVisible + viewportColumns + 5 = 3 + 5 = 8.
      // With the correct math, startIndex = max(0, 0 - 5) = 0, but endIndex still gets the full
      // bufferSize=5 on the right: lastVisibleEnd=3 (300/100), endIndex = min(20, 3 + 5) = 8.
      // (Note: in this case the visible end happens to match — the bug only shows up when
      // the naive formula tries to "compensate" or when bufferSize is large enough that the
      // *right* edge gets clipped because the left clamp ate the buffer. See next test.)
      const widths = new Array<number>(20).fill(100)
      const prefixSums = prefixSumsFrom(widths)
      const result = calculateVirtualWindowVariable(prefixSums, 5, 300, 0, widths.length)
      expect(result.startIndex).toBe(0)
      expect(result.endIndex).toBe(8) // lastVisibleEnd=3, +5 buffer
    })

    it("doesn't shrink the left buffer when the right buffer clamps at totalItems (off-by-buffer guard, mirrored)", () => {
      // 20 columns × 100px, container 300px, scrolled to 1700 (last 3 columns visible), buffer 5.
      // firstVisibleIndex = 17, lastVisibleEnd = 20 (clamped by totalItems).
      // startIndex = max(0, 17 - 5) = 12, endIndex = min(20, 20 + 5) = 20.
      // A naive formula tying end-buffer to start-buffer would shrink one when the other clamps.
      const widths = new Array<number>(20).fill(100)
      const prefixSums = prefixSumsFrom(widths)
      const result = calculateVirtualWindowVariable(prefixSums, 5, 300, 1700, widths.length)
      expect(result.startIndex).toBe(12)
      expect(result.endIndex).toBe(20)
      expect(result.visibleCount).toBe(8)
    })

    it('buffer larger than available room clamps both ends independently', () => {
      // 5 columns × 100px, container 200px, scrolled to 0, buffer 100.
      // startIndex = max(0, 0 - 100) = 0
      // lastVisibleEnd = 2, endIndex = min(5, 2 + 100) = 5
      const widths = [100, 100, 100, 100, 100]
      const prefixSums = prefixSumsFrom(widths)
      const result = calculateVirtualWindowVariable(prefixSums, 100, 200, 0, widths.length)
      expect(result.startIndex).toBe(0)
      expect(result.endIndex).toBe(5)
      expect(result.visibleCount).toBe(5)
    })
  })

  describe('invariants', () => {
    it('throws when prefixSums length does not match totalItems + 1', () => {
      expect(() => calculateVirtualWindowVariable([0, 100, 200], 0, 100, 0, 5)).toThrow(/prefixSums.length/)
    })

    it('accepts the empty case (totalItems=0, prefixSums=[0])', () => {
      // Mirror of the empty test above, but explicit about the invariant boundary.
      expect(() => calculateVirtualWindowVariable([0], 0, 100, 0, 0)).not.toThrow()
    })
  })
})

describe('getScrollToPositionVariable', () => {
  const widths = [100, 150, 200, 50, 100] // total 600
  const prefixSums = prefixSumsFrom(widths) // [0, 100, 250, 450, 500, 600]
  const containerSize = 300

  describe('item is visible', () => {
    it('returns undefined when item is fully inside the viewport', () => {
      // Viewport 100..400. Item 1 spans 100..250 — fully visible.
      const result = getScrollToPositionVariable(prefixSums, 1, 100, containerSize)
      expect(result).toBeUndefined()
    })

    it('returns undefined when item left edge exactly equals viewport left edge', () => {
      // Viewport 100..400. Item 1 starts at 100.
      const result = getScrollToPositionVariable(prefixSums, 1, 100, containerSize)
      expect(result).toBeUndefined()
    })

    it('returns undefined when item right edge exactly equals viewport right edge', () => {
      // Item 2 ends at 450. Viewport ending at 450 = scrollOffset 150.
      const result = getScrollToPositionVariable(prefixSums, 2, 150, containerSize)
      expect(result).toBeUndefined()
    })
  })

  describe('item is off-left', () => {
    it("returns the item's left edge X when off-left", () => {
      // Viewport 200..500. Item 0 spans 0..100 — off-left. Want to scroll to 0.
      const result = getScrollToPositionVariable(prefixSums, 0, 200, containerSize)
      expect(result).toBe(0)
    })

    it('returns prefixSums[index] when item starts before scrollOffset', () => {
      // Viewport 300..600. Item 1 starts at 100, off-left. Scroll target = 100.
      const result = getScrollToPositionVariable(prefixSums, 1, 300, containerSize)
      expect(result).toBe(100)
    })
  })

  describe('item is off-right', () => {
    it('returns right − containerSize when item is off-right', () => {
      // Viewport 0..300. Item 3 spans 450..500 — off-right. Scroll target = 500 - 300 = 200.
      const result = getScrollToPositionVariable(prefixSums, 3, 0, containerSize)
      expect(result).toBe(200)
    })

    it('scrolls to fit the last item at the right edge', () => {
      // Viewport 0..300. Item 4 spans 500..600. Scroll = 600 - 300 = 300.
      const result = getScrollToPositionVariable(prefixSums, 4, 0, containerSize)
      expect(result).toBe(300)
    })
  })

  describe('invariants', () => {
    it('throws on negative index', () => {
      expect(() => getScrollToPositionVariable(prefixSums, -1, 0, containerSize)).toThrow(/out of range/)
    })

    it('throws when index equals totalItems', () => {
      expect(() => getScrollToPositionVariable(prefixSums, 5, 0, containerSize)).toThrow(/out of range/)
    })

    it('throws when index exceeds totalItems', () => {
      expect(() => getScrollToPositionVariable(prefixSums, 100, 0, containerSize)).toThrow(/out of range/)
    })
  })
})
