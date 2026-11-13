import { describe, it, expect, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import {
  formatKeyCombo,
  physicalKeyCombo,
  latinKeyCombo,
  formatChord,
  isTypingKeyCombo,
  typingRefusal,
  toDisplayShortcut,
  toCanonicalShortcut,
  comboHasShift,
  typedCharacterCombo,
  keyComboCandidates,
  capturedKeyCombo,
  toAriaKeyShortcut,
} from './key-capture'
import { keydownWithAltGraph } from './test-key-events'

/** Runs the rest of the current test on macOS or, with `false`, off it. */
function setMacOS(isMac: boolean) {
  onTestFinished(withPlatform(isMac ? 'macos' : 'linux'))
}

function makeKeyEvent(overrides: Partial<KeyboardEvent>): KeyboardEvent {
  return {
    key: '',
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...overrides,
  } as KeyboardEvent
}

describe('formatChord', () => {
  it('writes macOS glyphs in ⌘⌃⌥⇧ order on macOS', () => {
    setMacOS(true)
    expect(formatChord({ shiftKey: true }, 'F9')).toBe('⇧F9')
    expect(formatChord({ shiftKey: true, metaKey: true, altKey: true }, 'F')).toBe('⌘⌥⇧F')
  })

  it('writes Ctrl+Alt+Shift+Super words off macOS', () => {
    setMacOS(false)
    expect(formatChord({ shiftKey: true }, 'F9')).toBe('Shift+F9')
    expect(formatChord({ metaKey: true, shiftKey: true, ctrlKey: true }, 'F')).toBe('Ctrl+Shift+Super+F')
  })

  it('agrees with formatKeyCombo for the same keypress', () => {
    for (const platform of ['macos', 'linux'] as const) {
      const undo = withPlatform(platform)
      const event = makeKeyEvent({ ctrlKey: true, altKey: true, key: 'r' })
      expect(formatChord({ ctrlKey: true, altKey: true }, 'R')).toBe(formatKeyCombo(event))
      undo()
    }
  })
})

describe('formatKeyCombo', () => {
  it('uses Super for metaKey on Linux', () => {
    setMacOS(false)
    const result = formatKeyCombo(makeKeyEvent({ metaKey: true, key: 'a' }))
    expect(result).toBe('Super+A')
  })

  it('uses ⌘ for metaKey on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ metaKey: true, key: 'a' }))
    expect(result).toBe('⌘A')
  })

  it('formats Alt+F1 on Linux', () => {
    setMacOS(false)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'F1' }))
    expect(result).toBe('Alt+F1')
  })

  it('formats Alt+F2 on Linux', () => {
    setMacOS(false)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'F2' }))
    expect(result).toBe('Alt+F2')
  })

  it('resolves Dead key via event.code for ⌥+letter on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'Dead', code: 'KeyH' }))
    expect(result).toBe('⌥H')
  })

  it('resolves Dead key via event.code for ⌥+digit on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'Dead', code: 'Digit6' }))
    expect(result).toBe('⌥6')
  })

  it('resolves Dead key via event.code for ⌥+punctuation on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'Dead', code: 'BracketLeft' }))
    expect(result).toBe('⌥[')
  })

  it('resolves Dead key with multiple modifiers on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ metaKey: true, altKey: true, key: 'Dead', code: 'KeyE' }))
    expect(result).toBe('⌘⌥E')
  })
})

