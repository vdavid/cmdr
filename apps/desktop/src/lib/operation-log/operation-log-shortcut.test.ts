/**
 * Regression guard for the default ⌘⌥L shortcut being genuinely JS-dispatchable.
 *
 * The collision test in `command-registry.test.ts` pins the registry STRING; this
 * pins the whole chain: on macOS, a real Option+Command+L keydown formats to the
 * exact string the dispatch map holds, so `lookupCommand` resolves it to
 * `log.operationLog`. The modifier order is load-bearing — `formatKeyCombo` emits
 * Command-then-Option (⌘⌥), so a well-meaning "fix" to the Apple display order
 * (⌥⌘L) would silently stop the keyboard dispatch (it would fire only via the
 * native menu accelerator). This test fails loudly if that happens.
 */

import { describe, it, expect, vi, beforeAll, afterAll } from 'vitest'
import { getEffectiveShortcuts } from '$lib/shortcuts/shortcuts-store'
import { formatKeyCombo } from '$lib/shortcuts/key-capture'
import { initShortcutDispatch, lookupCommand, destroyShortcutDispatch } from '$lib/shortcuts/shortcut-dispatch'

// These modules read the platform (`isMacOS()`, userAgent-based) only at call time, never
// at module load, so a static import is safe: the mock below is active before any of them run.
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeAll(() => {
  // Force the macOS branch of isMacOS() for both the shortcut-map build (toPlatformShortcut)
  // and formatKeyCombo, so they agree the way they do on a real Mac.
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterAll(() => navigatorSpy.mockReset())

describe('log.operationLog default shortcut (⌘⌥L)', () => {
  it('a real Option+Command+L keydown resolves to log.operationLog', () => {
    // The registry default, converted to the active platform (macOS: unchanged).
    expect(getEffectiveShortcuts('log.operationLog')).toEqual(['⌘⌥L'])

    // The string a genuine keypress produces must match that exactly.
    const combo = formatKeyCombo({
      metaKey: true,
      altKey: true,
      ctrlKey: false,
      shiftKey: false,
      key: 'l',
      code: 'KeyL',
    } as KeyboardEvent)
    expect(combo).toBe('⌘⌥L')

    initShortcutDispatch()
    try {
      expect(lookupCommand(combo)).toBe('log.operationLog')
    } finally {
      destroyShortcutDispatch()
    }
  })
})
