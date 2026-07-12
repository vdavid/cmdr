/**
 * Regression guard for the default ⌘⌥A shortcut being genuinely JS-dispatchable.
 *
 * Mirrors `operation-log-shortcut.test.ts`: on macOS a real Option+Command+A keydown must
 * format to the exact string the dispatch map holds, so `lookupCommand` resolves it to
 * `askCmdr.toggle`. The modifier order is load-bearing — `formatKeyCombo` emits
 * Command-then-Option (⌘⌥), so switching the registry default to the Apple display order
 * (⌥⌘A) would silently break the keyboard dispatch (native-menu-only). This fails loudly
 * if that happens.
 */

import { describe, it, expect, vi, beforeAll, afterAll } from 'vitest'
import { getEffectiveShortcuts } from '$lib/shortcuts/shortcuts-store'
import { formatKeyCombo } from '$lib/shortcuts/key-capture'
import { initShortcutDispatch, lookupCommand, destroyShortcutDispatch } from '$lib/shortcuts/shortcut-dispatch'

const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeAll(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterAll(() => navigatorSpy.mockReset())

describe('askCmdr.toggle default shortcut (⌘⌥A)', () => {
  it('a real Option+Command+A keydown resolves to askCmdr.toggle', () => {
    expect(getEffectiveShortcuts('askCmdr.toggle')).toEqual(['⌘⌥A'])

    const combo = formatKeyCombo({
      metaKey: true,
      altKey: true,
      ctrlKey: false,
      shiftKey: false,
      key: 'a',
      code: 'KeyA',
    } as KeyboardEvent)
    expect(combo).toBe('⌘⌥A')

    initShortcutDispatch()
    try {
      expect(lookupCommand(combo)).toBe('askCmdr.toggle')
    } finally {
      destroyShortcutDispatch()
    }
  })
})
