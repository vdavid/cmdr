import { describe, it, expect } from 'vitest'
import { buildShortcutSummary } from './download-toast-shortcuts'

describe('buildShortcutSummary', () => {
  it('returns both keys when both shortcuts are set', () => {
    expect(buildShortcutSummary('⌘J', '⌃⌥⌘J')).toEqual({ inApp: '⌘J', global: '⌃⌥⌘J' })
  })

  it('returns only the in-app key when the global hotkey is off or unbound', () => {
    expect(buildShortcutSummary('⌘J', '')).toEqual({ inApp: '⌘J', global: null })
  })

  it('returns only the global key when the in-app shortcut is unbound', () => {
    expect(buildShortcutSummary('', '⌃⌥⌘J')).toEqual({ inApp: null, global: '⌃⌥⌘J' })
  })
})
