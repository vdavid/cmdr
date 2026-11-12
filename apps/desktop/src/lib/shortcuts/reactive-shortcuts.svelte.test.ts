/**
 * The two readers feature code reads a shortcut through: `commandHasShortcut` (the yes/no
 * a sentence around a chip needs) and `shortcutAsText` (the text-only sinks). Both must
 * follow a rebind and an unbind, since they feed live UI.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import { commandHasShortcut, shortcutAsText } from './reactive-shortcuts.svelte'
import { removeShortcut, resetShortcut, setShortcut } from './shortcuts-store'
import { toDisplayShortcut } from './key-capture'

describe('feature-facing shortcut readers', () => {
  afterEach(() => {
    resetShortcut('nav.back')
  })

  it('reads the first binding in display form', () => {
    setShortcut('nav.back', 0, '⌘Backspace')
    expect(shortcutAsText('nav.back', 'toast')).toBe(toDisplayShortcut('⌘Backspace'))
    expect(commandHasShortcut('nav.back')).toBe(true)
  })

  it('says nothing for an unbound command', () => {
    while (commandHasShortcut('nav.back')) removeShortcut('nav.back', 0)
    expect(shortcutAsText('nav.back', 'announcement')).toBeUndefined()
    expect(commandHasShortcut('nav.back')).toBe(false)
  })
})
