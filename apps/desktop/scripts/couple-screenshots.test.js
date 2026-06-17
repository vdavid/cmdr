import { describe, it, expect } from 'vitest'
import { couplingsFromReport, coupleCatalog, fileForKey } from './couple-screenshots.js'

// An oxfmt-shaped fixture catalog mirroring the real `messages/en/<area>.json`:
// 2-space indent, each `@key` twin right after its message key, BLANK LINES
// between groups, a nested `placeholders` object, ICU braces, doubled
// apostrophes. The N1 safety guarantee under test: coupling edits ONLY
// `@key.screenshot` and leaves every other byte — values, other twin fields,
// indentation, and the blank-line grouping — byte-identical.
const FIXTURE = `{
  "common.ok": "OK",
  "@common.ok": {
    "description": "Confirm button in dialogs."
  },

  "common.cancel": "Cancel",
  "@common.cancel": {
    "description": "Dismiss button in dialogs.",
    "placeholders": {}
  },

  "common.greeting": "It''s {count, plural, one {# file} other {# files}}",
  "@common.greeting": {
    "description": "Status text.",
    "placeholders": {
      "count": "number of files"
    }
  }
}
`

describe('coupleCatalog (N1 value-safety, line-surgical)', () => {
  it('edits ONLY @key.screenshot lines; every other byte is preserved', () => {
    const keyToScreenshot = new Map([
      ['common.ok', 'dialog.png'],
      ['common.greeting', 'status.png'],
    ])
    const { text, changed, couplingCount } = coupleCatalog(FIXTURE, keyToScreenshot)

    expect(changed).toBe(true)
    expect(couplingCount).toBe(2)

    // The output must still parse and carry every message value byte-identical.
    const before = JSON.parse(FIXTURE)
    const after = JSON.parse(text)
    for (const k of Object.keys(before)) {
      if (k.startsWith('@')) continue
      expect(after[k]).toBe(before[k])
    }

    // The ONLY textual difference is the two inserted screenshot fields. Each was
    // appended after the twin's previously-last field, so each insertion is
    // `,\n    "screenshot": "…"` (a comma added to the prior line + the new line).
    // Removing both reproduces the input exactly, blank lines included.
    const reverted = text
      .replace(',\n    "screenshot": "dialog.png"', '')
      .replace(',\n    "screenshot": "status.png"', '')
    expect(reverted).toBe(FIXTURE)

    // The couplings landed on the right twins, descriptions/placeholders intact.
    expect(after['@common.ok']).toEqual({ description: 'Confirm button in dialogs.', screenshot: 'dialog.png' })
    expect(after['@common.greeting']).toEqual({
      description: 'Status text.',
      placeholders: { count: 'number of files' },
      screenshot: 'status.png',
    })
    // An untouched key keeps its twin exactly.
    expect(after['@common.cancel']).toEqual({ description: 'Dismiss button in dialogs.', placeholders: {} })
  })

  it('preserves the blank lines between catalog groups', () => {
    const { text } = coupleCatalog(FIXTURE, new Map([['common.ok', 'dialog.png']]))
    // Same number of blank (empty) lines as the input — the round-trip bug this
    // guards would collapse them to zero.
    /** @param {string} s */
    const blanks = (s) => s.split('\n').filter((/** @type {string} */ l) => l === '').length
    expect(blanks(text)).toBe(blanks(FIXTURE))
  })

  it('replaces an existing screenshot value rather than duplicating it', () => {
    const once = coupleCatalog(FIXTURE, new Map([['common.ok', 'first.png']])).text
    const twice = coupleCatalog(once, new Map([['common.ok', 'second.png']])).text
    const obj = JSON.parse(twice)
    expect(obj['@common.ok']).toEqual({ description: 'Confirm button in dialogs.', screenshot: 'second.png' })
    // Exactly one screenshot line for this twin.
    expect((twice.match(/"screenshot": "second.png"/g) ?? []).length).toBe(1)
    expect(twice.includes('first.png')).toBe(false)
  })

  it('is idempotent: a second run with the same couplings is a byte-for-byte no-op', () => {
    const keyToScreenshot = new Map([['common.ok', 'dialog.png']])
    const first = coupleCatalog(FIXTURE, keyToScreenshot)
    expect(first.changed).toBe(true)
    const second = coupleCatalog(first.text, keyToScreenshot)
    expect(second.changed).toBe(false)
    expect(second.couplingCount).toBe(0)
    expect(second.text).toBe(first.text)
  })

  it('reports keys whose twin lacks a description (still couples them)', () => {
    const catalog = `{
  "common.ok": "OK",
  "@common.ok": {
    "placeholders": {}
  }
}
`
    const { coupledWithoutDescription, couplingCount } = coupleCatalog(catalog, new Map([['common.ok', 'x.png']]))
    expect(couplingCount).toBe(1)
    expect(coupledWithoutDescription).toEqual(['common.ok → x.png'])
  })

  it('reports keys with no twin at all and skips them (never synthesizes one)', () => {
    const catalog = `{
  "common.ok": "OK"
}
`
    const { text, missingTwins, couplingCount, changed } = coupleCatalog(catalog, new Map([['common.ok', 'x.png']]))
    expect(missingTwins).toEqual(['common.ok'])
    expect(couplingCount).toBe(0)
    expect(changed).toBe(false)
    expect(text).toBe(catalog)
  })

  it('reports keys absent from the catalog rather than minting them', () => {
    const catalog = `{
  "common.ok": "OK",
  "@common.ok": {
    "description": "x"
  }
}
`
    const { text, missingKeys, changed } = coupleCatalog(catalog, new Map([['common.ghost', 'x.png']]))
    expect(missingKeys).toEqual(['common.ghost'])
    expect(changed).toBe(false)
    expect(text).toBe(catalog)
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
