/**
 * Unit tests for the build-time catalog metadata strip.
 *
 * The interesting property is the MIRROR: what the plugin removes has to be
 * exactly what the runtime's `stripMetadata` would have removed, or the bundle
 * and the app disagree about what a catalog contains. Both sides go through
 * `splitCatalogFile`, and the last test here pins that they agree.
 */

import { describe, expect, it } from 'vitest'
import { isLocaleCatalogId, stripCatalogSource } from './vite-strip-catalog-metadata.ts'

const CATALOG = '/repo/apps/desktop/src/lib/intl/messages/en/menu.json'

/** Strips one catalog source and parses the result, typed: `JSON.parse` alone is `any`. */
function parseStripped(source: string): Record<string, unknown> {
  return JSON.parse(stripCatalogSource(source)) as Record<string, unknown>
}

describe('isLocaleCatalogId', () => {
  it('matches a locale catalog file', () => {
    expect(isLocaleCatalogId(CATALOG)).toBe(true)
    expect(isLocaleCatalogId('/repo/apps/desktop/src/lib/intl/messages/pt-BR/fileExplorer.json')).toBe(true)
  })

  it('rejects the reserved non-locale dirs, which also hold JSON', () => {
    expect(isLocaleCatalogId('/repo/apps/desktop/src/lib/intl/messages/screenshots/capture-report.json')).toBe(false)
  })

  it('rejects JSON outside the catalogs', () => {
    expect(isLocaleCatalogId('/repo/feature-status.json')).toBe(false)
    expect(isLocaleCatalogId('/repo/apps/desktop/src/lib/error-messages/__fixtures__/golden.json')).toBe(false)
    expect(isLocaleCatalogId('/repo/apps/desktop/src/lib/intl/messages/en/menu.js')).toBe(false)
  })

  it('rejects an explicit raw or url import, which asks for the bytes on purpose', () => {
    expect(isLocaleCatalogId(`${CATALOG}?raw`)).toBe(false)
    expect(isLocaleCatalogId(`${CATALOG}?url`)).toBe(false)
  })
})

describe('stripCatalogSource', () => {
  it('drops @key metadata and keeps the messages', () => {
    const out = parseStripped(
      JSON.stringify({
        'menu.app.quit': 'Quit Cmdr',
        '@menu.app.quit': { description: 'VERB. Last item of the macOS app menu.' },
      }),
    )
    expect(out).toEqual({ 'menu.app.quit': 'Quit Cmdr' })
  })

  it('keeps ICU syntax and non-ASCII values byte-for-byte', () => {
    const icu = '{count, plural, one {# file} other {# files}}'
    const out = parseStripped(
      JSON.stringify({ 'a.icu': icu, 'a.zh': '关于 Cmdr', '@a.icu': { sourceHash: 'abc1234' } }),
    )
    expect(out).toEqual({ 'a.icu': icu, 'a.zh': '关于 Cmdr' })
  })

  it('drops a non-string value, exactly as the runtime does', () => {
    const out = parseStripped(JSON.stringify({ 'a.ok': 'yes', 'a.weird': 42, 'a.list': ['x'] }))
    expect(out).toEqual({ 'a.ok': 'yes' })
  })

  it('emits parseable JSON for an empty catalog', () => {
    expect(parseStripped('{}')).toEqual({})
  })

  it('removes every @ entry from a real catalog, and no message', async () => {
    const raw = (await import('../src/lib/intl/messages/en/menu.json', { with: { type: 'json' } })).default as Record<
      string,
      unknown
    >
    const stripped = parseStripped(JSON.stringify(raw))
    const messageKeys = Object.keys(raw).filter((k) => !k.startsWith('@') && typeof raw[k] === 'string')
    expect(Object.keys(stripped)).toEqual(messageKeys)
    expect(Object.keys(stripped).some((k) => k.startsWith('@'))).toBe(false)
    expect(messageKeys.length).toBeGreaterThan(0)
  })
})
