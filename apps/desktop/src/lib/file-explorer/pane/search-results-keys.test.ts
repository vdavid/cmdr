/**
 * Pins the snapshot pane's keyboard contract (David's round-2 P-series).
 *
 * Tests are written to fail BEFORE the implementation lands: each one mirrors one of the
 * bugs David reported in `docs/specs/search-fixup-round2-brief.md` ("P1-P10" section).
 */
import { describe, it, expect } from 'vitest'
import { computeSearchPaneKeyAction, type SearchPaneKeyAction } from './search-results-keys'

function key(
  name: string,
  mods: { shift?: boolean; meta?: boolean; ctrl?: boolean; alt?: boolean } = {},
): { key: string; shiftKey: boolean; metaKey: boolean; ctrlKey: boolean; altKey: boolean } {
  return {
    key: name,
    shiftKey: !!mods.shift,
    metaKey: !!mods.meta,
    ctrlKey: !!mods.ctrl,
    altKey: !!mods.alt,
  }
}

const ctx = { cursorIndex: 5, count: 50, visibleItems: 10 }

function asMove(a: SearchPaneKeyAction | null) {
  if (!a || a.kind !== 'move-cursor') throw new Error(`expected move-cursor, got ${a?.kind ?? 'null'}`)
  return a
}

describe('computeSearchPaneKeyAction', () => {
  describe('P1: PageUp / PageDown', () => {
    it('PageDown steps by visibleItems - 1', () => {
      const a = asMove(computeSearchPaneKeyAction(key('PageDown'), ctx))
      // visibleItems = 10 → step = 9. From 5 we land on 14.
      expect(a.index).toBe(14)
      expect(a.overflow).toBe(false)
    })

    it('PageDown clamps at the last entry and flags overflow', () => {
      const a = asMove(computeSearchPaneKeyAction(key('PageDown'), { cursorIndex: 48, count: 50, visibleItems: 10 }))
      expect(a.index).toBe(49)
      expect(a.overflow).toBe(true)
    })

    it('PageUp steps backward by visibleItems - 1', () => {
      const a = asMove(computeSearchPaneKeyAction(key('PageUp'), ctx))
      expect(a.index).toBe(0)
      // 5 - 9 = -4 → clamped, overflow true.
      expect(a.overflow).toBe(true)
    })

    it('PageUp from mid-list does NOT flag overflow when the step lands in-bounds', () => {
      const a = asMove(computeSearchPaneKeyAction(key('PageUp'), { cursorIndex: 30, count: 50, visibleItems: 10 }))
      expect(a.index).toBe(21)
      expect(a.overflow).toBe(false)
    })
  })

  describe('P2: Home / End', () => {
    it('Home moves to index 0 and flags overflow', () => {
      const a = asMove(computeSearchPaneKeyAction(key('Home'), ctx))
      expect(a.index).toBe(0)
      expect(a.overflow).toBe(true)
    })

    it('End moves to count - 1 and flags overflow', () => {
      const a = asMove(computeSearchPaneKeyAction(key('End'), ctx))
      expect(a.index).toBe(49)
      expect(a.overflow).toBe(true)
    })

    it('Home on an empty snapshot returns index 0', () => {
      const a = asMove(computeSearchPaneKeyAction(key('Home'), { cursorIndex: 0, count: 0, visibleItems: 10 }))
      expect(a.index).toBe(0)
    })
  })

  describe('P3: Left / Right are explicit no-ops', () => {
    it('ArrowLeft returns a noop action (intentional swallow)', () => {
      expect(computeSearchPaneKeyAction(key('ArrowLeft'), ctx)).toEqual({ kind: 'noop' })
    })

    it('ArrowRight returns a noop action', () => {
      expect(computeSearchPaneKeyAction(key('ArrowRight'), ctx)).toEqual({ kind: 'noop' })
    })
  })

  describe('P4: Space toggles selection at cursor', () => {
    it('plain Space toggles the cursor row', () => {
      expect(computeSearchPaneKeyAction(key(' '), ctx)).toEqual({ kind: 'toggle-selection-at-cursor' })
    })

    it('Shift+Space is NOT a toggle (reserved for Quick Look elsewhere)', () => {
      expect(computeSearchPaneKeyAction(key(' ', { shift: true }), ctx)).not.toEqual({
        kind: 'toggle-selection-at-cursor',
      })
    })

    it('Insert toggles and advances (Total Commander style)', () => {
      expect(computeSearchPaneKeyAction(key('Insert'), ctx)).toEqual({ kind: 'toggle-selection-and-advance' })
    })
  })

  describe('P5: Shift+Up / Shift+Down carry the shiftKey through to the caller', () => {
    it('ArrowDown with Shift propagates shiftKey on the move action', () => {
      const a = asMove(computeSearchPaneKeyAction(key('ArrowDown', { shift: true }), ctx))
      expect(a.shiftKey).toBe(true)
      expect(a.index).toBe(6)
    })

    it('ArrowUp with Shift propagates shiftKey on the move action', () => {
      const a = asMove(computeSearchPaneKeyAction(key('ArrowUp', { shift: true }), ctx))
      expect(a.shiftKey).toBe(true)
      expect(a.index).toBe(4)
    })

    it('PageDown with Shift propagates shiftKey for range fill', () => {
      const a = asMove(computeSearchPaneKeyAction(key('PageDown', { shift: true }), ctx))
      expect(a.shiftKey).toBe(true)
    })
  })

  describe('P7: F3 and F4 dispatch view / edit', () => {
    it('F3 returns view-file', () => {
      expect(computeSearchPaneKeyAction(key('F3'), ctx)).toEqual({ kind: 'view-file' })
    })

    it('F4 returns edit-file', () => {
      expect(computeSearchPaneKeyAction(key('F4'), ctx)).toEqual({ kind: 'edit-file' })
    })
  })

  describe('Enter opens the cursor', () => {
    it('plain Enter requests open-cursor', () => {
      expect(computeSearchPaneKeyAction(key('Enter'), ctx)).toEqual({ kind: 'open-cursor' })
    })
  })

  describe('Cmd / Ctrl combos defer to the unified dispatcher', () => {
    it('Cmd+A is not handled here', () => {
      expect(computeSearchPaneKeyAction(key('a', { meta: true }), ctx)).toBeNull()
    })

    it('Cmd+ArrowDown is not handled here', () => {
      expect(computeSearchPaneKeyAction(key('ArrowDown', { meta: true }), ctx)).toBeNull()
    })

    it('Ctrl+Space is not handled here', () => {
      expect(computeSearchPaneKeyAction(key(' ', { ctrl: true }), ctx)).toBeNull()
    })
  })

  describe('unknown keys', () => {
    it('returns null for letter keys', () => {
      expect(computeSearchPaneKeyAction(key('a'), ctx)).toBeNull()
    })

    it('returns null for Tab', () => {
      expect(computeSearchPaneKeyAction(key('Tab'), ctx)).toBeNull()
    })
  })
})
