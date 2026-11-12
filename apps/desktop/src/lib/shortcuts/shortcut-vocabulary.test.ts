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
 * (`'Enter'` vs `'↩'`), an abbreviation (`'PageUp'` vs `'PgUp'`), Apple's display
 * modifier order (`'⌥⌘A'` vs the emitted `'⌘⌥A'`), or a symbol spelled as Shift plus
 * a US key (`'⇧8'` or `'⇧*'` for `'*'`): a typed symbol is named by its character, so
 * that default would only work on some layouts.
 *
 * It runs per platform over every command's RAW arm for that platform, capability ignored:
 * a gated command's keys must be spelled right before its capability flips on.
 */

import { describe, it, expect, beforeAll } from 'vitest'
import type { Platform } from '#lib/platform/index.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import { commands } from '#lib/commands/command-registry.js'
import { PLATFORMS } from '#lib/commands/platform-shortcuts.js'
import { classifySystemShortcut } from '#lib/settings/sections/keyboard-shortcuts-banner.js'
import { formatKeyCombo } from './key-capture'

type ModifierFlag = 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'

/**
 * Each platform's modifier tokens, in the order `formatKeyCombo` emits them, with the event
 * flag each one sets: ⌘⌃⌥⇧ glyphs on macOS, `Ctrl+Alt+Shift+Super` words elsewhere.
 */
const offMacModifierTokens: readonly [token: string, flag: ModifierFlag][] = [
  ['Ctrl+', 'ctrlKey'],
  ['Alt+', 'altKey'],
  ['Shift+', 'shiftKey'],
  ['Super+', 'metaKey'],
]
const modifierTokens: Record<Platform, readonly [token: string, flag: ModifierFlag][]> = {
  macos: [
    ['⌘', 'metaKey'],
    ['⌃', 'ctrlKey'],
    ['⌥', 'altKey'],
    ['⇧', 'shiftKey'],
  ],
  windows: offMacModifierTokens,
  linux: offMacModifierTokens,
}

/** Stored key token → the `event.key` a real keypress carries. */
const keyTokenToEventKey: Record<string, string> = {
  '↑': 'ArrowUp',
  '↓': 'ArrowDown',
  '←': 'ArrowLeft',
  '→': 'ArrowRight',
  Space: ' ',
}

/** Splits a stored combo into its leading modifier tokens (in written order) and its key. */
function parseCombo(shortcut: string, platform: Platform): { tokens: string[]; key: string } {
  const tokens: string[] = []
  let rest = shortcut
  for (;;) {
    // The key itself may look like a token's tail (`Ctrl++`), so a token never eats the last char.
    const token = modifierTokens[platform].find(([t]) => rest.startsWith(t) && rest.length > t.length)?.[0]
    if (token === undefined) break
    tokens.push(token)
    rest = rest.slice(token.length)
  }
  return { tokens, key: rest }
}

/**
 * Turn a stored shortcut string back into the keydown that would produce it.
 * Letters go back to lowercase (an unshifted `A` key reports `'a'`), everything
 * else is the token itself unless the table above remaps it.
 */
function keydownFor(shortcut: string, platform: Platform): KeyboardEvent {
  const event = { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, key: '', code: '' }
  const { tokens, key } = parseCombo(shortcut, platform)
  for (const token of tokens) {
    const flag = modifierTokens[platform].find(([t]) => t === token)?.[1]
    if (flag) event[flag] = true
  }
  event.key = keyTokenToEventKey[key] ?? (key.length === 1 ? key.toLowerCase() : key)
  return event as unknown as KeyboardEvent
}

/**
 * macOS-native commands are exempt on macOS: AppKit owns both their behavior and their
 * accelerator there, so their strings are pure display that mirrors what the OS menu
 * shows (`⌥⌘H`), and nothing ever dispatches them from a keydown.
 */
function dispatchableDefaults(platform: Platform): (readonly [string, string])[] {
  return commands
    .filter((command) => platform !== 'macos' || !command.nativeShortcut)
    .flatMap((command) => command.shortcuts[platform].map((shortcut) => [command.id, shortcut] as const))
}

describe.each(PLATFORMS)('registry default shortcuts on %s', (platform) => {
  beforeAll(() => withPlatform(platform))

  it.each(dispatchableDefaults(platform))('%s: %s is reachable by a real keypress', (_id, shortcut) => {
    expect(formatKeyCombo(keydownFor(shortcut, platform))).toBe(shortcut)
  })

  it('spells modifiers in the order formatKeyCombo emits, not Apple display order', () => {
    const emittedOrder = modifierTokens[platform].map(([token]) => token)
    const misordered = dispatchableDefaults(platform).filter(([, shortcut]) => {
      const positions = parseCombo(shortcut, platform).tokens.map((token) => emittedOrder.indexOf(token))
      return positions.some((position, i) => i > 0 && position < positions[i - 1])
    })
    expect(misordered).toEqual([])
  })
})

describe('macOS system-shortcut warnings', () => {
  beforeAll(() => withPlatform('macos'))

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
