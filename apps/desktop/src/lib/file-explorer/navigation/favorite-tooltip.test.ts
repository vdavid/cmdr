import { describe, it, expect } from 'vitest'
import { buildFavoriteTooltip } from './favorite-tooltip'

describe('buildFavoriteTooltip', () => {
  it('leads with the path so a renamed favorite still reveals where it points', () => {
    const tip = buildFavoriteTooltip('/Users/dave/Documents', true)
    expect(tip.startsWith('/Users/dave/Documents\n')).toBe(true)
  })

  it('uses the Option symbol and arrow glyphs on macOS (no "Alt")', () => {
    const tip = buildFavoriteTooltip('/Users/dave/Documents', true)
    expect(tip).toContain('⌥↑ / ⌥↓')
    expect(tip).not.toContain('Alt')
  })

  it('spells out "Alt" on non-macOS platforms', () => {
    const tip = buildFavoriteTooltip('/home/dave/docs', false)
    expect(tip).toContain('Alt+↑ / Alt+↓')
    expect(tip).not.toContain('⌥')
  })

  it('keeps the right-click hint on both platforms', () => {
    for (const isMac of [true, false]) {
      expect(buildFavoriteTooltip('/p', isMac)).toContain('Right-click to rename or remove.')
    }
  })

  it('puts the path on its own first line (pre-line break)', () => {
    const [first] = buildFavoriteTooltip('/Applications', true).split('\n')
    expect(first).toBe('/Applications')
  })
})
