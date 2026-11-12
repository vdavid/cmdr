import { describe, it, expect } from 'vitest'
import { LINUX_DESKTOPS, reservedCombos, systemReservation } from './system-reserved-keys'

describe('systemReservation', () => {
  it('names a macOS feature by its message key, on macOS only', () => {
    expect(systemReservation('⌘Space', 'macos')).toEqual({ os: 'macos', featureKey: 'shortcuts.system.spotlight' })
    expect(systemReservation('⌘Space', 'linux')).toBeNull()
  })

  it('names the Windows feature that takes a combo, on Windows only', () => {
    expect(systemReservation('Alt+Tab', 'windows')).toEqual({
      os: 'windows',
      featureKey: 'shortcuts.system.switchingApps',
    })
    expect(systemReservation('Ctrl+Shift+Escape', 'windows')).toEqual({
      os: 'windows',
      featureKey: 'shortcuts.system.taskManager',
    })
    expect(systemReservation('Ctrl+Shift+Escape', 'macos')).toBeNull()
  })

  it('names EVERY Linux desktop that takes a combo, in one fixed order, never a sample', () => {
    expect(systemReservation('Alt+F2', 'linux')).toEqual({ os: 'linux', desktops: [...LINUX_DESKTOPS] })
    expect(systemReservation('Alt+F1', 'linux')).toEqual({ os: 'linux', desktops: ['KDE Plasma', 'Xfce', 'Cinnamon'] })
    expect(systemReservation('Ctrl+F3', 'linux')).toEqual({ os: 'linux', desktops: ['KDE Plasma', 'Xfce'] })
    // Ubuntu is GNOME plus its own keys, so it takes everything GNOME does.
    expect(systemReservation('Alt+F5', 'linux')).toEqual({ os: 'linux', desktops: ['GNOME', 'Cinnamon', 'Ubuntu'] })
    expect(systemReservation('Ctrl+Alt+D', 'linux')).toEqual({ os: 'linux', desktops: ['Ubuntu'] })
  })

  it('leaves a key no listed desktop takes alone', () => {
    expect(systemReservation('Alt+Shift+P', 'linux')).toBeNull()
    expect(systemReservation('Ctrl+Shift+K', 'linux')).toBeNull()
    expect(systemReservation('F11', 'linux')).toBeNull()
  })

  it('lists each platform’s reserved combos for the vocabulary and invariants checks', () => {
    expect(reservedCombos('windows')).toContain('Ctrl+Alt+Delete')
    expect(reservedCombos('linux')).toContain('Ctrl+Alt+→')
    expect(reservedCombos('macos')).toContain('⌘⌥Escape')
  })
})