describe('physicalKeyCombo', () => {
  it('names the physical key when ⌥⇧ changed what the layout typed', () => {
    setMacOS(true)
    // macOS US QWERTY types `±` for ⌥⇧=, so `event.key` spells nothing bindable.
    const usPlusMinus = makeKeyEvent({ key: '±', code: 'Equal', altKey: true, shiftKey: true })
    expect(physicalKeyCombo(usPlusMinus)).toBe('⌥⇧=')
  })

  it('names the same physical key on a layout that types something else entirely', () => {
    setMacOS(true)
    const otherLayout = makeKeyEvent({ key: 'ő', code: 'Equal', altKey: true, shiftKey: true })
    expect(physicalKeyCombo(otherLayout)).toBe('⌥⇧=')
  })

  it('stays out of a Shift-only keypress: the typed character IS its name', () => {
    setMacOS(true)
    // `⇧8` types `*` on US and `(` on Hungarian; those are the `*` and `(` keys
    // for that user, so naming the physical key would make the combo layout-bound.
    expect(physicalKeyCombo(makeKeyEvent({ key: '*', code: 'Digit8', shiftKey: true }))).toBeNull()
    expect(physicalKeyCombo(makeKeyEvent({ key: '(', code: 'Digit8', shiftKey: true }))).toBeNull()
  })

  it('names the physical key when ⌘⇧ changed what the layout typed', () => {
    setMacOS(true)
    // ⌘⇧. reports `>` on US: the command key stays a key position, like any ⌘ combo.
    expect(physicalKeyCombo(makeKeyEvent({ key: '>', code: 'Period', metaKey: true, shiftKey: true }))).toBe('⌘⇧.')
  })

  it('returns null when event.key already IS the physical character', () => {
    setMacOS(true)
    expect(physicalKeyCombo(makeKeyEvent({ key: '=', code: 'Equal', shiftKey: true }))).toBeNull()
    expect(physicalKeyCombo(makeKeyEvent({ key: '8', code: 'Digit8', altKey: true }))).toBeNull()
  })

  it('returns null with no character-altering modifier held', () => {
    setMacOS(true)
    expect(physicalKeyCombo(makeKeyEvent({ key: '=', code: 'Equal' }))).toBeNull()
    expect(physicalKeyCombo(makeKeyEvent({ key: '=', code: 'Equal', metaKey: true }))).toBeNull()
  })

  it('returns null for a code it cannot name (letters, numpad, function keys)', () => {
    setMacOS(true)
    // ⌥A types `å` on US, but letters are out of scope: `normalizeKeyName`'s Dead
    // branch covers the layouts that matter and nothing binds a bare ⌥<letter>.
    expect(physicalKeyCombo(makeKeyEvent({ key: 'å', code: 'KeyA', altKey: true }))).toBeNull()
    expect(physicalKeyCombo(makeKeyEvent({ key: '+', code: 'NumpadAdd', altKey: true }))).toBeNull()
    expect(physicalKeyCombo(makeKeyEvent({ key: 'F1', code: 'F1', shiftKey: true }))).toBeNull()
  })

  it('names the letter key when ⌥ composed a character under a command modifier', () => {
    setMacOS(true)
    // ⌘⌥R reports `®` and ⌘⌥F `ƒ` on US: never typing, so the key position is the identity,
    // as the Dead branch of `normalizeKeyName` already makes it for ⌘⌥E.
    expect(physicalKeyCombo(makeKeyEvent({ key: '®', code: 'KeyR', metaKey: true, altKey: true }))).toBe('⌘⌥R')
    expect(physicalKeyCombo(makeKeyEvent({ key: 'ƒ', code: 'KeyF', ctrlKey: true, altKey: true }))).toBe('⌃⌥F')
  })
})

/**
 * Real keypresses captured on macOS (Safari key logger, 2026-10-05, US layout plus a
 * PC-layout Genius keyboard), and the layouts the character rule exists for.
 */
describe('typed symbols are named by the character, on any layout', () => {
  it.each([
    ['US ⇧8', { key: '*', code: 'Digit8', shiftKey: true }, '*'],
    ["Swedish ⇧'", { key: '*', code: 'Backslash', shiftKey: true }, '*'],
    ['numpad *', { key: '*', code: 'NumpadMultiply' }, '*'],
    ['US ⇧=', { key: '+', code: 'Equal', shiftKey: true }, '+'],
    ['Hungarian ⇧3', { key: '+', code: 'Digit3', shiftKey: true }, '+'],
    ['Swedish +', { key: '+', code: 'Minus' }, '+'],
    ['numpad +', { key: '+', code: 'NumpadAdd' }, '+'],
    ['US ⇧-', { key: '_', code: 'Minus', shiftKey: true }, '_'],
    ['French ⇧1', { key: '1', code: 'Digit1', shiftKey: true }, '1'],
  ])('%s formats as %s', (_layout, event, expected) => {
    setMacOS(true)
    expect(formatKeyCombo(makeKeyEvent(event))).toBe(expected)
  })

  it('drops Shift off macOS the same way', () => {
    setMacOS(false)
    expect(formatKeyCombo(makeKeyEvent({ key: '*', code: 'Digit8', shiftKey: true }))).toBe('*')
  })

  it('keeps Shift where it means something: letters, Space, and named keys', () => {
    setMacOS(true)
    expect(formatKeyCombo(makeKeyEvent({ key: 'H', code: 'KeyH', shiftKey: true }))).toBe('⇧H')
    expect(formatKeyCombo(makeKeyEvent({ key: 'Ő', code: 'BracketLeft', shiftKey: true }))).toBe('⇧Ő')
    expect(formatKeyCombo(makeKeyEvent({ key: ' ', code: 'Space', shiftKey: true }))).toBe('⇧Space')
    expect(formatKeyCombo(makeKeyEvent({ key: 'F8', code: 'F8', shiftKey: true }))).toBe('⇧F8')
    expect(formatKeyCombo(makeKeyEvent({ key: 'Tab', code: 'Tab', shiftKey: true }))).toBe('⇧Tab')
  })

  it('keeps Shift when a command modifier is held: that combo is a key position', () => {
    setMacOS(true)
    expect(formatKeyCombo(makeKeyEvent({ key: '>', code: 'Period', metaKey: true, shiftKey: true }))).toBe('⌘⇧>')
    expect(formatKeyCombo(makeKeyEvent({ key: '±', code: 'Equal', altKey: true, shiftKey: true }))).toBe('⌥⇧±')
  })
})

