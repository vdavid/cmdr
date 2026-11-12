import { describe, it, expect } from 'vitest'
import { buildShortcutSummary } from './download-toast-shortcuts'

describe('buildShortcutSummary', () => {
  it('returns both when both shortcuts are set', () => {
    expect(buildShortcutSummary(true, '⌃⌥⌘J')).toEqual({ inApp: true, global: '⌃⌥⌘J' })
  })

  it('returns only the in-app one when the global hotkey is off or unbound', () => {
    expect(buildShortcutSummary(true, '')).toEqual({ inApp: true, global: null })
  })

  it('returns only the global key when the in-app shortcut is unbound', () => {
    expect(buildShortcutSummary(false, '⌃⌥⌘J')).toEqual({ inApp: false, global: '⌃⌥⌘J' })
  })
})
