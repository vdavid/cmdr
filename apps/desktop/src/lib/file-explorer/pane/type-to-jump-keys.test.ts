/**
 * Tests for `routeTypingKey`, the ONE typing intercept both key paths share.
 * They pin which keys each mode consumes and which fall through.
 */
import { beforeEach, describe, it, expect, vi } from 'vitest'
import { isBoundByAnyCommand } from '#lib/shortcuts/index.js'
import { routeTypingKey, typingFilters, type TypingKeyTarget } from './type-to-jump-keys'

vi.mock('#lib/shortcuts/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/shortcuts/index.js')>()),
  isBoundByAnyCommand: vi.fn(() => false),
}))

beforeEach(() => {
  vi.mocked(isBoundByAnyCommand).mockReset().mockReturnValue(false)
})

/** The target with its spies as plain `vi.fn()` properties (not `TypingKeyTarget` methods), so asserting on one isn't an unbound method. */
function pane(mode: 'jump' | 'filter', active = false, renaming = false, typingLive = false) {
  const target = {
    isFilterTypingLive: () => typingLive,
    endFilterTyping: vi.fn(),
    noteFilterKeyRanCommand: vi.fn(),
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

describe('typingFilters', () => {
  it('follows the setting in a folder', () => {
    expect(typingFilters('filter', false)).toBe(true)
    expect(typingFilters('jump', false)).toBe(false)
  })

  it('falls back to jump in the network views, which have nothing to filter', () => {
    expect(typingFilters('filter', true)).toBe(false)
    expect(typingFilters('jump', true)).toBe(false)
  })
})

describe('routeTypingKey in filter mode', () => {
  it('starts the filter on a letter or digit', () => {
    const p = pane('filter')
    expect(routeTypingKey(p, key('a'))).toBe(true)
    expect(p.appendQuickFilter).toHaveBeenCalledWith('a')
  })

  // Regression anchor: cmdr-reports#56, a Russian layout couldn't start the filter at all.
  it('starts the filter on a letter or digit from any script', () => {
    for (const k of ['ф', 'Ж', 'й', 'ё', 'é', 'ß', 'α', 'あ', '中', '٣']) {
      const p = pane('filter')
      expect(routeTypingKey(p, key(k))).toBe(true)
      expect(p.appendQuickFilter).toHaveBeenCalledWith(k)
    }
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

// Regression: ⇧Space (Quick Look) did nothing while a filter was on, because the filter took it as a typed space.
describe('routeTypingKey with ⇧Space', () => {
  const shiftSpace = () => key(' ', { shiftKey: true })

  it('leaves a bound ⇧Space to its command while a filter is on', () => {
    vi.mocked(isBoundByAnyCommand).mockReturnValue(true)
    const p = pane('filter', true)
    expect(routeTypingKey(p, shiftSpace())).toBe(false)
    expect(p.appendQuickFilter).not.toHaveBeenCalled()
  })

  it('leaves a bound ⇧Space to its command while a jump is on', () => {
    vi.mocked(isBoundByAnyCommand).mockReturnValue(true)
    const p = pane('jump', true)
    expect(routeTypingKey(p, shiftSpace())).toBe(false)
    expect(p.handleJumpKeystroke).not.toHaveBeenCalled()
  })

  it('keeps an unbound ⇧Space as a typed space, where no command would miss it', () => {
    const p = pane('filter', true)
    expect(routeTypingKey(p, shiftSpace())).toBe(true)
    expect(p.appendQuickFilter).toHaveBeenCalledWith(' ')
  })

  it('still types plain Space and shifted punctuation mid-burst, even when they are bound', () => {
    vi.mocked(isBoundByAnyCommand).mockReturnValue(true)
    const p = pane('filter', true, false, true)
    expect(routeTypingKey(p, key(' '))).toBe(true)
    expect(p.appendQuickFilter).toHaveBeenCalledWith(' ')
    expect(routeTypingKey(p, key('_', { shiftKey: true }))).toBe(true)
    expect(p.appendQuickFilter).toHaveBeenCalledWith('_')
  })
})

describe('routeTypingKey and the filter typing burst', () => {
  it('types a bound key into the filter while the person is still typing', () => {
    vi.mocked(isBoundByAnyCommand).mockReturnValue(true)
    const p = pane('filter', true, false, true)
    for (const k of [' ', '-', '*']) expect(routeTypingKey(p, key(k))).toBe(true)
    expect(p.appendQuickFilter.mock.calls).toEqual([[' '], ['-'], ['*']])
    expect(p.noteFilterKeyRanCommand).not.toHaveBeenCalled()
  })

  it('hands a bound key to its command after a pause, and says so once it does', () => {
    vi.mocked(isBoundByAnyCommand).mockReturnValue(true)
    const p = pane('filter', true)
    expect(routeTypingKey(p, key(' '))).toBe(false)
    expect(p.appendQuickFilter).not.toHaveBeenCalled()
    expect(p.noteFilterKeyRanCommand).toHaveBeenCalledWith(' ')
  })

  it('keeps typing an unbound key after a pause, since no command would take it', () => {
    const p = pane('filter', true)
    expect(routeTypingKey(p, key('!', { shiftKey: true }))).toBe(true)
    expect(p.appendQuickFilter).toHaveBeenCalledWith('!')
    expect(p.noteFilterKeyRanCommand).not.toHaveBeenCalled()
  })

  it('always types letters and digits, pause or not', () => {
    vi.mocked(isBoundByAnyCommand).mockReturnValue(true)
    const p = pane('filter', true)
    expect(routeTypingKey(p, key('a'))).toBe(true)
    expect(routeTypingKey(p, key('7'))).toBe(true)
    expect(p.appendQuickFilter.mock.calls).toEqual([['a'], ['7']])
  })

  it('ends the burst at once on a cursor move, modifiers or not, and lets the key through', () => {
    for (const k of ['ArrowDown', 'ArrowUp', 'Home', 'End', 'PageDown', 'Enter', 'Tab']) {
      const p = pane('filter', true, false, true)
      expect(routeTypingKey(p, key(k))).toBe(false)
      expect(p.endFilterTyping).toHaveBeenCalledOnce()
    }
    const shifted = pane('filter', true, false, true)
    routeTypingKey(shifted, key('ArrowDown', { shiftKey: true }))
    expect(shifted.endFilterTyping).toHaveBeenCalledOnce()
  })

  it('keeps the burst through Backspace, which edits the pattern', () => {
    const p = pane('filter', true, false, true)
    expect(routeTypingKey(p, key('Backspace'))).toBe(true)
    expect(p.endFilterTyping).not.toHaveBeenCalled()
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

  it('starts a jump on a non-Latin letter, and still leaves punctuation to its command', () => {
    const p = pane('jump')
    expect(routeTypingKey(p, key('д'))).toBe(true)
    expect(p.handleJumpKeystroke).toHaveBeenCalledWith('д')
    expect(routeTypingKey(p, key('-'))).toBe(false)
    expect(routeTypingKey(p, key(' '))).toBe(false)
  })
})
