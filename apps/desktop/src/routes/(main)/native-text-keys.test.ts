/**
 * The combos a focused text field owns, per OS, set for set against
 * `docs/notes/windows-port/keymap-decisions.md` § "Native text keys". A change here is a change to
 * that file first.
 */
import { describe, it, expect } from 'vitest'
import { nativeTextKeys } from './native-text-keys'

const WINDOWS_AND_LINUX = {
  copyWithSelection: ['Ctrl+C'],
  fieldOwned: [
    'Ctrl+Insert',
    'Ctrl+←',
    'Ctrl+→',
    'Ctrl+Shift+←',
    'Ctrl+Shift+→',
    'Home',
    'End',
    'Shift+Home',
    'Shift+End',
    'Ctrl+Home',
    'Ctrl+End',
    'Ctrl+Shift+Home',
    'Ctrl+Shift+End',
    'Ctrl+Backspace',
    'Ctrl+Delete',
    'Shift+Delete',
    'Ctrl+Shift+V',
    'Shift+F10',
  ],
  bailInField: ['Ctrl+I'],
}

const EXPECTED = {
  macos: {
    copyWithSelection: ['⌘C'],
    fieldOwned: [
      '⌘←',
      '⌘→',
      '⌘⇧←',
      '⌘⇧→',
      '⌥←',
      '⌥→',
      '⌥⇧←',
      '⌥⇧→',
      '⌘Backspace',
      '⌥Backspace',
      '⌘⌥Backspace',
      '⌘Delete',
      '⌥Delete',
    ],
    bailInField: [],
  },
  windows: WINDOWS_AND_LINUX,
  linux: WINDOWS_AND_LINUX,
} as const

describe.each(['macos', 'windows', 'linux'] as const)('the native text keys on %s', (platform) => {
  const table = nativeTextKeys(platform)

  // A tripwire, not a proof: it compares the table with a hand copy of the decisions file's sets,
  // so a change to one without the other fails here.
  it('match the hand copy of the decisions file’s sets', () => {
    expect([...table.copyWithSelection].sort()).toEqual([...EXPECTED[platform].copyWithSelection].sort())
    expect([...table.fieldOwned].sort()).toEqual([...EXPECTED[platform].fieldOwned].sort())
    expect([...table.bailInField].sort()).toEqual([...EXPECTED[platform].bailInField].sort())
  })

  it('never put one combo in two sets', () => {
    const all = [...table.copyWithSelection, ...table.fieldOwned, ...table.bailInField]
    expect(new Set(all).size).toBe(all.length)
  })
})

it('leaves bare Home and End out on macOS, where they scroll and never move the caret', () => {
  expect(nativeTextKeys('macos').fieldOwned.has('Home')).toBe(false)
  expect(nativeTextKeys('macos').fieldOwned.has('End')).toBe(false)
})