describe('PC keys macOS renames', () => {
  it('names the PC Insert key Insert, though macOS reports it as Help', () => {
    setMacOS(true)
    // Captured: key "Help", code "Help", keyCode 45 (a Genius PC keyboard on macOS).
    expect(formatKeyCombo(makeKeyEvent({ key: 'Help', code: 'Help' }))).toBe('Insert')
    expect(formatKeyCombo(makeKeyEvent({ key: 'Insert', code: 'Insert' }))).toBe('Insert')
  })

  it('names forward delete Delete', () => {
    setMacOS(true)
    expect(formatKeyCombo(makeKeyEvent({ key: 'Delete', code: 'Delete' }))).toBe('Delete')
  })
})

describe('typedCharacterCombo', () => {
  it('names the character ⌥ typed, the way AltGr types symbols on PC layouts', () => {
    setMacOS(true)
    expect(typedCharacterCombo(makeKeyEvent({ key: '*', code: 'Slash', altKey: true }))).toBe('*')
    expect(typedCharacterCombo(makeKeyEvent({ key: '*', code: 'Slash', altKey: true, shiftKey: true }))).toBe('*')
  })

  it('stays out of everything else', () => {
    setMacOS(true)
    expect(typedCharacterCombo(makeKeyEvent({ key: '*', code: 'Digit8', shiftKey: true }))).toBeNull()
    expect(typedCharacterCombo(makeKeyEvent({ key: '*', code: 'Slash', altKey: true, metaKey: true }))).toBeNull()
    expect(typedCharacterCombo(makeKeyEvent({ key: '*', code: 'Slash', altKey: true, ctrlKey: true }))).toBeNull()
    expect(typedCharacterCombo(makeKeyEvent({ key: 'å', code: 'KeyA', altKey: true }))).toBeNull()
    expect(typedCharacterCombo(makeKeyEvent({ key: ' ', code: 'Space', altKey: true }))).toBeNull()
    expect(typedCharacterCombo(makeKeyEvent({ key: 'Dead', code: 'KeyE', altKey: true }))).toBeNull()
  })
})

// Regression: on a Russian layout, File List bindings (dispatched from JS) were dead,
// while Main Window ones (native menu accelerators) worked (GitHub #1044).
describe('latinKeyCombo', () => {
  it('names a non-Latin letter by its key position, with any modifiers', () => {
    setMacOS(true)
    expect(latinKeyCombo(makeKeyEvent({ key: 'в', code: 'KeyD' }))).toBe('D')
    expect(latinKeyCombo(makeKeyEvent({ key: 'В', code: 'KeyD', shiftKey: true }))).toBe('⇧D')
    expect(latinKeyCombo(makeKeyEvent({ key: 'с', code: 'KeyC', ctrlKey: true }))).toBe('⌃C')
    expect(latinKeyCombo(makeKeyEvent({ key: 'с', code: 'KeyC', metaKey: true }))).toBe('⌘C')
    expect(latinKeyCombo(makeKeyEvent({ key: 'ψ', code: 'KeyC', metaKey: true, shiftKey: true }))).toBe('⌘⇧C')
    expect(latinKeyCombo(makeKeyEvent({ key: 'ש', code: 'KeyA', ctrlKey: true }))).toBe('⌃A')
  })

  it('leaves Latin layouts alone, Dvorak and ⌥-typed letters included', () => {
    setMacOS(true)
    // Dvorak's J sits on KeyC: the user reads J, so J it stays.
    expect(latinKeyCombo(makeKeyEvent({ key: 'j', code: 'KeyC', metaKey: true }))).toBeNull()
    expect(latinKeyCombo(makeKeyEvent({ key: 'å', code: 'KeyA', altKey: true, metaKey: true }))).toBeNull()
    expect(latinKeyCombo(makeKeyEvent({ key: 'a', code: 'KeyA' }))).toBeNull()
  })

  it('stays out of bare ⌥ (typing), non-letter keys, and non-letter characters', () => {
    setMacOS(true)
    // US ⌥P types π: that's typing, like ⌥A → å.
    expect(latinKeyCombo(makeKeyEvent({ key: 'π', code: 'KeyP', altKey: true }))).toBeNull()
    expect(latinKeyCombo(makeKeyEvent({ key: 'х', code: 'BracketLeft' }))).toBeNull()
    expect(latinKeyCombo(makeKeyEvent({ key: '.', code: 'KeyU' }))).toBeNull()
    expect(latinKeyCombo(makeKeyEvent({ key: 'Enter', code: 'Enter' }))).toBeNull()
  })
})

