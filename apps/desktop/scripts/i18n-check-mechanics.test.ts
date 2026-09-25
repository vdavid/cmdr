/**
 * Tests for the MECHANICS check (`i18n-check-mechanics.ts`): a locale's declared
 * typography (`docs/i18n/<tag>/mechanics.json`) held against its catalog values.
 * Schema problems are errors; findings are warnings held to a per-locale baseline
 * that only ratchets down.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  EXIT_SCHEMA,
  adopt,
  findMechanicsIssues,
  inspectMechanics,
  report,
  shrinkWrap,
  validateMechanics,
} from './i18n-check-mechanics.ts'
import type { MechanicsBaseline } from './i18n-check-mechanics.ts'
import { EXIT_CLEAN, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import { mechanicsSummary } from './i18n-mechanics-lib.ts'
import type { Mechanics } from './i18n-mechanics-lib.ts'

const guillemets: Mechanics = {
  quotes: { primary: ['«', '»'], nested: ['“', '”'] },
  ellipsis: { glyph: '…' },
  apostrophes: ['’'],
  spacing: [{ pattern: '«(?![\\u00a0\\u202f])', why: 'a narrow no-break space follows «' }],
  hedges: [{ pattern: '\\(e\\)', why: 'a bracketed gender ending' }],
}

const kinds = (tag: string, messages: Record<string, string>, mechanics: Mechanics = guillemets) =>
  findMechanicsIssues({ tag, mechanics, messages }).map(({ key, kind }) => `${key}:${kind}`)

describe('validateMechanics', () => {
  it('accepts a full declaration and a minimal one', () => {
    expect(validateMechanics(guillemets, 'fr')).toEqual([])
    expect(validateMechanics({ quotes: { primary: ['“', '”'] }, ellipsis: { glyph: '…' } }, 'en')).toEqual([])
  })
  it('requires the ellipsis: a known glyph, and only a no-break space before it', () => {
    const quotes = { primary: ['“', '”'] }
    expect(validateMechanics({ quotes, ellipsis: { glyph: '⋯', spaceBefore: '\u00a0' } }, 'ok')).toEqual([])
    const text = [
      validateMechanics({ quotes }, 'a'),
      validateMechanics({ quotes, ellipsis: { glyph: '...' } }, 'b'),
      validateMechanics({ quotes, ellipsis: { glyph: '…', spaceBefore: ' ' } }, 'c'),
      validateMechanics({ quotes, ellipsis: { glyph: '…', after: 'x' } }, 'd'),
    ]
      .flat()
      .join('\n')
    expect(text).toMatch(/a\/mechanics\.json: "ellipsis" must be an object with a "glyph"/)
    expect(text).toMatch(/b\/mechanics\.json: "\.\.\." isn't an ellipsis glyph/)
    expect(text).toMatch(/c\/mechanics\.json: "ellipsis\.spaceBefore" must be a no-break space/)
    expect(text).toMatch(/d\/mechanics\.json: ellipsis: unknown field "after"/)
  })
  it('flags a missing or malformed quote pair, a non-quote mark, a bad regex, a rule without a why, and unknown fields', () => {
    const text = [
      validateMechanics({}, 'a'),
      validateMechanics({ quotes: { primary: ['«'] } }, 'b'),
      validateMechanics({ quotes: { primary: ['x', 'y'] } }, 'c'),
      validateMechanics({ quotes: { primary: ['“', '”'] }, hedges: [{ pattern: '(', why: 'w' }] }, 'd'),
      validateMechanics({ quotes: { primary: ['“', '”'] }, spacing: [{ pattern: 'x' }] }, 'e'),
      validateMechanics({ quotes: { primary: ['“', '”'] }, colour: 1 }, 'f'),
      validateMechanics({ quotes: { primary: ['“', '”'] }, apostrophes: ['ab'] }, 'g'),
    ]
      .flat()
      .join('\n')
    expect(text).toMatch(/a\/mechanics\.json: "quotes\.primary" must be an opening and a closing quote mark/)
    expect(text).toMatch(/b\/mechanics\.json: "quotes\.primary" must be an opening and a closing quote mark/)
    expect(text).toMatch(/c\/mechanics\.json: "x" isn't a quotation mark/)
    expect(text).toMatch(/d\/mechanics\.json: hedges\[0\] pattern doesn't compile/)
    expect(text).toMatch(/e\/mechanics\.json: spacing\[0\] needs a "pattern" and a "why"/)
    expect(text).toMatch(/f\/mechanics\.json: unknown field "colour"/)
    expect(text).toMatch(/g\/mechanics\.json: "apostrophes" must be a list of single characters/)
  })
})

describe('findMechanicsIssues', () => {
  it('flags straight double quotes, three dots, and quote marks outside the declared pairs', () => {
    expect(
      kinds('fr', {
        'a.ok': 'Ouvrir «\u202f{name}\u202f»…',
        'a.straight': 'Ouvrir "{name}"',
        'a.dots': 'Chargement...',
        'a.foreign': 'Ouvrir «\u202f„{name}“\u202f»',
      }),
    ).toEqual(['a.dots:ellipsis', 'a.foreign:quote-mark', 'a.straight:straight-quote'])
  })

  it('holds the ellipsis to the declared glyph', () => {
    const hant: Mechanics = { quotes: { primary: ['「', '」'] }, ellipsis: { glyph: '⋯' } }
    expect(kinds('zh-Hant', { 'a.ok': '載入中⋯', 'a.latin': '載入中…', 'a.dots': '載入中...' }, hant)).toEqual([
      'a.dots:ellipsis',
      'a.latin:ellipsis',
    ])
    expect(kinds('fr', { 'a.ok': 'Chargement…', 'a.cjk': 'Chargement⋯' })).toEqual(['a.cjk:ellipsis'])
  })

  it('with no spaceBefore, a label-ending ellipsis hugs its word', () => {
    expect(kinds('fr', { 'a.ok': 'Ouvrir…', 'a.spaced': 'Ouvrir …', 'a.lead': '… et plus', 'a.mid': 'a … b' })).toEqual(
      ['a.spaced:ellipsis'],
    )
  })

  it('with a spaceBefore, a label-ending ellipsis takes exactly that space; leading, range, and truncating ones stay', () => {
    const de: Mechanics = { quotes: { primary: ['„', '“'] }, ellipsis: { glyph: '…', spaceBefore: '\u00a0' } }
    expect(
      kinds(
        'de',
        {
          'a.ok': 'Wird geladen\u00a0…',
          'a.glued': 'Wird geladen…',
          'a.ascii': 'Wird geladen …',
          'a.insert': 'Öffnen mit {app}…',
          'a.quoted': '„Einführung …“',
          'a.lead': '…und mehr',
          'a.range': '1…5',
          'a.trunc': 'Datei…name',
        },
        de,
      ),
    ).toEqual(['a.ascii:ellipsis', 'a.glued:ellipsis', 'a.insert:ellipsis', 'a.quoted:ellipsis'])
  })

  it('reads ICU: a doubled apostrophe is one apostrophe, placeholders and categories are not text', () => {
    const mechanics: Mechanics = {
      quotes: { primary: ['“', '”'], nested: ['‘', '’'] },
      ellipsis: { glyph: '…' },
      apostrophes: ["'"],
    }
    expect(
      kinds(
        'en',
        {
          'a.icu': "{count, plural, one {# file isn''t here} other {# files aren''t here}}",
          'a.escaped': "It''s '{'literal'}'",
          'a.nested': '{kind, select, a {“{name}”} other {‘{name}’}}',
        },
        mechanics,
      ),
    ).toEqual([])
  })

  it('treats a raw family literally: its apostrophes are single, and a code span is typed verbatim', () => {
    expect(
      kinds('fr', {
        'errors.a.message': 'L’accès à «\u202f{path}\u202f» est refusé.',
        'errors.a.suggestion': 'Lancez `xattr -d "com.apple.quarantine" ...` dans le Terminal.',
        'errors.b.message': 'Impossible d’ouvrir "{path}".',
      }),
    ).toEqual(['errors.b.message:straight-quote'])
  })

  it('checks each plural branch on its own and keeps the spacing around an insert', () => {
    expect(
      kinds('fr', {
        'a.branch': '{count, plural, one {«{name}»} other {«\u202f{name}\u202f»}}',
      }),
    ).toEqual(['a.branch:spacing'])
  })

  it('never flags a single guillemet, the breadcrumb separator in every language', () => {
    expect(kinds('fr', { 'a.crumb': 'Réglages › IA' })).toEqual([])
    const declared = { quotes: { primary: ['«', '»'], nested: ['‹', '›'] }, ellipsis: { glyph: '…' } }
    expect(validateMechanics(declared, 'fr')).toEqual([])
  })

  it('flags a hedge once per key, whatever the number of hits', () => {
    const found = findMechanicsIssues({
      tag: 'fr',
      mechanics: guillemets,
      messages: { 'a.hedge': 'Connecté(e) et prêt(e)' },
    })
    expect(found).toEqual([{ key: 'a.hedge', kind: 'hedge', detail: 'a bracketed gender ending: "(e)"' }])
  })
})

describe('mechanicsSummary', () => {
  it('names the declared ellipsis, and the space before a label-ending one', () => {
    expect(mechanicsSummary({ quotes: { primary: ['「', '」'] }, ellipsis: { glyph: '⋯' } })).toContain(
      'ellipsis ⋯, never `...`',
    )
    const de = mechanicsSummary({
      quotes: { primary: ['„', '“'] },
      ellipsis: { glyph: '…', spaceBefore: '\u00a0' },
    })
    expect(de).toContain('ellipsis … with a no-break space (U+00A0) before a label-ending one')
    expect(de).toContain('never a straight `"`')
  })
})

describe('inspectMechanics + report + shrinkWrap + adopt (fixture tree)', () => {
  let root: string
  let messagesRoot: string
  let docsRoot: string
  const write = (path: string, data: unknown) => {
    mkdirSync(join(path, '..'), { recursive: true })
    writeFileSync(path, typeof data === 'string' ? data : JSON.stringify(data, null, 2))
  }
  const capture = () => {
    const lines: string[] = []
    return { lines, write: (line: string) => void lines.push(line) }
  }

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'mechanics-'))
    messagesRoot = join(root, 'messages')
    docsRoot = join(root, 'docs')
    write(join(messagesRoot, 'en', 'a.json'), { 'a.x': 'Open "{name}"', 'a.y': 'Loading…' })
    write(join(messagesRoot, 'en-GB', 'a.json'), { 'a.y': 'Loading...' })
    write(join(messagesRoot, 'fr', 'a.json'), { 'a.x': 'Ouvrir "{name}"', 'a.y': 'Chargement...' })
    write(join(messagesRoot, 'de', 'a.json'), { 'a.x': 'Öffnen "{name}"', 'a.y': 'Laden...' })
    write(join(docsRoot, 'en', 'mechanics.json'), {
      quotes: { primary: ['“', '”'], nested: ['‘', '’'] },
      ellipsis: { glyph: '…' },
    })
    write(join(docsRoot, 'fr', 'mechanics.json'), guillemets)
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('skips a locale with no mechanics.json, and an overlay inherits its base language', () => {
    const outcome = inspectMechanics({ messagesRoot, docsRoot, baseline: { findings: {} } })
    expect(outcome.locales.map((l) => l.locale)).toEqual(['en', 'en-GB', 'fr'])
    expect(outcome.locales.find((l) => l.locale === 'en-GB')?.issues.map((i) => i.kind)).toEqual(['ellipsis'])
  })

  it('warns past the baseline, listing every finding, and stays one line within it', () => {
    const past = capture()
    const outcome = inspectMechanics({ messagesRoot, docsRoot, baseline: { findings: { en: 1, 'en-GB': 1 } } })
    expect(report(outcome, past.write)).toBe(EXIT_ISSUES)
    expect(past.lines.join('\n')).toMatch(/fr: 2 typography findings \(no baseline\)/)
    expect(past.lines.join('\n')).toContain('a.y')

    const within = capture()
    const ok = inspectMechanics({ messagesRoot, docsRoot, baseline: { findings: { en: 1, 'en-GB': 1, fr: 2 } } })
    expect(report(ok, within.write)).toBe(EXIT_CLEAN)
  })

  it('ratchets down, drops a locale that lost its file, and never raises a number', () => {
    const baseline: MechanicsBaseline = { findings: { en: 3, fr: 1, de: 4 } }
    const outcome = inspectMechanics({ messagesRoot, docsRoot, baseline })
    const path = join(root, 'baseline.json')
    expect(shrinkWrap(outcome, baseline, path)).toEqual(['de', 'en'])
    expect((JSON.parse(readFileSync(path, 'utf8')) as MechanicsBaseline).findings).toEqual({ en: 1, fr: 1 })
  })

  it('adopt records a locale with no baseline at its current count, and refuses one that has a baseline', () => {
    const baseline: MechanicsBaseline = { findings: { en: 1 } }
    const outcome = inspectMechanics({ messagesRoot, docsRoot, baseline })
    const path = join(root, 'baseline.json')
    expect(adopt(outcome, baseline, 'fr', path)).toBe(2)
    expect((JSON.parse(readFileSync(path, 'utf8')) as MechanicsBaseline).findings.fr).toBe(2)
    expect(() => adopt(outcome, baseline, 'en', path)).toThrow(/already has a baseline/)
    expect(() => adopt(outcome, baseline, 'de', path)).toThrow(/no mechanics\.json/)
  })

  it('exits with the schema code on a malformed mechanics.json', () => {
    write(join(docsRoot, 'fr', 'mechanics.json'), { quotes: {} })
    const outcome = inspectMechanics({ messagesRoot, docsRoot, baseline: { findings: {} } })
    expect(report(outcome, capture().write)).toBe(EXIT_SCHEMA)
  })
})
