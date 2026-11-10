import { describe, expect, it } from 'vitest'
import { COLOR_CYAN, COLOR_GOLD, COLOR_GREEN, COLOR_HOMEBREW, COLOR_PURPLE, COLOR_UNKNOWN } from './colors.js'
import { countryColor, countryColors, COLOR_OTHER_COUNTRIES, tint } from './country-colors.js'
import { topCountryCount } from './server/sources/funnel.js'

describe('countryColor', () => {
  it('gives each mapped country its fixed color, whatever the case', () => {
    expect(countryColor('US')).toBe(countryColors.US)
    expect(countryColor('de')).toBe(countryColors.DE)
  })

  it('returns the "Other" grey for an unmapped code, "(unknown)", and "Other"', () => {
    expect(countryColor('ZZ')).toBe(COLOR_OTHER_COUNTRIES)
    expect(countryColor('(unknown)')).toBe(COLOR_OTHER_COUNTRIES)
    expect(countryColor('other')).toBe(COLOR_OTHER_COUNTRIES)
  })

  it('colors at least as many countries as the charts slice, each distinctly, none reused from another dashboard property', () => {
    const colors = Object.values(countryColors)
    expect(colors.length).toBeGreaterThanOrEqual(topCountryCount)
    expect(new Set(colors).size).toBe(colors.length)
    for (const taken of [COLOR_GOLD, COLOR_PURPLE, COLOR_GREEN, COLOR_CYAN, COLOR_HOMEBREW, COLOR_UNKNOWN]) {
      expect(colors).not.toContain(taken)
    }
  })

  it('"Other" reads as no identity: the dashboard grey', () => {
    expect(COLOR_OTHER_COUNTRIES).toBe(COLOR_UNKNOWN)
  })
})

describe('tint', () => {
  it('mixes toward white by the given share', () => {
    expect(tint('#000000', 0.5)).toBe('#808080')
    expect(tint('#ff0000', 0)).toBe('#ff0000')
    expect(tint('#204060', 1)).toBe('#ffffff')
  })
})
