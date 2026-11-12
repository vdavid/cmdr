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

import { describe, it, expect, beforeAll } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { getEffectiveShortcuts } from '#lib/shortcuts/shortcuts-store.js'
import { formatKeyCombo } from '#lib/shortcuts/key-capture.js'
import { initShortcutDispatch, lookupCommand, destroyShortcutDispatch } from '#lib/shortcuts/shortcut-dispatch.js'

// These modules read the platform (`isMacOS()`) only at call time, never
// at module load, so a static import is safe: the platform below is set before any of them run.
// macOS for both the shortcut-map build (the registry's macOS arm) and formatKeyCombo, so they agree
// the way they do on a real Mac.
beforeAll(() => withPlatform('macos'))

describe('log.operationLog default shortcut (⌘⌥L)', () => {
  it('a real Option+Command+L keydown resolves to log.operationLog', () => {
    // The registry's default for the active platform.
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
