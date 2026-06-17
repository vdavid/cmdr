import { describe, it, expect } from 'vitest'
import { couplingsFromReport, coupleCatalog, serializeCatalog, fileForKey } from './couple-screenshots.js'

// A small fixture catalog mirroring the real `messages/en/<area>.json` shape: a
// message value, then its ARB-style `@key` twin (description + placeholders).
// The N1 safety guarantee under test: coupling touches ONLY `@key.screenshot`,
// never a message VALUE or any other twin field.
function fixtureCatalog() {
  return {
    'common.ok': 'OK',
    '@common.ok': {
      description: 'Confirm button in dialogs.',
    },
    'common.cancel': 'Cancel',
    '@common.cancel': {
      description: 'Dismiss button in dialogs.',
      placeholders: {},
    },
    // A value with ICU braces and doubled apostrophes — exactly the kind of
    // string a careless round-trip could corrupt.
    'common.greeting': "It''s {count, plural, one {# file} other {# files}}",
    '@common.greeting': {
      description: 'Status text.',
      placeholders: { count: 'number of files' },
    },
  }
}

/**
 * All message VALUES (non-`@` keys) of a catalog, as a plain object.
 * @param {Record<string, unknown>} json
 * @returns {Record<string, unknown>}
 */
function valuesOf(json) {
  /** @type {Record<string, unknown>} */
  const out = {}
  for (const [k, v] of Object.entries(json)) {
    if (!k.startsWith('@')) out[k] = v
  }
  return out
}

describe('coupleCatalog (N1 value-safety)', () => {
  it('changes only @key.screenshot fields; every message value is byte-identical', () => {
    const before = fixtureCatalog()
    const beforeValuesJson = JSON.stringify(valuesOf(before))

    const keyToScreenshot = new Map([
      ['common.ok', 'dialog.png'],
      ['common.greeting', 'status.png'],
    ])
    const { json: after, changed, couplingCount } = coupleCatalog(before, keyToScreenshot)

    expect(changed).toBe(true)
    expect(couplingCount).toBe(2)

    // Every message value survives byte-for-byte (the heart of the guarantee).
    expect(JSON.stringify(valuesOf(after))).toBe(beforeValuesJson)

    // The only difference vs. the original is the added `screenshot` fields.
    expect(after['@common.ok']).toEqual({ description: 'Confirm button in dialogs.', screenshot: 'dialog.png' })
    expect(after['@common.greeting']).toEqual({
      description: 'Status text.',
      placeholders: { count: 'number of files' },
      screenshot: 'status.png',
    })
    // An untouched key keeps its twin exactly.
    expect(after['@common.cancel']).toEqual({ description: 'Dismiss button in dialogs.', placeholders: {} })
  })

  it('is idempotent: a second run with the same couplings is a no-op', () => {
    const keyToScreenshot = new Map([['common.ok', 'dialog.png']])
    const first = coupleCatalog(fixtureCatalog(), keyToScreenshot)
    expect(first.changed).toBe(true)
    const second = coupleCatalog(first.json, keyToScreenshot)
    expect(second.changed).toBe(false)
    expect(second.couplingCount).toBe(0)
    expect(serializeCatalog(second.json)).toBe(serializeCatalog(first.json))
  })

  it('keeps each @key twin directly after its message key after coupling', () => {
    const { json } = coupleCatalog(fixtureCatalog(), new Map([['common.cancel', 'dialog.png']]))
    const keys = Object.keys(json)
    for (const k of keys) {
      if (k.startsWith('@')) continue
      const twinIndex = keys.indexOf(`@${k}`)
      if (twinIndex === -1) continue
      expect(twinIndex).toBe(keys.indexOf(k) + 1)
    }
  })

  it('reports keys coupled without a description twin', () => {
    const catalog = { 'common.ok': 'OK' } // no twin at all
    const { coupledWithoutDescription, couplingCount } = coupleCatalog(catalog, new Map([['common.ok', 'x.png']]))
    expect(couplingCount).toBe(1)
    expect(coupledWithoutDescription).toEqual(['common.ok → x.png'])
  })

  it('reports keys absent from the catalog rather than minting them', () => {
    const catalog = { 'common.ok': 'OK' }
    const { json, missingKeys, changed } = coupleCatalog(catalog, new Map([['common.ghost', 'x.png']]))
    expect(missingKeys).toEqual(['common.ghost'])
    expect(changed).toBe(false)
    expect('@common.ghost' in json).toBe(false)
  })
})

describe('couplingsFromReport', () => {
  it('flattens surface→keys with first-surface-wins ordering', () => {
    const report = {
      'narrow-dialog': { screenshot: 'dialog.png', keys: ['common.ok'] },
      'broad-window': { screenshot: 'window.png', keys: ['common.ok', 'common.cancel'] },
    }
    const map = couplingsFromReport(report)
    expect(map.get('common.ok')).toBe('dialog.png') // first surface wins
    expect(map.get('common.cancel')).toBe('window.png')
  })
})

describe('fileForKey', () => {
  it('maps a key to its area catalog file', () => {
    expect(fileForKey('settings.fsWatch.title')).toBe('settings.json')
    expect(fileForKey('common.ok')).toBe('common.json')
  })
})
