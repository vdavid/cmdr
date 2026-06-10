import { describe, it, expect } from 'vitest'
import type { Command } from '$lib/commands/types'
import {
  classifyConflict,
  classifySystemShortcut,
  fixedKeyMessage,
  reservedByMacOsMessage,
  systemShortcutMessage,
} from './keyboard-shortcuts-banner'

function cmd(partial: { id: string; name?: string; nativeShortcut?: true; fixedKey?: true }): Command {
  const command: Command = {
    id: partial.id as Command['id'],
    name: partial.name ?? partial.id,
    scope: 'App',
    showInPalette: false,
    shortcuts: [],
  }
  if (partial.nativeShortcut) command.nativeShortcut = true
  if (partial.fixedKey) command.fixedKey = true
  return command
}

describe('classifyConflict', () => {
  it('returns null for an empty conflict set', () => {
    expect(classifyConflict([])).toBeNull()
  })

  it('classifies a purely non-native conflict as normal (first command)', () => {
    const result = classifyConflict([cmd({ id: 'file.copy' }), cmd({ id: 'file.move' })])
    expect(result).toMatchObject({ kind: 'normal', command: { id: 'file.copy' } })
  })

  it('classifies a native conflict as native', () => {
    const result = classifyConflict([cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true })])
    expect(result).toMatchObject({ kind: 'native', command: { id: 'app.hide' } })
  })

  it('lets the native command win a mixed set (native + normal)', () => {
    // The combo is unusable regardless of the normal command, so native wins
    // even when a normal command appears first in the list.
    const result = classifyConflict([
      cmd({ id: 'file.copy' }),
      cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true }),
    ])
    expect(result).toMatchObject({ kind: 'native', command: { id: 'app.hide' } })
  })
})

describe('reservedByMacOsMessage', () => {
  it('builds the honest reserved-by-macOS copy', () => {
    const msg = reservedByMacOsMessage('⌘H', cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true }))
    expect(msg).toBe("⌘H is reserved by macOS (Hide Cmdr) and won't reach Cmdr. Pick a different combo.")
  })
})

describe('classifyConflict (fixed-key)', () => {
  it('classifies a fixed-key conflict as fixed', () => {
    const result = classifyConflict([cmd({ id: 'nav.up', name: 'Select previous file', fixedKey: true })])
    expect(result).toMatchObject({ kind: 'fixed', command: { id: 'nav.up' } })
  })

  it('lets the fixed command win a mixed set (fixed + normal)', () => {
    // The fixed binding can't be removed and keeps firing, so the combo is
    // non-resolvable even though a normal command appears first.
    const result = classifyConflict([
      cmd({ id: 'file.copy' }),
      cmd({ id: 'nav.up', name: 'Select previous file', fixedKey: true }),
    ])
    expect(result).toMatchObject({ kind: 'fixed', command: { id: 'nav.up' } })
  })

  it('lets a native conflict outrank a fixed one (both in the set)', () => {
    const result = classifyConflict([
      cmd({ id: 'nav.up', fixedKey: true }),
      cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true }),
    ])
    expect(result?.kind).toBe('native')
  })
})

describe('fixedKeyMessage', () => {
  it('names the combo and the owning command', () => {
    const message = fixedKeyMessage('↑', cmd({ id: 'nav.up', name: 'Select previous file', fixedKey: true }))
    expect(message).toBe(
      "↑ is a fixed key in Cmdr (Select previous file) and can't be reassigned. Pick a different combo.",
    )
  })
})

describe('classifySystemShortcut', () => {
  it('flags well-known macOS system combos with their feature label', () => {
    expect(classifySystemShortcut('⌘Space')).toEqual({ kind: 'system', label: 'Spotlight' })
    expect(classifySystemShortcut('⌃↑')).toEqual({ kind: 'system', label: 'Mission Control' })
    expect(classifySystemShortcut('⌘⇧4')).toEqual({ kind: 'system', label: 'screenshots' })
  })

  it('returns null for ordinary combos', () => {
    expect(classifySystemShortcut('⌘⇧P')).toBeNull()
    expect(classifySystemShortcut('F5')).toBeNull()
    expect(classifySystemShortcut('Ctrl+Space')).toBeNull() // non-mac display form never matches
  })
})

describe('systemShortcutMessage', () => {
  it('names the combo and the owning feature and points at System Settings', () => {
    const message = systemShortcutMessage('⌘Space', 'Spotlight')
    expect(message).toContain('⌘Space')
    expect(message).toContain('Spotlight')
    expect(message).toContain('System Settings')
  })
})
