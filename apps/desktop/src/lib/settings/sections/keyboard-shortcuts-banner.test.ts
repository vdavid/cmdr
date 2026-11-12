import { beforeEach, describe, it, expect, onTestFinished } from 'vitest'
import { withCapabilities, withPlatform } from '#lib/platform/test-platform.js'
import type { Command } from '#lib/commands/types.js'
import { BLOCKED_BY_DIALOGS } from '#lib/commands/while-dialog-open.js'
import { sameOnAll } from '#lib/commands/platform-shortcuts.js'
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
    // The banner reads `name`; `displayName` is the palette's, and resolves to
    // `name` for every command that declares no override.
    displayName: partial.name ?? partial.id,
    scope: 'App',
    showInPalette: false,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  }
  if (partial.nativeShortcut) command.nativeShortcut = true
  if (partial.fixedKey) command.fixedKey = true
  return command
}

describe('classifyConflict', () => {
  // A native command owns its key only where the OS has an app menu (macOS).
  beforeEach(() => withCapabilities('appMenu'))

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

  it('treats Quit as an ordinary command where no app menu owns it, so nothing claims a macOS reservation', () => {
    onTestFinished(withCapabilities())
    const result = classifyConflict([cmd({ id: 'app.quit', name: 'Quit Cmdr', nativeShortcut: true })])
    expect(result).toMatchObject({ kind: 'normal', command: { id: 'app.quit' } })
  })
})

describe('reservedByMacOsMessage', () => {
  it('builds the honest reserved-by-macOS copy', () => {
    const msg = reservedByMacOsMessage('⌘H', cmd({ id: 'app.hide', name: 'Hide Cmdr', nativeShortcut: true }))
    expect(msg).toBe('⌘H is reserved by macOS (Hide Cmdr) and won’t reach Cmdr. Pick a different combo.')
  })
})

describe('classifyConflict (fixed-key)', () => {
  beforeEach(() => withCapabilities('appMenu'))

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
      '↑ is a fixed key in Cmdr (Select previous file) and can’t be reassigned. Pick a different combo.',
    )
  })
})

describe('classifySystemShortcut', () => {
  it('flags well-known macOS system combos with the feature that takes them', () => {
    onTestFinished(withPlatform('macos'))
    expect(classifySystemShortcut('⌘Space')).toEqual({
      kind: 'system',
      reservation: { os: 'macos', featureKey: 'shortcuts.system.spotlight' },
    })
    expect(classifySystemShortcut('⌃↑')?.reservation).toEqual({
      os: 'macos',
      featureKey: 'shortcuts.system.missionControl',
    })
  })

  it('returns null for ordinary combos and for another OS’s reservations', () => {
    onTestFinished(withPlatform('macos'))
    expect(classifySystemShortcut('⌘⇧P')).toBeNull()
    expect(classifySystemShortcut('F5')).toBeNull()
    expect(classifySystemShortcut('Ctrl+Space')).toBeNull() // Windows' IME key, not a Mac one
  })

  it('flags what Windows itself takes, on Windows', () => {
    onTestFinished(withPlatform('windows'))
    expect(classifySystemShortcut('Alt+Tab')?.reservation).toEqual({
      os: 'windows',
      featureKey: 'shortcuts.system.switchingApps',
    })
    expect(classifySystemShortcut('⌘Space')).toBeNull()
    expect(classifySystemShortcut('Ctrl+Shift+K')).toBeNull()
  })

  it('flags what a Linux desktop takes, naming every one that does', () => {
    onTestFinished(withPlatform('linux'))
    expect(classifySystemShortcut('Ctrl+Alt+→')?.reservation).toEqual({
      os: 'linux',
      desktops: ['GNOME', 'Xfce', 'Cinnamon', 'Ubuntu'],
    })
    expect(classifySystemShortcut('Alt+F4')?.reservation).toEqual({
      os: 'linux',
      desktops: ['GNOME', 'KDE Plasma', 'Xfce', 'Cinnamon', 'Ubuntu'],
    })
  })
})

describe('systemShortcutMessage', () => {
  it('names the combo and the owning feature and points at System Settings on macOS', () => {
    onTestFinished(withPlatform('macos'))
    const message = systemShortcutMessage('⌘Space', { os: 'macos', featureKey: 'shortcuts.system.spotlight' })
    expect(message).toBe(
      '⌘Space is usually taken by macOS (Spotlight), so it may never reach Cmdr. You can free it up in System Settings > Keyboard.',
    )
  })

  it('names Windows and the feature on Windows', () => {
    onTestFinished(withPlatform('windows'))
    expect(systemShortcutMessage('Ctrl+Shift+Esc', { os: 'windows', featureKey: 'shortcuts.system.taskManager' })).toBe(
      'Ctrl+Shift+Esc is usually taken by Windows (Task Manager), so it may never reach Cmdr.',
    )
  })

  it('names every desktop that takes it on Linux, as a list in the UI language', () => {
    onTestFinished(withPlatform('linux'))
    expect(systemShortcutMessage('Alt+F1', { os: 'linux', desktops: ['KDE Plasma', 'Xfce', 'Cinnamon'] })).toBe(
      'On KDE Plasma, Xfce, and Cinnamon, the desktop usually takes Alt+F1 for itself, so it may never reach Cmdr. You can free it up in your desktop’s keyboard settings.',
    )
    expect(systemShortcutMessage('Ctrl+Alt+D', { os: 'linux', desktops: ['Ubuntu'] })).toBe(
      'On Ubuntu, the desktop usually takes Ctrl+Alt+D for itself, so it may never reach Cmdr. You can free it up in your desktop’s keyboard settings.',
    )
  })
})
