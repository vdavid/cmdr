import { describe, it, expect } from 'vitest'
import { globalRevealDescription } from './global-shortcut-description'

describe('globalRevealDescription', () => {
  it('references the given binding verbatim', () => {
    expect(globalRevealDescription('\u{2303}\u{2325}\u{2318}J')).toBe(
      'Press \u{2303}\u{2325}\u{2318}J from any app to jump to your most recent download.',
    )
  })

  it('tracks a rebound combo', () => {
    expect(globalRevealDescription('\u{2318}\u{21E7}K')).toContain('\u{2318}\u{21E7}K')
  })

  it('falls back to a generic phrasing when the binding is empty', () => {
    expect(globalRevealDescription('')).toBe('Jump to your most recent download from any app.')
  })
})
