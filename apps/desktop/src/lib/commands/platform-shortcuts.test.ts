import { describe, expect, it } from 'vitest'
import { cmdOrCtrl, concatShortcuts, macOnly, sameOnAll } from './platform-shortcuts'

/** The helpers' runtime refusals, reached past the type-level ones. */
const untypedSameOnAll = sameOnAll as (...combos: string[]) => unknown
const untypedCmdOrCtrl = cmdOrCtrl as (...combos: string[]) => unknown

describe('sameOnAll', () => {
  it('keeps the macOS spelling as written and writes Alt and Shift words elsewhere', () => {
    expect(sameOnAll('F5', '⇧F6', '⌥F7', '⌥⇧=')).toEqual({
      macos: ['F5', '⇧F6', '⌥F7', '⌥⇧='],
      windows: ['F5', 'Shift+F6', 'Alt+F7', 'Alt+Shift+='],
      linux: ['F5', 'Shift+F6', 'Alt+F7', 'Alt+Shift+='],
    })
  })

  it('spells "no key" as three empty lists', () => {
    expect(sameOnAll()).toEqual({ macos: [], windows: [], linux: [] })
  })

  it('reads a one-character key that looks like a modifier glyph as the key', () => {
    expect(sameOnAll('⌥+', '+')).toEqual({ macos: ['⌥+', '+'], windows: ['Alt++', '+'], linux: ['Alt++', '+'] })
  })

  it('refuses ⌘ and ⌃, at the type level and at load', () => {
    // @ts-expect-error ⌘ means a different key per platform: cmdOrCtrl says which
    expect(() => sameOnAll('⌘F')).toThrow(/sameOnAll/)
    // @ts-expect-error ⌃ needs the literal
    expect(() => sameOnAll('F5', '⌃D')).toThrow(/sameOnAll/)
    expect(() => untypedSameOnAll('⌥⌘F')).toThrow(/sameOnAll/)
  })
})

describe('cmdOrCtrl', () => {
  it('puts ⌘ on Ctrl off macOS, in canonical Ctrl+Alt+Shift order whatever the authored order', () => {
    expect(cmdOrCtrl('⌘F', '⌘⇧P', '⌘⌥⇧←', '⌥⌘H')).toEqual({
      macos: ['⌘F', '⌘⇧P', '⌘⌥⇧←', '⌥⌘H'],
      windows: ['Ctrl+F', 'Ctrl+Shift+P', 'Ctrl+Alt+Shift+←', 'Ctrl+Alt+H'],
      linux: ['Ctrl+F', 'Ctrl+Shift+P', 'Ctrl+Alt+Shift+←', 'Ctrl+Alt+H'],
    })
  })

  it('keeps a + key after the modifiers', () => {
    expect(cmdOrCtrl('⌘+').linux).toEqual(['Ctrl++'])
  })

  it('refuses a combo without ⌘ or with ⌃, at the type level and at load', () => {
    // @ts-expect-error no ⌘: sameOnAll
    expect(() => cmdOrCtrl('F5')).toThrow(/cmdOrCtrl/)
    // @ts-expect-error ⌃ would land on the same Ctrl as ⌘
    expect(() => cmdOrCtrl('⌘⌃⇧→')).toThrow(/cmdOrCtrl/)
    expect(() => untypedCmdOrCtrl('⌃D')).toThrow(/cmdOrCtrl/)
  })
})

describe('macOnly', () => {
  it('holds keys on macOS and none elsewhere', () => {
    expect(macOnly('⌘H')).toEqual({ macos: ['⌘H'], windows: [], linux: [] })
  })
})

describe('concatShortcuts', () => {
  it('joins arm by arm, in argument order', () => {
    expect(concatShortcuts(sameOnAll('Backspace'), cmdOrCtrl('⌘↑'), sameOnAll('Delete'))).toEqual({
      macos: ['Backspace', '⌘↑', 'Delete'],
      windows: ['Backspace', 'Ctrl+↑', 'Delete'],
      linux: ['Backspace', 'Ctrl+↑', 'Delete'],
    })
  })

  it('joins a literal arm as written', () => {
    const literal = { macos: ['⌃Tab'], windows: ['Ctrl+Tab'], linux: ['Ctrl+Tab'] }
    expect(concatShortcuts(literal, macOnly('⌘T')).windows).toEqual(['Ctrl+Tab'])
  })

  it('throws on a combo repeated within an arm', () => {
    const ctrlD = { macos: ['⌃D'], windows: ['Ctrl+D'], linux: ['Ctrl+D'] }
    expect(() => concatShortcuts(cmdOrCtrl('⌘D'), ctrlD)).toThrow(/repeated on windows/)
  })
})
