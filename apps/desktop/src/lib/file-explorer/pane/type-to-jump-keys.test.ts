/**
 * Tests for `routeTypingKey`, the ONE typing intercept both key paths share.
 * They pin which keys each mode consumes and which fall through.
 */
import { describe, it, expect, vi } from 'vitest'
import { routeTypingKey, type TypingKeyTarget } from './type-to-jump-keys'

/** The target with its spies as plain `vi.fn()` properties (not `TypingKeyTarget` methods), so asserting on one isn't an unbound method. */
function pane(mode: 'jump' | 'filter', active = false, renaming = false) {
  const target = {
    isRenaming: () => renaming,
    isJumpActive: () => active,
    handleJumpKeystroke: vi.fn(),
    clearJumpState: vi.fn(),
    isQuickFilterMode: () => mode === 'filter',
    isQuickFilterActive: () => active,
    appendQuickFilter: vi.fn(),
    backspaceQuickFilter: vi.fn(),
    clearQuickFilter: vi.fn(),
  } satisfies TypingKeyTarget
  return target
}

const key = (k: string, mods: KeyboardEventInit = {}) => new KeyboardEvent('keydown', { key: k, ...mods })

describe('routeTypingKey in filter mode', () => {
  it('starts the filter on a letter or digit', () => {
    const p = pane('filter')
    expect(routeTypingKey(p, key('a'))).toBe(true)
    expect(p.appendQuickFilter).toHaveBeenCalledWith('a')
  })

  it('leaves punctuation, Backspace, and Esc alone while no filter is on', () => {
    const p = pane('filter')
    expect(routeTypingKey(p, key('-'))).toBe(false)
    expect(routeTypingKey(p, key('Backspace'))).toBe(false)
    expect(routeTypingKey(p, key('Escape'))).toBe(false)
  })

  it('extends an active filter with any printable, edits with Backspace, clears with Esc', () => {
    const p = pane('filter', true)
    expect(routeTypingKey(p, key('.'))).toBe(true)
    expect(p.appendQuickFilter).toHaveBeenCalledWith('.')
    expect(routeTypingKey(p, key('Backspace'))).toBe(true)
    expect(p.backspaceQuickFilter).toHaveBeenCalled()
    expect(routeTypingKey(p, key('Escape'))).toBe(true)
    expect(p.clearQuickFilter).toHaveBeenCalled()
  })

  it('lets navigation keys through and keeps the filter', () => {
    const p = pane('filter', true)
    for (const k of ['ArrowDown', 'Enter', 'Tab', 'PageDown', 'Home']) {
      expect(routeTypingKey(p, key(k))).toBe(false)
    }
    expect(p.clearQuickFilter).not.toHaveBeenCalled()
    expect(p.clearJumpState).not.toHaveBeenCalled()
  })

  it('never swallows a command combo', () => {
    const p = pane('filter', true)
    expect(routeTypingKey(p, key('a', { metaKey: true }))).toBe(false)
    expect(routeTypingKey(p, key('Backspace', { metaKey: true }))).toBe(false)
  })

  it('stands aside while renaming', () => {
    expect(routeTypingKey(pane('filter', false, true), key('a'))).toBe(false)
  })
})

describe('routeTypingKey in jump mode', () => {
  it('feeds the jump buffer and clears it on a reset key', () => {
    const p = pane('jump')
    expect(routeTypingKey(p, key('a'))).toBe(true)
    expect(p.handleJumpKeystroke).toHaveBeenCalledWith('a')
    expect(routeTypingKey(p, key('ArrowDown'))).toBe(false)
    expect(p.clearJumpState).toHaveBeenCalled()
    expect(p.appendQuickFilter).not.toHaveBeenCalled()
  })
})
