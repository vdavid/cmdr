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

import { describe, it, expect, beforeAll } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { getEffectiveShortcuts } from '#lib/shortcuts/shortcuts-store.js'
import { formatKeyCombo } from '#lib/shortcuts/key-capture.js'
import { initShortcutDispatch, lookupCommand, destroyShortcutDispatch } from '#lib/shortcuts/shortcut-dispatch.js'
beforeAll(() => withPlatform('macos'))

describe('askCmdr.newChat default shortcut (⌘N)', () => {
  it('a real Command+N keydown resolves to askCmdr.newChat', () => {
    expect(getEffectiveShortcuts('askCmdr.newChat')).toEqual(['⌘N'])
    const combo = formatKeyCombo({
      metaKey: true,
      altKey: false,
      ctrlKey: false,
      shiftKey: false,
      key: 'n',
      code: 'KeyN',
    } as KeyboardEvent)
    expect(combo).toBe('⌘N')

    initShortcutDispatch()
    try {
      expect(lookupCommand(combo)).toBe('askCmdr.newChat')
    } finally {
      destroyShortcutDispatch()
    }
  })
})

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
