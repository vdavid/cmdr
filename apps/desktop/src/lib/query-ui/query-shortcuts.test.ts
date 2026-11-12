/**
 * The dialog's modifier-shortcut router: new query, the mode chips, the Enter combinations,
 * recent items, and the numbered modes, on each platform's own keys.
 *
 * Two things regress quietly here: the macOS Option-glyph remap (⌥F arrives as `key: "ƒ"`,
 * so the router has to fall back to the key position), and modifier SUPERSETS (⌥⌘F must not
 * fire the plain ⌥F chip shortcut). Both get a case below. The macOS block keeps the events
 * it always sent; the Windows and Linux block speaks Ctrl and Alt.
 */

import { beforeEach, describe, it, expect } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { modeForShortcutNumber, routeModifierShortcut, type ModifierShortcutHandlers } from './query-shortcuts'

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

describe('routeModifierShortcut on macos', () => {
  beforeEach(() => withPlatform('macos'))

  it('routes ⌘N to the new-query reset', () => {
    const h = makeHandlers()
    const e = key('n', { meta: true })
    expect(routeModifierShortcut(e, h)).toBe(true)
    expect(e.defaultPrevented).toBe(true)
    expect(h.calls.newQuery).toBe(1)
  })

  it('rejects ⌘N supersets (⌥⌘N, ⇧⌘N, ⌃⌘N)', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('n', { meta: true, alt: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('n', { meta: true, shift: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('n', { meta: true, ctrl: true }), h)).toBe(false)
    expect(h.calls.newQuery).toBe(0)
  })

  it('matches ⌘N and ⌘Y on a non-Latin layout by the Latin key position', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('т', { meta: true, code: 'KeyN' }), h)).toBe(true)
    expect(routeModifierShortcut(key('н', { meta: true, code: 'KeyY' }), h)).toBe(true)
    expect(h.calls.newQuery).toBe(1)
    expect(h.calls.toggleRecent).toBe(1)
  })

  it('leaves ⌃N, ⌃Y, and ⌃1 alone: macOS takes ⌘', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('n', { ctrl: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('y', { ctrl: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('1', { ctrl: true }), h)).toBe(false)
  })

  it('routes ⌥A / ⌥F / ⌥R to the mode chips without stealing focus', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('a', { alt: true }), h)).toBe(true)
    expect(routeModifierShortcut(key('f', { alt: true }), h)).toBe(true)
    expect(routeModifierShortcut(key('r', { alt: true }), h)).toBe(true)
    expect(h.calls.modes).toEqual(['ai', 'filename', 'regex'])
    expect(h.calls.focusInput).toBe(0)
  })

  it('matches an ⌥ letter through the macOS typographic remap by key position', () => {
    // Option+F reports `key: "ƒ"` on macOS; `code` stays layout-stable.
    const h = makeHandlers()
    expect(routeModifierShortcut(key('ƒ', { alt: true, code: 'KeyF' }), h)).toBe(true)
    expect(routeModifierShortcut(key('®', { alt: true, code: 'KeyR' }), h)).toBe(true)
    expect(h.calls.modes).toEqual(['filename', 'regex'])
  })

  it('rejects ⌥ letter supersets (⌥⌘F, ⌥⇧F)', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('ƒ', { alt: true, meta: true, code: 'KeyF' }), h)).toBe(false)
    expect(routeModifierShortcut(key('Ï', { alt: true, shift: true, code: 'KeyF' }), h)).toBe(false)
    expect(h.calls.modes).toEqual([])
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

  it('swallows ⌘⏎, ⌃⏎, and ⇧⏎ so bare Enter stays the only Enter that acts', () => {
    const h = makeHandlers()
    for (const e of [key('Enter', { meta: true }), key('Enter', { ctrl: true }), key('Enter', { shift: true })]) {
      expect(routeModifierShortcut(e, h)).toBe(true)
      expect(e.defaultPrevented).toBe(true)
    }
    expect(h.calls.primary).toBe(0)
  })

  it('routes ⌘Y to the recent-items dropdown', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('y', { meta: true }), h)).toBe(true)
    expect(h.calls.toggleRecent).toBe(1)
  })

  it('leaves ⌘H unclaimed and unprevented, so it reaches macOS’s Hide', () => {
    const h = makeHandlers()
    const e = key('h', { meta: true })
    expect(routeModifierShortcut(e, h)).toBe(false)
    expect(e.defaultPrevented).toBe(false)
    expect(h.calls.toggleRecent).toBe(0)
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
    expect(routeModifierShortcut(key('4', { meta: true }), h)).toBe(false)
    expect(h.calls.modes).toEqual([])
  })

  it('ignores unmodified keys and unknown combos', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('a'), h)).toBe(false)
    expect(routeModifierShortcut(key('ArrowDown'), h)).toBe(false)
    expect(routeModifierShortcut(key('k', { meta: true }), h)).toBe(false)
  })
})

describe.each(['windows', 'linux'] as const)('routeModifierShortcut on %s', (platform) => {
  beforeEach(() => withPlatform(platform))

  it('routes Ctrl+N, Ctrl+H, and Ctrl+1-3', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('n', { ctrl: true, code: 'KeyN' }), h)).toBe(true)
    expect(routeModifierShortcut(key('h', { ctrl: true, code: 'KeyH' }), h)).toBe(true)
    expect(routeModifierShortcut(key('3', { ctrl: true, code: 'Digit3' }), h)).toBe(true)
    expect(h.calls).toMatchObject({ newQuery: 1, toggleRecent: 1, modes: ['regex'], focusInput: 1 })
  })

  it('leaves Super+N, Super+H, and Super+1 alone (Super is the desktop’s)', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('n', { meta: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('h', { meta: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('1', { meta: true }), h)).toBe(false)
    expect(h.calls).toMatchObject({ newQuery: 0, toggleRecent: 0, modes: [] })
  })

  it('rejects modifier supersets (Ctrl+Shift+N, Ctrl+Alt+N, Ctrl+Alt+F)', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('n', { ctrl: true, shift: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('n', { ctrl: true, alt: true }), h)).toBe(false)
    expect(routeModifierShortcut(key('f', { ctrl: true, alt: true, code: 'KeyF' }), h)).toBe(false)
    expect(h.calls).toMatchObject({ newQuery: 0, modes: [] })
  })

  it('routes Alt+A / Alt+F / Alt+R to the mode chips', () => {
    const h = makeHandlers()
    for (const letter of ['a', 'f', 'r']) {
      expect(routeModifierShortcut(key(letter, { alt: true, code: `Key${letter.toUpperCase()}` }), h)).toBe(true)
    }
    expect(h.calls.modes).toEqual(['ai', 'filename', 'regex'])
  })

  it('routes Alt+Enter to the primary action', () => {
    const h = makeHandlers()
    expect(routeModifierShortcut(key('Enter', { alt: true }), h)).toBe(true)
    expect(h.calls.primary).toBe(1)
  })

  it('swallows Ctrl+Enter, Super+Enter, and Shift+Enter, so none falls through to bare Enter', () => {
    const h = makeHandlers()
    for (const e of [key('Enter', { ctrl: true }), key('Enter', { meta: true }), key('Enter', { shift: true })]) {
      expect(routeModifierShortcut(e, h)).toBe(true)
      expect(e.defaultPrevented).toBe(true)
    }
    expect(h.calls.primary).toBe(0)
  })
})