describe('keyComboCandidates', () => {
  it('falls back to the Latin key position for a non-Latin letter', () => {
    setMacOS(true)
    expect(keyComboCandidates(makeKeyEvent({ key: 'В', code: 'KeyD', shiftKey: true }))).toEqual(['⇧В', '⇧D'])
    expect(keyComboCandidates(makeKeyEvent({ key: 'с', code: 'KeyC', metaKey: true }))).toEqual(['⌘С', '⌘C'])
  })

  it('lists the exact combo, then the physical key, then the typed character, without repeats', () => {
    setMacOS(true)
    expect(keyComboCandidates(makeKeyEvent({ key: '±', code: 'Equal', altKey: true, shiftKey: true }))).toEqual([
      '⌥⇧±',
      '⌥⇧=',
      '±',
    ])
    expect(keyComboCandidates(makeKeyEvent({ key: '*', code: 'Digit8', shiftKey: true }))).toEqual(['*'])
    expect(keyComboCandidates(makeKeyEvent({ key: 'a', code: 'KeyA', metaKey: true }))).toEqual(['⌘A'])
  })
})

describe('capturedKeyCombo', () => {
  it('records what the Settings capture field persists', () => {
    setMacOS(true)
    expect(capturedKeyCombo(makeKeyEvent({ key: '*', code: 'Digit8', shiftKey: true }))).toBe('*')
    expect(capturedKeyCombo(makeKeyEvent({ key: '*', code: 'NumpadMultiply' }))).toBe('*')
    expect(capturedKeyCombo(makeKeyEvent({ key: '±', code: 'Equal', altKey: true, shiftKey: true }))).toBe('⌥⇧=')
    expect(capturedKeyCombo(makeKeyEvent({ key: 'Help', code: 'Help' }))).toBe('Insert')
    expect(capturedKeyCombo(makeKeyEvent({ key: 'Delete', code: 'Delete' }))).toBe('Delete')
  })

  it('records a non-Latin letter by its Latin key position, so the binding works on every layout', () => {
    setMacOS(true)
    expect(capturedKeyCombo(makeKeyEvent({ key: 'В', code: 'KeyD', shiftKey: true }))).toBe('⇧D')
    expect(capturedKeyCombo(makeKeyEvent({ key: 'с', code: 'KeyC', ctrlKey: true }))).toBe('⌃C')
  })
})

/**
 * AltGr types characters. Windows reports it as Ctrl+Alt (plus `AltGraph`), so without
 * the guard German AltGr+Q (`@`) also meant `Ctrl+Alt+Q` by key position, and ran
 * whatever holds it. Layouts per Microsoft's KLC tables (kbdlayout.info).
 */
