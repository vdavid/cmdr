import { describe, expect, it } from 'vitest'
import { dropSlotAt, resolveDrop, slotLineX, type TabSpan } from './tab-drop-slot'

/** Three 100px tabs with a 5px gap between them, starting at x = 10. */
const bar: TabSpan[] = [
  { left: 10, right: 110 },
  { left: 115, right: 215 },
  { left: 220, right: 320 },
]

describe('dropSlotAt', () => {
  it('lands before a tab while the pointer is on its left half', () => {
    expect(dropSlotAt(20, bar)).toBe(0)
    expect(dropSlotAt(59, bar)).toBe(0)
    expect(dropSlotAt(120, bar)).toBe(1)
  })

  it('lands after a tab once the pointer passes its middle', () => {
    expect(dropSlotAt(61, bar)).toBe(1)
    expect(dropSlotAt(166, bar)).toBe(2)
  })

  it('treats the gap between two tabs as the slot between them', () => {
    expect(dropSlotAt(112, bar)).toBe(1)
  })

  it('appends for the empty space and the "+" button past the last tab', () => {
    expect(dropSlotAt(271, bar)).toBe(3)
    expect(dropSlotAt(900, bar)).toBe(3)
  })

  it('lands first for a pointer left of every tab', () => {
    expect(dropSlotAt(-50, bar)).toBe(0)
  })

  it('has one slot in a bar with no tabs measured', () => {
    expect(dropSlotAt(100, [])).toBe(0)
  })
})

describe('slotLineX', () => {
  it('sits on the first tab left edge for the first slot', () => {
    expect(slotLineX(0, bar, 0)).toBe(10)
  })

  it('sits in the middle of the gap between two tabs', () => {
    expect(slotLineX(1, bar, 0)).toBe(112.5)
    expect(slotLineX(2, bar, 0)).toBe(217.5)
  })

  it('sits on the last tab right edge for the append slot', () => {
    expect(slotLineX(3, bar, 0)).toBe(320)
  })

  it('falls back to the bar left edge when no tab is measured', () => {
    expect(slotLineX(0, [], 42)).toBe(42)
  })
})

describe('resolveDrop', () => {
  const within = (slot: number, sourceIndex: number, count = 4) =>
    resolveDrop({ slot, sourceIndex, targetCount: count, samePane: true, maxTabs: 10 })
  const across = (slot: number, targetCount: number) =>
    resolveDrop({ slot, sourceIndex: 0, targetCount, samePane: false, maxTabs: 10 })

  describe('within one pane', () => {
    it('changes nothing on either slot that touches the dragged tab', () => {
      expect(within(1, 1)).toEqual({ kind: 'unchanged' })
      expect(within(2, 1)).toEqual({ kind: 'unchanged' })
    })

    it('keeps the slot as the index when moving left', () => {
      expect(within(0, 2)).toEqual({ kind: 'move', toIndex: 0 })
      expect(within(1, 3)).toEqual({ kind: 'move', toIndex: 1 })
    })

    it('takes one off the slot when moving right, since the tab leaves a hole behind it', () => {
      expect(within(3, 1)).toEqual({ kind: 'move', toIndex: 2 })
      expect(within(4, 0)).toEqual({ kind: 'move', toIndex: 3 })
    })

    it('ignores the cap: a reorder adds no tab', () => {
      expect(resolveDrop({ slot: 0, sourceIndex: 9, targetCount: 10, samePane: true, maxTabs: 10 })).toEqual({
        kind: 'move',
        toIndex: 0,
      })
    })

    it("changes nothing for a pane's only tab", () => {
      expect(within(0, 0, 1)).toEqual({ kind: 'unchanged' })
      expect(within(1, 0, 1)).toEqual({ kind: 'unchanged' })
    })
  })

  describe('to the other pane', () => {
    it('uses the slot as the index', () => {
      expect(across(0, 2)).toEqual({ kind: 'move', toIndex: 0 })
      expect(across(2, 2)).toEqual({ kind: 'move', toIndex: 2 })
    })

    it('refuses a pane at the cap', () => {
      expect(across(0, 10)).toEqual({ kind: 'refused' })
    })
  })
})
