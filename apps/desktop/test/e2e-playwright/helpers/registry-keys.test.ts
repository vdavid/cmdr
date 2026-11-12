import { describe, it, expect } from 'vitest'
import { toPlaywrightCombo } from './registry-keys'

describe('toPlaywrightCombo', () => {
  it.each([
    ['⌘⌥L', 'macos', 'Meta+Alt+l'],
    ['⌃D', 'macos', 'Control+d'],
    ['⇧F8', 'macos', 'Shift+F8'],
    ['⌘↓', 'macos', 'Meta+ArrowDown'],
    ['Ctrl+Shift+Q', 'linux', 'Control+Shift+q'],
    ['Shift+F5', 'linux', 'Shift+F5'],
    ['Alt+←', 'windows', 'Alt+ArrowLeft'],
    ['Ctrl+PageDown', 'linux', 'Control+PageDown'],
    ['Ctrl++', 'linux', 'Control++'],
    ['Enter', 'linux', 'Enter'],
  ] as const)('%s on %s is %s', (combo, os, expected) => {
    expect(toPlaywrightCombo(combo, os)).toBe(expected)
  })
})
