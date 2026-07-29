/**
 * The dialog's modifier-shortcut router: ⌘N, ⌥A/F/R, ⌥⏎, ⌘⏎/⇧⏎, ⌘H, and ⌘1-9.
 *
 * Two things regress quietly here: the macOS Option-glyph remap (⌥F arrives as `key: "ƒ"`,
 * so the router has to fall back to `code`), and modifier SUPERSETS (⌥⌘F must not fire the
 * plain ⌥F chip shortcut). Both get a case below.
 */

import { describe, it, expect } from 'vitest'
import {
  matchKey,
  modeForShortcutNumber,
  routeModifierShortcut,
  type ModifierShortcutHandlers,
} from './query-shortcuts'

interface Mods {
  meta?: boolean
  alt?: boolean
  shift?: boolean
  ctrl?: boolean
  code?: string
}

function key(k: string, mods: Mods = {}): KeyboardEvent {
  return new KeyboardEvent('keydown', {
    key: k,
    code: mods.code,
    metaKey: mods.meta ?? false,
    altKey: mods.alt ?? false,
    shiftKey: mods.shift ?? false,
    ctrlKey: mods.ctrl ?? false,
    cancelable: true,
  })
}

function makeHandlers(aiEnabled = true): ModifierShortcutHandlers & {
  calls: {
    newQuery: number
    modes: string[]
    focusInput: number
    toggleRecent: number
    primary: number
  }
} {
  const calls = { newQuery: 0, modes: [] as string[], focusInput: 0, toggleRecent: 0, primary: 0 }
  return {
    calls,
    aiEnabled,
    onNewQuery: () => {
      calls.newQuery += 1
    },
    onModeChange: (mode) => {
      calls.modes.push(mode)
    },
    onFocusInput: () => {
      calls.focusInput += 1
    },
    onToggleRecent: () => {
      calls.toggleRecent += 1
    },
    onPrimaryAction: () => {
      calls.primary += 1
    },
  }
}

describe('matchKey', () => {
  it('matches a plain ⌘ combo', () => {
    expect(matchKey(key('n', { meta: true }), 'n', 'meta')).toBe(true)
  })

  it('rejects a modifier superset (⌥⌘N is not ⌘N)', () => {
    expect(matchKey(key('n', { meta: true, alt: true }), 'n', 'meta')).toBe(false)
  })

  it('rejects Shift and Control combos outright', () => {
    expect(matchKey(key('n', { meta: true, shift: true }), 'n', 'meta')).toBe(false)
    expect(matchKey(key('n', { meta: true, ctrl: true }), 'n', 'meta')).toBe(false)
  })

  it('matches an ⌥ letter through the macOS typographic remap via `code`', () => {
    // Option+F reports `key: "ƒ"` on macOS; `code` stays layout-stable.
    expect(matchKey(key('ƒ', { alt: true, code: 'KeyF' }), 'f', 'alt')).toBe(true)
  })

  it('does not fall back to `code` for ⌘ combos or named keys', () => {
    expect(matchKey(key('ƒ', { meta: true, code: 'KeyF' }), 'f', 'meta')).toBe(false)
    expect(matchKey(key('Dead', { alt: true, code: 'Enter' }), 'Enter', 'alt')).toBe(false)
  })
})

describe('modeForShortcutNumber', () => {
  it('puts AI first when AI is on', () => {
    expect(modeForShortcutNumber(1, true)).toBe('ai')
    expect(modeForShortcutNumber(2, true)).toBe('filename')
    expect(modeForShortcutNumber(3, true)).toBe('regex')
    expect(modeForShortcutNumber(4, true)).toBeNull()
  })

  it('shifts the numbering down when AI is off', () => {
    expect(modeForShortcutNumber(1, false)).toBe('filename')
    expect(modeForShortcutNumber(2, false)).toBe('regex')
    expect(modeForShortcutNumber(3, false)).toBeNull()
  })
})

describe('routeModifierShortcut', () => {
  it('routes ⌘N to the new-query reset', () => {
    const h = makeHandlers()
    const e = key('n', { meta: true })
    expect(routeModifierShortcut(e, h)).toBe(true)
    expect(e.defaultPrevented).toBe(true)
    expect(h.calls.newQuery).toBe(1)
  })

  it('routes ⌥A / ⌥F / ⌥R to the mode chips without stealing focus', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('a', { alt: true }), h)).toBe(true)
    expect(routeModifierShortcut(key('f', { alt: true }), h)).toBe(true)
    expect(routeModifierShortcut(key('r', { alt: true }), h)).toBe(true)
    expect(h.calls.modes).toEqual(['ai', 'filename', 'regex'])
    expect(h.calls.focusInput).toBe(0)
  })

  it('leaves ⌥A alone when AI is off', () => {
    const h = makeHandlers(false)
    expect(routeModifierShortcut(key('a', { alt: true }), h)).toBe(false)
    expect(h.calls.modes).toEqual([])
  })

  it('routes ⌥⏎ to the primary action', () => {
    const h = makeHandlers()
    const e = key('Enter', { alt: true })
    expect(routeModifierShortcut(e, h)).toBe(true)
    expect(e.defaultPrevented).toBe(true)
    expect(h.calls.primary).toBe(1)
  })

  it('swallows ⌘⏎ and ⇧⏎ so bare Enter stays the only Enter that acts', () => {
    const h = makeHandlers()
    for (const e of [key('Enter', { meta: true }), key('Enter', { shift: true })]) {
      expect(routeModifierShortcut(e, h)).toBe(true)
      expect(e.defaultPrevented).toBe(true)
    }
    expect(h.calls.primary).toBe(0)
  })

  it('routes ⌘H to the recent-items dropdown', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('h', { meta: true }), h)).toBe(true)
    expect(h.calls.toggleRecent).toBe(1)
  })

  it('routes ⌘1-3 to the numbered chips and returns focus to the field', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('1', { meta: true }), h)).toBe(true)
    expect(routeModifierShortcut(key('2', { meta: true }), h)).toBe(true)
    expect(h.calls.modes).toEqual(['ai', 'filename'])
    expect(h.calls.focusInput).toBe(2)
  })

  it('ignores a numbered shortcut with no chip behind it', () => {
    const h = makeHandlers(false)
    expect(routeModifierShortcut(key('3', { meta: true }), h)).toBe(false)
    expect(h.calls.modes).toEqual([])
  })

  it('ignores unmodified keys and unknown combos', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('a'), h)).toBe(false)
    expect(routeModifierShortcut(key('ArrowDown'), h)).toBe(false)
    expect(routeModifierShortcut(key('k', { meta: true }), h)).toBe(false)
  })
})
