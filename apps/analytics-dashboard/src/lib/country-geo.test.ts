import { describe, it, expect } from 'vitest'
import atlas from 'visionscarto-world-atlas/world/110m.json'
import { antarcticaMapId, countryForMapId, mapIdsForCountry } from './country-geo.js'

const features = (atlas as { objects: { countries: { geometries: { id: string; properties: { name: string } }[] } } })
  .objects.countries.geometries

describe('mapIdsForCountry', () => {
  it('maps the alpha-2 codes Umami and Cloudflare send to the atlas ids', () => {
    expect(mapIdsForCountry('SE')).toEqual(['752'])
    expect(mapIdsForCountry('HU')).toEqual(['348'])
    expect(mapIdsForCountry('US')).toEqual(['840'])
    expect(mapIdsForCountry('GB')).toEqual(['826'])
    // Zero-padded, as the atlas stores them.
    expect(mapIdsForCountry('AF')).toEqual(['004'])
    // Kosovo has no ISO numeric code; Cloudflare and MaxMind send XK.
    expect(mapIdsForCountry('XK')).toEqual(['-2'])
  })

  it('ignores case', () => {
    expect(mapIdsForCountry('se')).toEqual(['752'])
  })

  it('returns nothing for a place the 110m map has no shape for, or a non-country code', () => {
    expect(mapIdsForCountry('SG')).toEqual([])
    expect(mapIdsForCountry('HK')).toEqual([])
    expect(mapIdsForCountry('(unknown)')).toEqual([])
    expect(mapIdsForCountry('T1')).toEqual([])
  })
})

describe('against the atlas file', () => {
  it('points every mapped id at a real feature with that name', () => {
    const byId = new Map(features.map((f) => [f.id, f.properties.name]))
    expect(byId.get(mapIdsForCountry('SE')[0])).toBe('Sweden')
    expect(byId.get(mapIdsForCountry('CZ')[0])).toBe('Czechia')
    expect(byId.get(mapIdsForCountry('XK')[0])).toBe('Kosovo')
    for (const f of features) {
      if (f.id === antarcticaMapId) continue
      expect(countryForMapId(f.id), `${f.properties.name} (${f.id})`).not.toBeNull()
    }
  })

  it('round-trips every feature id back to its country', () => {
    for (const f of features) {
      const iso2 = countryForMapId(f.id)
      if (iso2) expect(mapIdsForCountry(iso2)).toContain(f.id)
    }
  })

  it('fills the shapes geolocation folds into a neighbor with that neighbor', () => {
    expect(countryForMapId('-1')).toBe('CY')
    expect(countryForMapId('-3')).toBe('SO')
    expect(countryForMapId(antarcticaMapId)).toBeNull()
  })
})
