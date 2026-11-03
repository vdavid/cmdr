/**
 * Tests for the clips' key captions (`marketing-clips-keys.ts`): the KeyCastr-style labels,
 * and that each command a clip dispatches is captioned with the shortcut the app binds.
 */

import { describe, it, expect } from 'vitest'
import { commands } from '../../src/lib/commands/command-registry'
import { COMMAND_CAPTIONS, keyLabel } from './marketing-clips-keys.js'

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
    for (const [id, caption] of Object.entries(COMMAND_CAPTIONS)) {
      const command = commands.find((c) => c.id === id)
      expect(command?.shortcuts[0], id).toBe(caption)
    }
  })
})
