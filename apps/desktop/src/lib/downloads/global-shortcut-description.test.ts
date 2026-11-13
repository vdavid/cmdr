import { describe, it, expect, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { globalGoToLatestDescription } from './global-shortcut-description'

describe('globalGoToLatestDescription', () => {
  it('references the given binding verbatim', () => {
    expect(globalGoToLatestDescription('\u{2303}\u{2325}\u{2318}J')).toBe(
      'Press \u{2303}\u{2325}\u{2318}J from any app to jump to your most recent download.',
    )
  })

  it('tracks a rebound combo', () => {
    expect(globalGoToLatestDescription('\u{2318}\u{21E7}K')).toContain('\u{21E7}\u{2318}K')
  })

  it('shows the binding the way the shortcut row does, so Windows reads Win', () => {
    onTestFinished(withPlatform('windows'))
    expect(globalGoToLatestDescription('Ctrl+Alt+Super+J')).toBe(
      'Press Ctrl+Alt+Win+J from any app to jump to your most recent download.',
    )
  })

  it('falls back to a generic phrasing when the binding is empty', () => {
    expect(globalGoToLatestDescription('')).toBe('Jump to your most recent download from any app.')
  })
})