describe('AltGr is typing off macOS', () => {
  const windowsAltGr = (key: string, code: string, extra: KeyboardEventInit = {}) =>
    keydownWithAltGraph({ key, code, ctrlKey: true, altKey: true, ...extra })

  it.each([
    ['German AltGr+Q', windowsAltGr('@', 'KeyQ'), ['Ctrl+Alt+@', '@']],
    ['Hungarian AltGr+F', windowsAltGr('[', 'KeyF'), ['Ctrl+Alt+[', '[']],
    ['Hungarian AltGr+V', windowsAltGr('@', 'KeyV'), ['Ctrl+Alt+@', '@']],
    ['Hungarian AltGr+- (types *)', windowsAltGr('*', 'Slash'), ['Ctrl+Alt+*', '*']],
    ['Polish AltGr+A', windowsAltGr('ą', 'KeyA'), ['Ctrl+Alt+Ą']],
  ])('Windows %s: the exact combo, then the typed character, never the key position', (_name, event, expected) => {
    onTestFinished(withPlatform('windows'))
    expect(physicalKeyCombo(event)).toBeNull()
    expect(latinKeyCombo(event)).toBeNull()
    expect(keyComboCandidates(event)).toEqual(expected)
  })

  it('leaves a US layout, which has no AltGr, as it was', () => {
    onTestFinished(withPlatform('windows'))
    const ctrlAltQ = new KeyboardEvent('keydown', { key: 'q', code: 'KeyQ', ctrlKey: true, altKey: true })
    expect(keyComboCandidates(ctrlAltQ)).toEqual(['Ctrl+Alt+Q'])
    const ctrlShiftPeriod = new KeyboardEvent('keydown', { key: '>', code: 'Period', ctrlKey: true, shiftKey: true })
    expect(keyComboCandidates(ctrlShiftPeriod)).toEqual(['Ctrl+Shift+>', 'Ctrl+Shift+.'])
  })

  it('treats a Linux AltGr, which sets no Ctrl or Alt, as the character it typed', () => {
    onTestFinished(withPlatform('linux'))
    expect(keyComboCandidates(keydownWithAltGraph({ key: '@', code: 'KeyQ' }))).toEqual(['@'])
    expect(keyComboCandidates(keydownWithAltGraph({ key: 'ą', code: 'KeyA' }))).toEqual(['Ą'])
  })

  it('still lets Super make a command: AltGr only owns Ctrl and Alt', () => {
    onTestFinished(withPlatform('windows'))
    expect(typedCharacterCombo(windowsAltGr('*', 'Slash', { metaKey: true }))).toBeNull()
  })

  it('names an AltGr dead key Dead, so the first half of a character resolves to no binding', () => {
    // Hungarian AltGr+1 starts `~`. The key-position stand-in macOS needs (`⌘⌥E`) would make
    // it `Ctrl+Alt+1` on Windows and a bare `1` on Linux, both bindable.
    for (const [platform, event, expected] of [
      ['windows', windowsAltGr('Dead', 'Digit1'), ['Ctrl+Alt+Dead']],
      ['linux', keydownWithAltGraph({ key: 'Dead', code: 'Digit1' }), ['Dead']],
    ] as const) {
      const undo = withPlatform(platform)
      expect(keyComboCandidates(event)).toEqual(expected)
      undo()
    }
  })

  it('records an AltGr keypress by what it typed', () => {
    onTestFinished(withPlatform('windows'))
    expect(capturedKeyCombo(windowsAltGr('@', 'KeyQ'))).toBe('Ctrl+Alt+@')
  })

  // The guard is off on macOS by construction: whether WebKit reports `AltGraph` for ⌥ is
  // unverified, and if it did, a platform-blind guard would kill ⌘⌥R, ⌥⇧=, and every ⌥ match.
  it.each([
    ['⌥⇧= (types ±)', { key: '±', code: 'Equal', altKey: true, shiftKey: true }],
    ['⌘⌥R (types ®)', { key: '®', code: 'KeyR', metaKey: true, altKey: true }],
    ['⌃⌥F (types ƒ)', { key: 'ƒ', code: 'KeyF', ctrlKey: true, altKey: true }],
    ['⌘⌥E (dead)', { key: 'Dead', code: 'KeyE', metaKey: true, altKey: true }],
    ['⌥ *', { key: '*', code: 'Slash', altKey: true }],
    ['numpad ⌥+', { key: '+', code: 'NumpadAdd', altKey: true }],
    ['Russian ⌘C', { key: 'с', code: 'KeyC', metaKey: true }],
    ['⌘ IntlBackslash (Hungarian í)', { key: 'í', code: 'IntlBackslash', metaKey: true }],
  ])('macOS %s resolves the same with AltGraph reported', (_name, init) => {
    onTestFinished(withPlatform('macos'))
    const plain = new KeyboardEvent('keydown', init)
    const withAltGraph = keydownWithAltGraph(init)
    expect(keyComboCandidates(withAltGraph)).toEqual(keyComboCandidates(plain))
    expect(capturedKeyCombo(withAltGraph)).toBe(capturedKeyCombo(plain))
  })
})

