import { describe, it, expect } from 'vitest'
import { chordOnKeyboard, keyboardFor, type DrawnKey } from './global-shortcut-keyboard'

/** Each key's printed words, for asserting what a keyboard shows. */
function printed(keys: readonly DrawnKey[]): string[] {
  return keys.flatMap((key) => [key.char, key.word, key.symbol].filter((part) => part !== undefined))
}

/** The printed main label of each lit key, sorted. */
function lit(binding: string, platform: Parameters<typeof keyboardFor>[0]): string[] | null {
  const keys = keyboardFor(platform)
  const chord = chordOnKeyboard(binding, keys)
  if (chord === null) return null
  return keys.flatMap((key, i) => (chord.has(i) ? [key.char ?? key.word ?? ''] : [])).sort()
}

describe('keyboardFor', () => {
  it('draws a Mac keyboard on macOS: lowercase words and the modifier glyphs', () => {
    const words = printed(keyboardFor('macos'))
    expect(words).toEqual(expect.arrayContaining(['control', 'option', 'command', '⌃', '⌥', '⌘', 'fn']))
    expect(words).not.toContain('Ctrl')
  })

  it('draws a PC keyboard on Windows, with the Windows key named the way Cmdr names it there', () => {
    const words = printed(keyboardFor('windows'))
    expect(words).toEqual(expect.arrayContaining(['Ctrl', 'Fn', 'Win', 'Alt', 'Shift', 'Caps Lock', 'Tab']))
    expect(words.join('')).not.toMatch(/[⌃⌥⌘]|command|option/)
  })

  it('draws the same PC keyboard on Linux, with the key named Super', () => {
    const words = printed(keyboardFor('linux'))
    expect(words).toContain('Super')
    expect(words).not.toContain('Win')
  })

  it('keeps one geometry on every OS, so the animation lands in the same place', () => {
    const geometry = (platform: Parameters<typeof keyboardFor>[0]) =>
      keyboardFor(platform).map(({ x, y, width }) => [x, y, width])
    expect(geometry('windows')).toEqual(geometry('macos'))
    expect(geometry('linux')).toEqual(geometry('macos'))
  })
})

describe('chordOnKeyboard', () => {
  it('lights the default chord on each OS: the left-hand modifiers and J', () => {
    expect(lit('⌃⌥⌘J', 'macos')).toEqual(['J', 'command', 'control', 'option'])
    expect(lit('Ctrl+Alt+Super+J', 'windows')).toEqual(['Alt', 'Ctrl', 'J', 'Win'])
    expect(lit('Ctrl+Alt+Super+J', 'linux')).toEqual(['Alt', 'Ctrl', 'J', 'Super'])
  })

  it('follows a rebind to any key the keyboard draws, Shift included', () => {
    expect(lit('Ctrl+Shift+K', 'linux')).toEqual(['Ctrl', 'K', 'Shift'])
    expect(lit('⌘⌥;', 'macos')).toEqual([';', 'command', 'option'])
  })

  it('answers null for a chord whose key the keyboard doesn’t draw, so the toast drops the animation', () => {
    expect(lit('Ctrl+Alt+P', 'windows')).toBeNull()
    expect(lit('Ctrl+Alt+F5', 'linux')).toBeNull()
    expect(lit('⌃⌥⌘Space', 'macos')).toBeNull()
    expect(lit('', 'macos')).toBeNull()
  })
})
