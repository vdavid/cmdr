/**
 * Tests for the clips' key captions (`marketing-clips-keys.ts`): the KeyCastr-style labels,
 * and that each command a clip dispatches is captioned with the shortcut the app binds.
 */

import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { COMMAND_CAPTIONS, keyLabel } from './marketing-clips-keys.js'

/**
 * A source entry's first macOS key, in each form an entry takes (`lib/commands/platform-shortcuts.ts`):
 * a helper call (`sameOnAll`, `cmdOrCtrl`, `macOnly`, the first part of a `concatShortcuts`), whose
 * argument IS the macOS spelling, or a literal's `macos` arm.
 */
const MAC_FIRST_KEY =
  /shortcuts: (?:(?:concatShortcuts\(\s*)?(?:sameOnAll|cmdOrCtrl|macOnly)\(|\{\s*macos: \[)\s*'([^']*)'/

/**
 * Every command's first macOS default shortcut (the clips are filmed on macOS), read from the
 * registry's source files as text: importing the registry would pull the app's Svelte and i18n
 * modules into this suite's tsconfig, which can't resolve them.
 */
function defaultShortcuts(): Map<string, string> {
  const dir = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'src', 'lib', 'commands', 'sources')
  const found = new Map<string, string>()
  for (const file of readdirSync(dir).filter((f) => f.endsWith('.ts'))) {
    const text = readFileSync(join(dir, file), 'utf8')
    // One chunk per entry, so an entry without shortcuts can't borrow the next one's.
    for (const entry of text.split(/\n\s*id: /).slice(1)) {
      const id = /^'([^']+)'/.exec(entry)?.[1]
      const first = MAC_FIRST_KEY.exec(entry)?.[1]
      if (id !== undefined && first !== undefined && !found.has(id)) found.set(id, first)
    }
  }
  return found
}

describe('keyLabel', () => {
  it('prints modifiers as glyphs in Apple order, whatever order they came in', () => {
    expect(keyLabel('Meta+Shift+n')).toBe('⇧⌘N')
    expect(keyLabel('Shift+Meta+n')).toBe('⇧⌘N')
    expect(keyLabel('Control+Alt+Shift+Meta+k')).toBe('⌃⌥⇧⌘K')
  })

  it('names the keys KeyCastr draws as glyphs or words', () => {
    expect(keyLabel('ArrowDown')).toBe('↓')
    expect(keyLabel('Meta+ArrowUp')).toBe('⌘↑')
    expect(keyLabel('Enter')).toBe('Enter')
    expect(keyLabel('Escape')).toBe('Esc')
    expect(keyLabel('Tab')).toBe('Tab')
    expect(keyLabel('F5')).toBe('F5')
    expect(keyLabel('Meta+[')).toBe('⌘[')
    expect(keyLabel('2')).toBe('2')
  })

  it('reads a trailing plus as the plus key', () => {
    expect(keyLabel('Meta++')).toBe('⌘+')
  })
})

describe('COMMAND_CAPTIONS', () => {
  it("captions each command with the app's default shortcut for it", () => {
    const shortcuts = defaultShortcuts()
    for (const [id, caption] of Object.entries(COMMAND_CAPTIONS)) expect(shortcuts.get(id), id).toBe(caption)
  })
})