/**
 * Go to root's `Ctrl+<` is the ISO key left of Z (`IntlBackslash`) by position: it types `<`
 * on German and Nordic layouts, `í` on Hungarian, and `\` on UK. A US ANSI keyboard has no
 * such key.
 */
describe.each(['windows', 'linux'] as const)('the IntlBackslash key on %s', (platform) => {
  const ctrl = (key: string, extra: KeyboardEventInit = {}) =>
    new KeyboardEvent('keydown', { key, code: 'IntlBackslash', ctrlKey: true, ...extra })

  it.each([
    ['German', '<', ['Ctrl+<']],
    ['Swedish', '<', ['Ctrl+<']],
    ['Hungarian', 'í', ['Ctrl+Í', 'Ctrl+<']],
    ['UK', '\\', ['Ctrl+\\', 'Ctrl+<']],
  ])('%s Ctrl+IntlBackslash also means Ctrl+<', (_layout, key, expected) => {
    onTestFinished(withPlatform(platform))
    expect(keyComboCandidates(ctrl(key))).toEqual(expected)
  })

  it('names it with Super alone too, and not with Shift or Alt added', () => {
    onTestFinished(withPlatform(platform))
    const superKey = new KeyboardEvent('keydown', { key: 'í', code: 'IntlBackslash', metaKey: true })
    expect(keyComboCandidates(superKey)).toEqual(['Super+Í', 'Super+<'])
    expect(keyComboCandidates(ctrl('>', { shiftKey: true }))).toEqual(['Ctrl+Shift+>'])
    expect(physicalKeyCombo(ctrl('í', { altKey: true }))).toBeNull()
  })

  it("leaves a US keyboard's Ctrl+Shift+, its own combo", () => {
    onTestFinished(withPlatform(platform))
    const usCtrlShiftComma = new KeyboardEvent('keydown', { key: '<', code: 'Comma', ctrlKey: true, shiftKey: true })
    expect(keyComboCandidates(usCtrlShiftComma)).toEqual(['Ctrl+Shift+<', 'Ctrl+Shift+,'])
  })
})

describe('the IntlBackslash key on macOS', () => {
  it('keeps the rule off: no macOS default needs it, and Mac ISO keyboards may swap its code', () => {
    onTestFinished(withPlatform('macos'))
    expect(
      keyComboCandidates(new KeyboardEvent('keydown', { key: 'í', code: 'IntlBackslash', metaKey: true })),
    ).toEqual(['⌘Í'])
  })
})

