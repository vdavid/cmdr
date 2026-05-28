/**
 * Pins the greedy-fit algorithm for the recent-searches footer strip. Mocked
 * widths only; the DOM-measurement integration lives in the Svelte component
 * on top of this helper.
 */
import { describe, it, expect } from 'vitest'
import { computeRecentChipsLayout } from './recent-chips-layout'

describe('computeRecentChipsLayout', () => {
  it('shows every chip when there is plenty of room', () => {
    const out = computeRecentChipsLayout({
      stripWidth: 1000,
      leadingLabelWidth: 100,
      trailingButtonWidth: 120,
      itemGap: 8,
      chipWidths: [80, 80, 80, 80, 80, 80],
    })
    expect(out.visibleCount).toBe(6)
  })

  it('drops trailing chips that do not fit in the middle slot', () => {
    // Middle slot = 300 - 50 - 60 - 2*4 = 182. Chips at 80 each + 4 px gap.
    // First chip: 80 → used 4+4+80 = 88. Visible 1.
    // Second: 88 + 4 + 80 = 172. Visible 2.
    // Third: 172 + 4 + 80 = 256. Exceeds 182. Stop.
    const out = computeRecentChipsLayout({
      stripWidth: 300,
      leadingLabelWidth: 50,
      trailingButtonWidth: 60,
      itemGap: 4,
      chipWidths: [80, 80, 80, 80, 80],
    })
    expect(out.visibleCount).toBe(2)
  })

  it('returns 0 when the strip is too narrow for any chip', () => {
    const out = computeRecentChipsLayout({
      stripWidth: 200,
      leadingLabelWidth: 100,
      trailingButtonWidth: 100,
      itemGap: 4,
      chipWidths: [50, 50, 50],
    })
    expect(out.visibleCount).toBe(0)
  })

  it('falls back to show-all when stripWidth is non-positive (defensive)', () => {
    const out = computeRecentChipsLayout({
      stripWidth: 0,
      leadingLabelWidth: 50,
      trailingButtonWidth: 60,
      itemGap: 4,
      chipWidths: [80, 80, 80],
    })
    expect(out.visibleCount).toBe(3)
  })

  it('returns 0 for an empty chip list, regardless of width', () => {
    const out = computeRecentChipsLayout({
      stripWidth: 1000,
      leadingLabelWidth: 50,
      trailingButtonWidth: 60,
      itemGap: 4,
      chipWidths: [],
    })
    expect(out.visibleCount).toBe(0)
  })

  it('respects variable chip widths (each chip checked against remaining slot)', () => {
    // Middle slot: 400 - 40 - 60 - 8 = 292. Outer gaps included via itemGap=4
    // (2 outer = 8). Chips: 100, 100, 100. After 1st = 4+4+100 = 108. After
    // 2nd = 108+4+100 = 212. After 3rd = 212+4+100 = 316 > 292 → stop at 2.
    const out = computeRecentChipsLayout({
      stripWidth: 400,
      leadingLabelWidth: 40,
      trailingButtonWidth: 60,
      itemGap: 4,
      chipWidths: [100, 100, 100],
    })
    expect(out.visibleCount).toBe(2)
  })
})
