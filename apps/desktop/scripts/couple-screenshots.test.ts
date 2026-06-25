import { describe, it, expect } from 'vitest'
import {
  couplingsFromReport,
  coupleCatalog,
  fileForKey,
  buildCoverageReport,
  renderCoverageReport,
  representativeFor,
  buildCouplings,
  REPRESENTATIVE_SCREENSHOTS,
} from './couple-screenshots.ts'

// `JSON.parse` is untyped (`any`); the catalogs are `{ key: string | object }`,
// so parse to a known shape for the assertions below.
const parse = (s: string): Record<string, unknown> => JSON.parse(s) as Record<string, unknown>

// An oxfmt-shaped fixture catalog mirroring the real `messages/en/<area>.json`:
// 2-space indent, each `@key` twin right after its message key, BLANK LINES
// between groups, a nested `placeholders` object, ICU braces, doubled
// apostrophes. The N1 safety guarantee under test: coupling edits ONLY
// `@key.screenshot` / `@key.screenshotNote` and leaves every other byte
// (values, other twin fields, indentation, and the blank-line grouping)
// byte-identical.
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
    const keyToCoupling = new Map([
      ['common.ok', { screenshot: 'dialog.png' }],
      ['common.greeting', { screenshot: 'status.png' }],
    ])
    const { text, changed, couplingCount } = coupleCatalog(FIXTURE, keyToCoupling)

    expect(changed).toBe(true)
    expect(couplingCount).toBe(2)

    // The output must still parse and carry every message value byte-identical.
    const before = parse(FIXTURE)
    const after = parse(text)
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

  it('writes a representative note (screenshotNote) and preserves every other byte', () => {
    const keyToCoupling = new Map([
      ['common.ok', { screenshot: 'rep.png', note: 'A stand-in image of the same dialog.' }],
    ])
    const { text, changed, couplingCount } = coupleCatalog(FIXTURE, keyToCoupling)
    expect(changed).toBe(true)
    expect(couplingCount).toBe(1)

    const after = parse(text)
    expect(after['@common.ok']).toEqual({
      description: 'Confirm button in dialogs.',
      screenshot: 'rep.png',
      screenshotNote: 'A stand-in image of the same dialog.',
    })
    // Values byte-identical; only the two new fields inserted on the twin.
    const reverted = text.replace(
      ',\n    "screenshot": "rep.png",\n    "screenshotNote": "A stand-in image of the same dialog."',
      '',
    )
    expect(reverted).toBe(FIXTURE)
  })

  it('REMOVES a stale screenshotNote when a key gains a direct (note-less) coupling', () => {
    // Couple representatively (with a note), then re-couple directly (no note):
    // the note must be removed, and the twin must stay valid + parseable.
    const rep = coupleCatalog(FIXTURE, new Map([['common.cancel', { screenshot: 'rep.png', note: 'stand-in' }]])).text
    expect((parse(rep)['@common.cancel'] as Record<string, unknown>).screenshotNote).toBe('stand-in')

    const direct = coupleCatalog(rep, new Map([['common.cancel', { screenshot: 'real.png' }]]))
    expect(direct.changed).toBe(true)
    const obj = parse(direct.text)
    const cancelTwin = obj['@common.cancel'] as Record<string, unknown>
    expect(cancelTwin).toEqual({
      description: 'Dismiss button in dialogs.',
      placeholders: {},
      screenshot: 'real.png',
    })
    expect('screenshotNote' in cancelTwin).toBe(false)
    // No dangling comma / broken JSON anywhere.
    expect(() => {
      JSON.parse(direct.text)
    }).not.toThrow()
  })

  it('preserves the blank lines between catalog groups', () => {
    const { text } = coupleCatalog(FIXTURE, new Map([['common.ok', { screenshot: 'dialog.png' }]]))
    // Same number of blank (empty) lines as the input: the round-trip bug this
    // guards would collapse them to zero.
    const blanks = (s: string) => s.split('\n').filter((l: string) => l === '').length
    expect(blanks(text)).toBe(blanks(FIXTURE))
  })

  it('replaces an existing screenshot value rather than duplicating it', () => {
    const once = coupleCatalog(FIXTURE, new Map([['common.ok', { screenshot: 'first.png' }]])).text
    const twice = coupleCatalog(once, new Map([['common.ok', { screenshot: 'second.png' }]])).text
    const obj = parse(twice)
    expect(obj['@common.ok']).toEqual({ description: 'Confirm button in dialogs.', screenshot: 'second.png' })
    // Exactly one screenshot line for this twin.
    expect((twice.match(/"screenshot": "second.png"/g) ?? []).length).toBe(1)
    expect(twice.includes('first.png')).toBe(false)
  })

  it('is idempotent: a second run with the same couplings is a byte-for-byte no-op', () => {
    const keyToCoupling = new Map([['common.ok', { screenshot: 'dialog.png' }]])
    const first = coupleCatalog(FIXTURE, keyToCoupling)
    expect(first.changed).toBe(true)
    const second = coupleCatalog(first.text, keyToCoupling)
    expect(second.changed).toBe(false)
    expect(second.couplingCount).toBe(0)
    expect(second.text).toBe(first.text)
  })

  it('is idempotent for a representative coupling too (note included)', () => {
    const keyToCoupling = new Map([['common.ok', { screenshot: 'rep.png', note: 'stand-in' }]])
    const first = coupleCatalog(FIXTURE, keyToCoupling)
    const second = coupleCatalog(first.text, keyToCoupling)
    expect(second.changed).toBe(false)
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
    const { coupledWithoutDescription, couplingCount } = coupleCatalog(
      catalog,
      new Map([['common.ok', { screenshot: 'x.png' }]]),
    )
    expect(couplingCount).toBe(1)
    expect(coupledWithoutDescription).toEqual(['common.ok → x.png'])
  })

  it('reports keys with no twin at all and skips them (never synthesizes one)', () => {
    const catalog = `{
  "common.ok": "OK"
}
`
    const { text, missingTwins, couplingCount, changed } = coupleCatalog(
      catalog,
      new Map([['common.ok', { screenshot: 'x.png' }]]),
    )
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
    const { text, missingKeys, changed } = coupleCatalog(catalog, new Map([['common.ghost', { screenshot: 'x.png' }]]))
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

describe('representativeFor', () => {
  const mappings = [
    { prefix: 'errors.write.', screenshot: 'write.png', note: 'write note' },
    { prefix: 'errors.', screenshot: 'errors.png', note: 'errors note' },
  ]
  it('returns the FIRST matching prefix (specific before broad)', () => {
    expect(representativeFor('errors.write.x.title', mappings)?.screenshot).toBe('write.png')
    expect(representativeFor('errors.listing.y.title', mappings)?.screenshot).toBe('errors.png')
  })
  it('returns null when no prefix matches', () => {
    expect(representativeFor('common.ok', mappings)).toBeNull()
  })
})

describe('buildCouplings (direct + representative, direct-wins)', () => {
  const mappings = [{ prefix: 'errors.', screenshot: 'error-example.png', note: 'the error layout' }]

  it('keeps direct couplings and never overwrites them with a representative', () => {
    const direct = new Map([['errors.listing.a.title', 'real-capture.png']])
    const allKeys = ['errors.listing.a.title', 'errors.listing.b.title']
    const captured = new Set(['real-capture.png', 'error-example.png'])
    const { byKey, directKeys, representativeKeys } = buildCouplings(direct, allKeys, captured, mappings)

    expect(byKey.get('errors.listing.a.title')).toEqual({ screenshot: 'real-capture.png' }) // direct, no note
    expect(byKey.get('errors.listing.b.title')).toEqual({ screenshot: 'error-example.png', note: 'the error layout' })
    expect(directKeys).toEqual(new Set(['errors.listing.a.title']))
    expect(representativeKeys).toEqual(new Set(['errors.listing.b.title']))
  })

  it('skips a representative whose screenshot was not actually captured', () => {
    const { byKey, representativeKeys } = buildCouplings(
      new Map(),
      ['errors.x.title'],
      new Set(['other.png']),
      mappings,
    )
    expect(byKey.size).toBe(0)
    expect(representativeKeys.size).toBe(0)
  })

  it('leaves a non-matching uncoupled key uncoupled', () => {
    const { byKey } = buildCouplings(new Map(), ['common.ok'], new Set(['error-example.png']), mappings)
    expect(byKey.has('common.ok')).toBe(false)
  })
})

describe('REPRESENTATIVE_SCREENSHOTS config', () => {
  it('lists specific prefixes before broader ones (first-match correctness)', () => {
    // For any two entries where one prefix is a prefix of the other, the longer
    // (more specific) one must come first, or it could never match.
    for (let i = 0; i < REPRESENTATIVE_SCREENSHOTS.length; i++) {
      for (let j = i + 1; j < REPRESENTATIVE_SCREENSHOTS.length; j++) {
        const a = REPRESENTATIVE_SCREENSHOTS[i].prefix
        const b = REPRESENTATIVE_SCREENSHOTS[j].prefix
        // An earlier entry must not be a prefix of a later one: `representativeFor`
        // returns the first match, so a broader prefix placed earlier would shadow
        // the more-specific one that follows, leaving it dead.
        expect(b.startsWith(a), `'${a}' (earlier) shadows '${b}' (later)`).toBe(false)
      }
    }
  })
  it('every entry has a non-empty note and a .png screenshot', () => {
    for (const m of REPRESENTATIVE_SCREENSHOTS) {
      expect(m.note.trim().length).toBeGreaterThan(0)
      expect(m.screenshot.endsWith('.png')).toBe(true)
    }
  })
})

describe('fileForKey', () => {
  it('maps a key to its area catalog file', () => {
    expect(fileForKey('settings.fsWatch.title')).toBe('settings.json')
    expect(fileForKey('common.ok')).toBe('common.json')
  })
})

describe('buildCoverageReport', () => {
  it('tallies direct vs representative vs uncoupled per area', () => {
    const directKeys = new Set(['common.ok', 'errors.listing.notFound.title'])
    const representativeKeys = new Set(['errors.listing.denied.title'])
    const keysByArea = new Map([
      ['common', ['common.ok', 'common.cancel']], // 1 direct, 1 uncoupled
      ['errors', ['errors.listing.notFound.title', 'errors.listing.denied.title']], // 1 direct, 1 representative
      ['about', ['about.version']], // 1 uncoupled
    ])
    const r = buildCoverageReport(directKeys, representativeKeys, keysByArea)

    expect(r.total).toBe(5)
    expect(r.direct).toBe(2)
    expect(r.representative).toBe(1)
    expect(r.uncoupled).toBe(2)

    // Areas are sorted by name.
    expect(r.areas.map((a) => a.area)).toEqual(['about', 'common', 'errors'])
    const errors = r.areas.find((a) => a.area === 'errors')
    expect(errors).toEqual({ area: 'errors', total: 2, direct: 1, representative: 1, uncoupled: 0 })
  })
})

describe('renderCoverageReport', () => {
  it('renders a markdown table distinguishing direct from representative', () => {
    const report = buildCoverageReport(
      new Set(['common.ok']),
      new Set(['common.cancel']),
      new Map([['common', ['common.ok', 'common.cancel', 'common.extra']]]),
    )
    const md = renderCoverageReport(report)
    expect(md).toContain('# Screenshot coverage')
    // 1 direct + 1 representative of 3 = 67%.
    expect(md).toContain('2 / 3 keys have a screenshot (67%)')
    expect(md).toContain('1 direct')
    expect(md).toContain('1 representative')
    expect(md).toContain('| common | 1 | 1 | 1 | 3 | 67% |')
    expect(md.toLowerCase()).toContain('partial')
    // It explains what a representative coupling is (honesty).
    expect(md.toLowerCase()).toContain('representative')
  })
})