describe('canonical vs display forms', () => {
  it('formats a keypress into the canonical word form, not a macOS glyph', () => {
    setMacOS(true)
    expect(formatKeyCombo(makeKeyEvent({ key: 'Enter' }))).toBe('Enter')
    expect(formatKeyCombo(makeKeyEvent({ metaKey: true, key: 'Backspace' }))).toBe('⌘Backspace')
    expect(formatKeyCombo(makeKeyEvent({ key: 'Escape' }))).toBe('Escape')
    expect(formatKeyCombo(makeKeyEvent({ key: 'PageUp' }))).toBe('PageUp')
  })

  it('renders macOS glyphs for display', () => {
    setMacOS(true)
    expect(toDisplayShortcut('Enter')).toBe('↩')
    expect(toDisplayShortcut('⌘Backspace')).toBe('⌘⌫')
    expect(toDisplayShortcut('PageDown')).toBe('PgDn')
    expect(toDisplayShortcut('')).toBe('')
  })

  it('shows macOS modifiers in the menu bar order (⌃⌥⇧⌘), whatever order they are stored in', () => {
    setMacOS(true)
    expect(toDisplayShortcut('⌘⇧P')).toBe('⇧⌘P')
    expect(toDisplayShortcut('⌘⇧.')).toBe('⇧⌘.')
    expect(toDisplayShortcut('⌘⌥Escape')).toBe('⌥⌘⎋')
    expect(toDisplayShortcut('⌘⌃⌥J')).toBe('⌃⌥⌘J')
    expect(toDisplayShortcut('⌘⌃⌥⇧K')).toBe('⌃⌥⇧⌘K')
    expect(toDisplayShortcut('⌥⇧=')).toBe('⌥⇧=')
    expect(toDisplayShortcut('⇧F8')).toBe('⇧F8')
    // Only modifiers held so far, as the capture field shows mid-press.
    expect(toDisplayShortcut('⌘⇧')).toBe('⇧⌘')
    expect(toDisplayShortcut(toDisplayShortcut('⌘⇧P'))).toBe('⇧⌘P')
  })

  it('restores the stored modifier order from a displayed or Apple-ordered combo', () => {
    setMacOS(true)
    expect(toCanonicalShortcut('⇧⌘P')).toBe('⌘⇧P')
    expect(toCanonicalShortcut('⌃⌥⇧⌘K')).toBe('⌘⌃⌥⇧K')
    expect(toCanonicalShortcut(toDisplayShortcut('⌘⌥Backspace'))).toBe('⌘⌥Backspace')
  })

  it('uses the familiar abbreviations off macOS', () => {
    setMacOS(false)
    expect(toDisplayShortcut('Escape')).toBe('Esc')
    expect(toDisplayShortcut('Ctrl+Backspace')).toBe('Ctrl+Backspace')
  })

  it('names the Windows key "Win" on Windows and "Super" on Linux, while the stored token stays Super', () => {
    onTestFinished(withPlatform('windows'))
    expect(toDisplayShortcut('Super+E')).toBe('Win+E')
    expect(toDisplayShortcut('Ctrl+Alt+Super+J')).toBe('Ctrl+Alt+Win+J')
    expect(toDisplayShortcut('Super+')).toBe('Win+')
    expect(toDisplayShortcut(toDisplayShortcut('Super+Escape'))).toBe('Win+Esc')
    expect(toCanonicalShortcut('Win+E')).toBe('Super+E')
    expect(toCanonicalShortcut('Ctrl+Win+')).toBe('Ctrl+Super+')
    withPlatform('linux')
    expect(toDisplayShortcut('Super+E')).toBe('Super+E')
    withPlatform('macos')
    expect(toDisplayShortcut('⌘E')).toBe('⌘E')
  })

  it('is idempotent, so wrapping an already-displayed value is safe', () => {
    setMacOS(true)
    expect(toDisplayShortcut(toDisplayShortcut('⌘Backspace'))).toBe('⌘⌫')
    expect(toCanonicalShortcut(toCanonicalShortcut('⌘⌫'))).toBe('⌘Backspace')
  })

  it('heals legacy and display spellings back to canonical', () => {
    setMacOS(true)
    expect(toCanonicalShortcut('↩')).toBe('Enter')
    expect(toCanonicalShortcut('⌘⌫')).toBe('⌘Backspace')
    expect(toCanonicalShortcut('⌥⌘⎋')).toBe('⌘⌥Escape')
    expect(toCanonicalShortcut('PgUp')).toBe('PageUp')
    expect(toCanonicalShortcut('Ctrl+Esc')).toBe('Ctrl+Escape')
    expect(toCanonicalShortcut('⌘A')).toBe('⌘A')
  })

  it('heals a Shift-only US key position to the character it typed', () => {
    setMacOS(true)
    // Stored before typed symbols were named by their character: no keypress can
    // produce `⇧8` any more, and on the US layout those names were written on it meant `*`.
    expect(toCanonicalShortcut('⇧8')).toBe('*')
    expect(toCanonicalShortcut('⇧=')).toBe('+')
    expect(toCanonicalShortcut('⇧-')).toBe('_')
    expect(toCanonicalShortcut('⇧/')).toBe('?')
    expect(toCanonicalShortcut('Shift+8')).toBe('*')
    expect(toCanonicalShortcut('⇧A')).toBe('⇧A')
    expect(toCanonicalShortcut('⇧F8')).toBe('⇧F8')
    expect(toCanonicalShortcut('⌘⇧8')).toBe('⌘⇧8')
    expect(toCanonicalShortcut('⌥⇧=')).toBe('⌥⇧=')
  })
})

