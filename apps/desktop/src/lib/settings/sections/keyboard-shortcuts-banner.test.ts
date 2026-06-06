import { describe, it, expect } from 'vitest'
import type { Command } from '$lib/commands/types'
import { classifyConflict, reservedByMacOsMessage } from './keyboard-shortcuts-banner'

function cmd(partial: { id: string; name?: string; nativeShortcut?: true }): Command {
  const command: Command = {
    id: partial.id as Command['id'],
    name: partial.name ?? partial.id,
    scope: 'App',
    showInPalette: false,
    shortcuts: [],
  }
  if (partial.nativeShortcut) command.nativeShortcut = true
  return command
}

describe('classifyConflict', () => {
  it('returns null for an empty conflict set', () => {
    expect(classifyConflict([])).toBeNull()
  })

  it('classifies a purely non-native conflict as normal (first command)', () => {
    const result = classifyConflict([cmd({ id: 'file.copy' }), cmd({ id: 'file.move' })])
    expect(result?.kind).toBe('normal')
    expect(result?.command.id).toBe('file.copy')
  })

  it('classifies a native conflict as native', () => {
    const result = classifyConflict([cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true })])
    expect(result?.kind).toBe('native')
    expect(result?.command.id).toBe('app.hide')
  })

  it('lets the native command win a mixed set (native + normal)', () => {
    // The combo is unusable regardless of the normal command, so native wins
    // even when a normal command appears first in the list.
    const result = classifyConflict([
      cmd({ id: 'file.copy' }),
      cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true }),
    ])
    expect(result?.kind).toBe('native')
    expect(result?.command.id).toBe('app.hide')
  })
})

describe('reservedByMacOsMessage', () => {
  it('builds the honest reserved-by-macOS copy', () => {
    const msg = reservedByMacOsMessage('⌘H', cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true }))
    expect(msg).toBe("⌘H is reserved by macOS (Hide Cmdr) and won't reach Cmdr. Pick a different combo.")
  })
})
