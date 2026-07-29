/**
 * The shortcut vocabulary contract: every string the registry (or a system-shortcut
 * table) holds must be the EXACT string a real keypress produces.
 *
 * `formatKeyCombo` is the single writer of that vocabulary — it feeds both central
 * dispatch and the persisted rebind — so a default spelled any other way is dead on
 * the keyboard. Each case below reconstructs a synthetic keydown from the stored
 * string and demands `formatKeyCombo` reproduce it byte for byte.
 *
 * This is what catches the whole family of silent breakage: a word-vs-symbol split
 * (`'Enter'` vs `'↩'`), an abbreviation (`'PageUp'` vs `'PgUp'`), or Apple's display
 * modifier order (`'⌥⌘A'` vs the emitted `'⌘⌥A'`).
 */

import { describe, it, expect, vi, beforeAll, afterAll } from 'vitest'
import { commands } from '$lib/commands/command-registry'
import { classifySystemShortcut } from '$lib/settings/sections/keyboard-shortcuts-banner'
import { formatKeyCombo } from './key-capture'

const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeAll(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterAll(() => navigatorSpy.mockReset())

/** The modifier symbols `formatKeyCombo` emits, mapped to the event flag each one sets. */
const modifierSymbolToFlag = {
  '⌘': 'metaKey',
  '⌃': 'ctrlKey',
  '⌥': 'altKey',
  '⇧': 'shiftKey',
} as const satisfies Record<string, keyof KeyboardEvent>

/** Stored key token → the `event.key` a real keypress carries. */
const keyTokenToEventKey: Record<string, string> = {
  '↑': 'ArrowUp',
  '↓': 'ArrowDown',
  '←': 'ArrowLeft',
  '→': 'ArrowRight',
  Space: ' ',
}

/**
 * Turn a stored shortcut string back into the keydown that would produce it.
 * Letters go back to lowercase (an unshifted `A` key reports `'a'`), everything
 * else is the token itself unless the table above remaps it.
 */
function keydownFor(shortcut: string): KeyboardEvent {
  const event = { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, key: '', code: '' }

  let rest = shortcut
  while (rest.length > 0 && rest[0] in modifierSymbolToFlag) {
    event[modifierSymbolToFlag[rest[0] as keyof typeof modifierSymbolToFlag]] = true
    rest = rest.slice(1)
  }

  event.key = keyTokenToEventKey[rest] ?? (rest.length === 1 ? rest.toLowerCase() : rest)
  return event as unknown as KeyboardEvent
}

/**
 * macOS-native commands are exempt: AppKit owns both their behavior and their
 * accelerator, so their strings are pure display that mirrors what the OS menu
 * shows (`⌥⌘H`), and nothing ever dispatches them from a keydown.
 */
const dispatchableDefaults = commands
  .filter((command) => !command.nativeShortcut)
  .flatMap((command) => command.shortcuts.map((shortcut) => [command.id, shortcut] as const))

describe('registry default shortcuts', () => {
  it.each(dispatchableDefaults)('%s: %s is reachable by a real keypress', (_id, shortcut) => {
    expect(formatKeyCombo(keydownFor(shortcut))).toBe(shortcut)
  })

  it('spells modifiers in the ⌘⌃⌥⇧ order formatKeyCombo emits, not Apple display order', () => {
    const emittedOrder = ['⌘', '⌃', '⌥', '⇧']
    const misordered = dispatchableDefaults.filter(([, shortcut]) => {
      const positions = (shortcut.match(/[⌘⌃⌥⇧]/gu) ?? []).map((char) => emittedOrder.indexOf(char))
      return positions.some((position, i) => i > 0 && position < positions[i - 1])
    })
    expect(misordered).toEqual([])
  })
})

describe('macOS system-shortcut warnings', () => {
  // The table is keyed by the display strings `formatKeyCombo` produces, so a
  // combo spelled Apple's way there silently never warns.
  it.each([
    ['Spotlight (⌘Space)', { metaKey: true, key: ' ' }],
    ['Finder search (⌥⌘Space)', { metaKey: true, altKey: true, key: ' ' }],
    ['Force quit (⌥⌘Escape)', { metaKey: true, altKey: true, key: 'Escape' }],
    ['Lock screen (⌃⌘Q)', { metaKey: true, ctrlKey: true, key: 'q' }],
  ])('warns for %s', (_name, event) => {
    const combo = formatKeyCombo(event as KeyboardEvent)
    expect(classifySystemShortcut(combo)).not.toBeNull()
  })
})
