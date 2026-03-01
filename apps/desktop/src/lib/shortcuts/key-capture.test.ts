import { describe, it, expect, vi, afterEach } from 'vitest'
import { toPlatformShortcut } from './key-capture'

// Mock navigator to control isMacOS() behavior
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')

function setMacOS(isMac: boolean) {
    navigatorSpy.mockReturnValue({
        userAgent: isMac ? 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' : 'Mozilla/5.0 (X11; Linux x86_64)',
    } as Navigator)
}

describe('toPlatformShortcut', () => {
    afterEach(() => {
        vi.restoreAllMocks()
    })

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
})