describe('isTypingKeyCombo', () => {
  it('treats bare keys as typing (Tab, letters, Space, Enter)', () => {
    expect(isTypingKeyCombo('Tab')).toBe(true)
    expect(isTypingKeyCombo('A')).toBe(true)
    expect(isTypingKeyCombo('Space')).toBe(true)
    expect(isTypingKeyCombo('Enter')).toBe(true)
  })

  it('refuses Shift plus a letter in every scope', () => {
    for (const combo of ['⇧A', '⇧Ő', '⇧Д', 'Shift+A']) {
      expect(typingRefusal(combo, 'Main window/File list')).toBe('shiftLetter')
      expect(typingRefusal(combo, 'Viewer window')).toBe('shiftLetter')
    }
  })

  it('refuses a bare letter or digit in the main window, where typing takes it first', () => {
    for (const combo of ['A', 'Ő', 'Д', '7']) {
      for (const scope of ['App', 'Main window', 'Main window/File list', 'Main window/Servers'] as const) {
        expect(typingRefusal(combo, scope)).toBe('bareKey')
      }
    }
  })

  it('allows a bare letter or digit in a window with no typing intercept', () => {
    for (const scope of ['Viewer window', 'Operation queue window', 'About window'] as const) {
      expect(typingRefusal('W', scope)).toBe(null)
      expect(typingRefusal('1', scope)).toBe(null)
    }
  })

  it('allows everything that reaches a command', () => {
    const combos = ['⇧Space', '⇧Tab', '⇧F5', '⇧↓', '*', '-', 'Space', 'F5', 'Enter', '⌘⇧A', '⌥⇧A', '⌘A', 'Ctrl+Shift+A']
    for (const combo of [...combos, 'Shift+F5', '']) expect(typingRefusal(combo, 'Main window/File list')).toBe(null)
  })

  it('treats shift-only combos as typing (⇧Tab reverse-tab, ⇧A capital letter)', () => {
    expect(isTypingKeyCombo('⇧Tab')).toBe(true)
    expect(isTypingKeyCombo('⇧A')).toBe(true)
    expect(isTypingKeyCombo('Shift+Tab')).toBe(true)
  })

  it('keeps command-modifier combos live (⌘, ⌃, ⌥, Ctrl, Alt, Super)', () => {
    expect(isTypingKeyCombo('⌘C')).toBe(false)
    expect(isTypingKeyCombo('⌃X')).toBe(false)
    expect(isTypingKeyCombo('⌥↓')).toBe(false)
    expect(isTypingKeyCombo('Ctrl+C')).toBe(false)
    expect(isTypingKeyCombo('Alt+Tab')).toBe(false)
    expect(isTypingKeyCombo('Super+Space')).toBe(false)
  })

  it('keeps F-keys and Escape live (never typing)', () => {
    expect(isTypingKeyCombo('F5')).toBe(false)
    expect(isTypingKeyCombo('F12')).toBe(false)
    expect(isTypingKeyCombo('⇧F6')).toBe(false)
    expect(isTypingKeyCombo('Escape')).toBe(false)
  })
})

describe('comboHasShift', () => {
  it('recognizes Shift in both platform spellings', () => {
    expect(comboHasShift('⇧F6')).toBe(true)
    expect(comboHasShift('Shift+F6')).toBe(true)
    expect(comboHasShift('⌘⇧P')).toBe(true)
    expect(comboHasShift('Ctrl+Alt+Shift+P')).toBe(true)
  })

  it('rejects combos without Shift', () => {
    expect(comboHasShift('F2')).toBe(false)
    expect(comboHasShift('⌘C')).toBe(false)
    expect(comboHasShift('Ctrl+C')).toBe(false)
    expect(comboHasShift('Super+Space')).toBe(false)
    expect(comboHasShift('')).toBe(false)
  })
})

describe('toAriaKeyShortcut', () => {
  it('passes a bare key through', () => {
    expect(toAriaKeyShortcut('F2')).toBe('F2')
  })

  it('spells the macOS modifier glyphs as ARIA modifier names', () => {
    expect(toAriaKeyShortcut('⇧F6')).toBe('Shift+F6')
    expect(toAriaKeyShortcut('⌘C')).toBe('Meta+C')
    expect(toAriaKeyShortcut('⌥F')).toBe('Alt+F')
    expect(toAriaKeyShortcut('⌘⌃⌥⇧K')).toBe('Meta+Control+Alt+Shift+K')
  })

  it('spells the Windows and Linux form as ARIA modifier names', () => {
    expect(toAriaKeyShortcut('Ctrl+Shift+P')).toBe('Control+Shift+P')
    expect(toAriaKeyShortcut('Alt+F1')).toBe('Alt+F1')
    expect(toAriaKeyShortcut('Super+A')).toBe('Meta+A')
  })

  it('names a display-form key by its KeyboardEvent.key name', () => {
    expect(toAriaKeyShortcut('⌘⌫')).toBe('Meta+Backspace')
    expect(toAriaKeyShortcut('Esc')).toBe('Escape')
  })

  it('spells the keys ARIA reserves a word for', () => {
    expect(toAriaKeyShortcut('Ctrl++')).toBe('Control+Plus')
    expect(toAriaKeyShortcut('⌘+')).toBe('Meta+Plus')
    expect(toAriaKeyShortcut('⇧Space')).toBe('Shift+Space')
  })
})
