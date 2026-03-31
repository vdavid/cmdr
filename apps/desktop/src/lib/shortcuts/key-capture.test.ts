import { describe, it, expect, vi, afterEach } from 'vitest'
import { formatKeyCombo, toPlatformShortcut } from './key-capture'

// Mock navigator to control isMacOS() behavior
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')

function setMacOS(isMac: boolean) {
  navigatorSpy.mockReturnValue({
    userAgent: isMac ? 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' : 'Mozilla/5.0 (X11; Linux x86_64)',
  } as Navigator)
}

afterEach(() => {
  navigatorSpy.mockReset()
})

describe('toPlatformShortcut', () => {
  it('returns shortcut as-is on macOS', () => {
    setMacOS(true)
    expect(toPlatformShortcut('⌘Q')).toBe('⌘Q')
    expect(toPlatformShortcut('⌘⇧P')).toBe('⌘⇧P')
  })

  it('converts basic modifiers on Linux', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⌘Q')).toBe('Ctrl+Q')
    expect(toPlatformShortcut('⌘⇧P')).toBe('Ctrl+Shift+P')
    expect(toPlatformShortcut('⌥⌘O')).toBe('Alt+Ctrl+O')
  })

  it('handles ⌃⌘ collision by mapping ⌃ to Shift on Linux', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⌃⌘C')).toBe('Shift+Ctrl+C')
  })

  it('passes through platform-neutral shortcuts unchanged', () => {
    setMacOS(false)
    expect(toPlatformShortcut('Tab')).toBe('Tab')
    expect(toPlatformShortcut('Enter')).toBe('Enter')
    expect(toPlatformShortcut('F4')).toBe('F4')
    expect(toPlatformShortcut('Space')).toBe('Space')
    expect(toPlatformShortcut('Backspace')).toBe('Backspace')
    expect(toPlatformShortcut('↑')).toBe('↑')
    expect(toPlatformShortcut('PageUp')).toBe('PageUp')
  })

  it('converts Cmd+arrow shortcuts on Linux', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⌘↑')).toBe('Ctrl+↑')
    expect(toPlatformShortcut('⌘[')).toBe('Ctrl+[')
    expect(toPlatformShortcut('⌘]')).toBe('Ctrl+]')
  })

  it('converts view mode shortcuts on Linux', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⌘1')).toBe('Ctrl+1')
    expect(toPlatformShortcut('⌘2')).toBe('Ctrl+2')
    expect(toPlatformShortcut('⌘,')).toBe('Ctrl+,')
  })

  it('converts Ctrl-only shortcut on Linux', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⌃Tab')).toBe('Ctrl+Tab')
  })

  it('converts Shift+F-key shortcuts on Linux', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⇧F6')).toBe('Shift+F6')
    expect(toPlatformShortcut('⇧F8')).toBe('Shift+F8')
  })

  it('converts complex modifier combos on Linux', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⌘⇧.')).toBe('Ctrl+Shift+.')
    expect(toPlatformShortcut('⌘⇧A')).toBe('Ctrl+Shift+A')
    expect(toPlatformShortcut('⌃⇧Tab')).toBe('Ctrl+Shift+Tab')
  })

  it('converts Alt+F-key shortcuts for volume choosers', () => {
    setMacOS(false)
    expect(toPlatformShortcut('⌥F1')).toBe('Alt+F1')
    expect(toPlatformShortcut('⌥F2')).toBe('Alt+F2')
  })
})

describe('formatKeyCombo', () => {
  function makeKeyEvent(overrides: Partial<KeyboardEvent>): KeyboardEvent {
    return {
      key: '',
      metaKey: false,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      ...overrides,
    } as KeyboardEvent
  }

  it('uses Super for metaKey on Linux', () => {
    setMacOS(false)
    const result = formatKeyCombo(makeKeyEvent({ metaKey: true, key: 'a' }))
    expect(result).toBe('Super+A')
  })

  it('uses ⌘ for metaKey on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ metaKey: true, key: 'a' }))
    expect(result).toBe('⌘A')
  })

  it('formats Alt+F1 on Linux', () => {
    setMacOS(false)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'F1' }))
    expect(result).toBe('Alt+F1')
  })

  it('formats Alt+F2 on Linux', () => {
    setMacOS(false)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'F2' }))
    expect(result).toBe('Alt+F2')
  })

  it('resolves Dead key via event.code for ⌥+letter on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'Dead', code: 'KeyH' }))
    expect(result).toBe('⌥H')
  })

  it('resolves Dead key via event.code for ⌥+digit on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'Dead', code: 'Digit6' }))
    expect(result).toBe('⌥6')
  })

  it('resolves Dead key via event.code for ⌥+punctuation on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ altKey: true, key: 'Dead', code: 'BracketLeft' }))
    expect(result).toBe('⌥[')
  })

  it('resolves Dead key with multiple modifiers on macOS', () => {
    setMacOS(true)
    const result = formatKeyCombo(makeKeyEvent({ metaKey: true, altKey: true, key: 'Dead', code: 'KeyE' }))
    expect(result).toBe('⌘⌥E')
  })
})
