/**
 * Pins the snapshot pane's keyboard contract.
 *
 * Originally written TDD-style for the snapshot-pane keyboard fixes (PgUp/PgDown,
 * Home/End, Space-toggle, ⇧Up/⇧Down range, ⌘A select-all, F-keys). Each case
 * pins a specific contract: see `computeSearchPaneKeyAction` below.
 */
import { beforeEach, describe, it, expect, vi } from 'vitest'

const { eventMatchesCommandSpy } = vi.hoisted(() => ({ eventMatchesCommandSpy: vi.fn() }))
vi.mock('#lib/shortcuts/index.js', () => ({ eventMatchesCommand: eventMatchesCommandSpy }))

import { computeSearchPaneKeyAction, type SearchPaneKeyAction } from './search-results-keys'

function key(
  name: string,
  mods: { shift?: boolean; meta?: boolean; ctrl?: boolean; alt?: boolean } = {},
): KeyboardEvent {
  return new KeyboardEvent('keydown', {
    key: name,
    shiftKey: !!mods.shift,
    metaKey: !!mods.meta,
    ctrlKey: !!mods.ctrl,
    altKey: !!mods.alt,
  })
}

const ctx = { cursorIndex: 5, count: 50, visibleItems: 10 }

function asMove(a: SearchPaneKeyAction | null) {
  if (!a || a.kind !== 'move-cursor') throw new Error(`expected move-cursor, got ${a?.kind ?? 'null'}`)
  return a
}

describe('computeSearchPaneKeyAction', () => {
  beforeEach(() => {
    eventMatchesCommandSpy.mockReset().mockReturnValue(false)
  })

  it('reveals the cursor result for the live nav.parent binding, without swallowing Cmd+Backspace', () => {
    eventMatchesCommandSpy.mockImplementation((event: { key: string; metaKey: boolean }, command: string) => {
      const combo = `${event.metaKey ? 'Meta+' : ''}${event.key}`
      return (
        (command === 'file.delete' && combo === 'Meta+Backspace') ||
        (command === 'nav.parent' && ['Backspace', 'k', 'Meta+ArrowUp'].includes(combo))
      )
    })

    expect(computeSearchPaneKeyAction(key('k'), ctx)).toEqual({ kind: 'reveal-cursor' })
    expect(computeSearchPaneKeyAction(key('Backspace'), ctx)).toEqual({ kind: 'reveal-cursor' })
    expect(computeSearchPaneKeyAction(key('ArrowUp', { meta: true }), ctx)).toEqual({ kind: 'reveal-cursor' })
    expect(computeSearchPaneKeyAction(key('Backspace', { meta: true }), ctx)).toBeNull()
  })

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

  describe('P3: Left / Right jump to the first and last row, like Full view', () => {
    it('ArrowLeft jumps to the first row', () => {
      expect(computeSearchPaneKeyAction(key('ArrowLeft'), ctx)).toEqual({
        kind: 'move-cursor',
        index: 0,
        overflow: true,
        shiftKey: false,
      })
    })

    it('ArrowRight jumps to the last row', () => {
      expect(computeSearchPaneKeyAction(key('ArrowRight'), ctx)).toEqual({
        kind: 'move-cursor',
        index: 49,
        overflow: true,
        shiftKey: false,
      })
    })

    it('Shift+ArrowRight extends the selection to the last row', () => {
      const a = asMove(computeSearchPaneKeyAction(key('ArrowRight', { shift: true }), ctx))
      expect(a.index).toBe(49)
      expect(a.shiftKey).toBe(true)
    })

    it('ArrowRight on an empty snapshot returns index 0', () => {
      const a = asMove(computeSearchPaneKeyAction(key('ArrowRight'), { cursorIndex: 0, count: 0, visibleItems: 10 }))
      expect(a.index).toBe(0)
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
