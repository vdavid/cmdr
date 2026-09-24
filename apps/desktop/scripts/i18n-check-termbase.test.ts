/**
 * Tests for the TERMBASE check (`i18n-check-termbase.ts`): schema problems in
 * `concepts.json` / `<tag>/terms.json` are errors, and coverage drift (a shipped key
 * whose English uses a concept while its translation uses none of the ruling's
 * forms) is a warning held to a per-locale baseline that only ratchets down.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  EXIT_SCHEMA,
  findDrift,
  inspectTermbase,
  report,
  shrinkWrap,
  validateConcepts,
  validateTerms,
} from './i18n-check-termbase.ts'
import type { Baseline } from './i18n-check-termbase.ts'
import { EXIT_CLEAN, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import type { Concepts } from './i18n-termbase-lib.ts'

const concepts: Concepts = {
  operation: { en: 'operation', match: ['operation', 'operations'], sense: 'file work', distinct: ['transfer'] },
  transfer: { en: 'transfer', match: ['transfer*'], sense: 'bytes moving' },
}

describe('validateConcepts', () => {
  it('accepts a well-formed registry', () => {
    expect(validateConcepts(concepts, 'concepts.json')).toEqual([])
  })
  it('flags missing fields, a non-kebab id, uppercase match forms, unknown fields, and a dangling distinct', () => {
    const errors = validateConcepts(
      {
        Bad_Id: { en: 'x', match: ['x'], sense: 's' },
        a: { en: 'a', match: ['Upper'], sense: 's', distinct: ['nope'], colour: 'red' },
        b: { match: [], sense: '' },
      },
      'concepts.json',
    )
    expect(errors.join('\n')).toMatch(/Bad_Id.*kebab-case/)
    expect(errors.join('\n')).toMatch(/a: match form "Upper" must be lowercase/)
    expect(errors.join('\n')).toMatch(/a: distinct names unknown concept "nope"/)
    expect(errors.join('\n')).toMatch(/a: unknown field "colour"/)
    expect(errors.join('\n')).toMatch(/b: missing "en"/)
    expect(errors.join('\n')).toMatch(/b: "match" must be a non-empty list/)
    expect(errors.join('\n')).toMatch(/b: missing "sense"/)
  })
  it('accepts a notMatch list and holds it to the match rules', () => {
    expect(validateConcepts({ a: { en: 'a', match: ['a'], notMatch: ['a b', '=a'], sense: 's' } }, 'c.json')).toEqual(
      [],
    )
    const text = validateConcepts(
      {
        a: { en: 'a', match: ['a'], notMatch: ['Upper'], sense: 's' },
        b: { en: 'b', match: ['b'], notMatch: 'x', sense: 's' },
      },
      'c.json',
    ).join('\n')
    expect(text).toMatch(/a: notMatch form "Upper" must be lowercase/)
    expect(text).toMatch(/b: "notMatch" must be a list of strings/)
  })
  it('keeps a notMatch key out of drift in every locale', () => {
    const en = { 'q.a': 'Undo the operation', 'q.m': 'A math operation' }
    const nl = { 'q.a': 'Maak de actie ongedaan', 'q.m': 'Een wiskundige handeling' }
    const terms = { operation: { chosen: 'bewerking', confidence: 'high' as const, sources: 's' } }
    const withNot: Concepts = {
      operation: { en: 'operation', match: ['operation'], notMatch: ['math operation'], sense: 's' },
    }
    expect(findDrift({ tag: 'nl', terms, concepts: withNot, en, locale: nl }).map((f) => f.key)).toEqual(['q.a'])
  })
})

describe('validateTerms', () => {
  const base = {
    tag: 'nl',
    knownConcepts: new Set(Object.keys(concepts)),
    enKeys: new Set(['queue.empty.body', 'queue.title']),
    decisionHeadings: new Set(['Operation queue (`queue.*`)']),
  }
  it('accepts a well-formed termbase', () => {
    const terms = {
      operation: {
        chosen: 'bewerking',
        accept: ['bewerkingen'],
        avoid: [{ form: 'actie', why: 'reads as a user action' }],
        confidence: 'high',
        sources: 'macOS',
        exceptions: { 'queue.empty.body': 'names the transfer kinds' },
        decision: 'Operation queue (`queue.*`)',
      },
    }
    expect(validateTerms({ ...base, terms })).toEqual([])
  })
  it('flags an unknown concept, missing required fields, a bad confidence, a dead exception key, and a missing decision', () => {
    const terms = {
      nope: { chosen: 'x', confidence: 'high', sources: 's' },
      operation: {
        confidence: 'sure',
        exceptions: { 'queue.gone': 'why' },
        decision: 'No such heading',
        avoid: [{ form: 'x' }],
      },
    }
    const text = validateTerms({ ...base, terms }).join('\n')
    expect(text).toMatch(/nl\/terms\.json: nope: unknown concept/)
    expect(text).toMatch(/operation: missing "chosen"/)
    expect(text).toMatch(/operation: missing "sources"/)
    expect(text).toMatch(/operation: confidence "sure" must be one of confirmed, high, tentative/)
    expect(text).toMatch(/operation: exception names "queue\.gone", which isn't an English key/)
    expect(text).toMatch(/operation: decision "No such heading" matches no heading in nl\/decisions\.md/)
    expect(text).toMatch(/operation: avoid\[0\] needs a "form" and a "why"/)
  })
  it('holds proseAccept to a list of strings', () => {
    const ok = { operation: { chosen: 'x', proseAccept: ['klus'], confidence: 'high', sources: 's' } }
    expect(validateTerms({ ...base, terms: ok })).toEqual([])
    const bad = { operation: { chosen: 'x', proseAccept: 'klus', confidence: 'high', sources: 's' } }
    expect(validateTerms({ ...base, terms: bad }).join('\n')).toMatch(/"proseAccept" must be a list of strings/)
  })
})

describe('validateTerms: decision resolution', () => {
  const base = {
    tag: 'nl',
    knownConcepts: new Set(['operation']),
    enKeys: new Set<string>(),
    decisionHeadings: new Set([
      'Operation queue: de hernoeming (`queue.*`, 2026-08-08)',
      'Operation log (`operationLog.*`)',
      'Operation log (`operationLog.*`) extra',
      'Archive password',
    ]),
  }
  const withDecision = (decision: unknown) =>
    validateTerms({ ...base, terms: { operation: { chosen: 'x', confidence: 'high', sources: 's', decision } } })

  it('resolves a unique heading prefix, so a heading can gain a date or keys without breaking the pointer', () => {
    expect(withDecision('Operation queue')).toEqual([])
    expect(withDecision('Archive')).toEqual([])
  })
  it('keeps an exact heading working even when it is also a prefix of another', () => {
    expect(withDecision('Operation log (`operationLog.*`)')).toEqual([])
  })
  it('accepts a list, checking each entry', () => {
    expect(withDecision(['Operation queue', 'Archive'])).toEqual([])
    expect(withDecision(['Archive', 'Nope']).join('\n')).toMatch(/decision "Nope" matches no heading/)
  })
  it('flags an ambiguous prefix and a missing one', () => {
    expect(withDecision('Operation').join('\n')).toMatch(/decision "Operation" matches 3 headings/)
    expect(withDecision('Nope').join('\n')).toMatch(/decision "Nope" matches no heading in nl\/decisions\.md/)
    expect(withDecision(3).join('\n')).toMatch(/"decision" must be a heading string or a list of them/)
  })
})

describe('findDrift', () => {
  const en = {
    'q.a': 'Undo the operation',
    'q.b': '{count, plural, one {# operation} other {# operations}}',
    'q.c': 'Hi',
  }
  const terms = {
    operation: { chosen: 'bewerking', accept: ['bewerkingen'], confidence: 'high' as const, sources: 's' },
  }
  it('lists a key whose English uses the concept while the translation uses no ruling form', () => {
    const nl = {
      'q.a': 'Maak de actie ongedaan',
      'q.b': '{count, plural, one {# bewerking} other {# bewerkingen}}',
      'q.c': 'Hoi',
    }
    expect(findDrift({ tag: 'nl', terms, concepts, en, locale: nl })).toEqual([
      { key: 'q.a', concept: 'operation', value: 'Maak de actie ongedaan' },
    ])
  })
  it('accepts a proseAccept form in a prose key and never in a name key', () => {
    const en = {
      'q.prose': 'Cmdr finished the operation you started, so you can close this window now.',
      'q.label': 'Operation log',
    }
    const nl = { 'q.prose': 'Cmdr heeft je klus afgerond, dus je kunt dit venster sluiten.', 'q.label': 'Kluslogboek' }
    const withProse = { operation: { ...terms.operation, proseAccept: ['klus'] } }
    expect(findDrift({ tag: 'nl', terms: withProse, concepts, en, locale: nl }).map((f) => f.key)).toEqual(['q.label'])
  })
  it('skips a key in exceptions and a key the locale lacks', () => {
    const withException = { operation: { ...terms.operation, exceptions: { 'q.a': 'deliberate' } } }
    expect(findDrift({ tag: 'nl', terms: withException, concepts, en, locale: { 'q.c': 'Hoi' } })).toEqual([])
  })
})

describe('inspectTermbase + report + shrinkWrap (fixture tree)', () => {
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
    root = mkdtempSync(join(tmpdir(), 'termbase-'))
    messagesRoot = join(root, 'messages')
    docsRoot = join(root, 'docs')
    write(join(messagesRoot, 'en', 'q.json'), { 'q.a': 'Undo the operation', 'q.b': 'Operation log' })
    write(join(messagesRoot, 'nl', 'q.json'), { 'q.a': 'Maak de actie ongedaan', 'q.b': 'Actielogboek' })
    write(join(messagesRoot, 'de', 'q.json'), { 'q.a': 'Vorgang rückgängig', 'q.b': 'Vorgangsprotokoll' })
    write(join(docsRoot, 'concepts.json'), concepts)
    write(join(docsRoot, 'nl', 'terms.json'), { operation: { chosen: 'bewerking', confidence: 'high', sources: 's' } })
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('skips a locale without terms.json silently and warns on drift past the baseline', () => {
    const outcome = inspectTermbase({ messagesRoot, docsRoot, baseline: { drift: {} } })
    expect(outcome.locales.map((l) => l.locale)).toEqual(['nl'])
    const out = capture()
    expect(report(outcome, out.write)).toBe(EXIT_ISSUES)
    expect(out.lines.join('\n')).toContain('q.a')
    expect(out.lines.join('\n')).not.toContain('de:')
  })

  it('stays clean at or under the baseline, and shrink-wrap lowers it', () => {
    const baseline: Baseline = { drift: { nl: 5 } }
    const outcome = inspectTermbase({ messagesRoot, docsRoot, baseline })
    const out = capture()
    expect(report(outcome, out.write)).toBe(EXIT_CLEAN)
    const path = join(root, 'baseline.json')
    expect(shrinkWrap(outcome, baseline, path)).toEqual(['nl'])
    expect((JSON.parse(readFileSync(path, 'utf8')) as Baseline).drift.nl).toBe(2)
  })

  it('drops a baseline for a locale that no longer has a termbase', () => {
    const baseline: Baseline = { drift: { nl: 2, fr: 4 } }
    const outcome = inspectTermbase({ messagesRoot, docsRoot, baseline })
    const path = join(root, 'baseline.json')
    expect(shrinkWrap(outcome, baseline, path)).toEqual(['fr'])
    expect((JSON.parse(readFileSync(path, 'utf8')) as Baseline).drift).toEqual({ nl: 2 })
  })

  it('warns when decisions.md grows past its byte budget, and ratchets the budget down only', () => {
    write(join(docsRoot, 'nl', 'decisions.md'), '# nl decisions\n\n## A\n\nShort.\n')
    const size = Buffer.byteLength('# nl decisions\n\n## A\n\nShort.\n')
    const baseline: Baseline = { drift: { nl: 2 }, decisionsBytes: { nl: size - 1 } }
    const grown = inspectTermbase({ messagesRoot, docsRoot, baseline })
    const out = capture()
    expect(report(grown, out.write)).toBe(EXIT_ISSUES)
    expect(out.lines.join('\n')).toMatch(
      new RegExp(`nl/decisions\\.md grew to ${String(size)} bytes, past its budget of ${String(size - 1)}`),
    )

    const roomy: Baseline = { drift: { nl: 2 }, decisionsBytes: { nl: size + 100 } }
    const outcome = inspectTermbase({ messagesRoot, docsRoot, baseline: roomy })
    expect(report(outcome, capture().write)).toBe(EXIT_CLEAN)
    const path = join(root, 'baseline.json')
    expect(shrinkWrap(outcome, roomy, path)).toEqual(['nl'])
    expect((JSON.parse(readFileSync(path, 'utf8')) as Baseline).decisionsBytes).toEqual({ nl: size })
  })

  it('records a decisions.md budget the first time it sees one, and drops it with the file', () => {
    write(join(docsRoot, 'nl', 'decisions.md'), '# nl decisions\n')
    const baseline: Baseline = { drift: { nl: 2 }, decisionsBytes: { fr: 10 } }
    const outcome = inspectTermbase({ messagesRoot, docsRoot, baseline })
    expect(report(outcome, capture().write)).toBe(EXIT_CLEAN)
    const path = join(root, 'baseline.json')
    expect(shrinkWrap(outcome, baseline, path)).toEqual(['fr', 'nl'])
    expect((JSON.parse(readFileSync(path, 'utf8')) as Baseline).decisionsBytes).toEqual({
      nl: Buffer.byteLength('# nl decisions\n'),
    })
  })

  it('exits with the schema code when a termbase names an unknown concept, whatever the drift', () => {
    write(join(docsRoot, 'nl', 'terms.json'), { nope: { chosen: 'x', confidence: 'high', sources: 's' } })
    const outcome = inspectTermbase({ messagesRoot, docsRoot, baseline: { drift: {} } })
    const out = capture()
    expect(report(outcome, out.write)).toBe(EXIT_SCHEMA)
    expect(out.lines.join('\n')).toMatch(/unknown concept/)
  })

  it('flags an exception whose English no longer matches the concept (a stale exception)', () => {
    write(join(docsRoot, 'nl', 'terms.json'), {
      operation: {
        chosen: 'bewerking',
        confidence: 'high',
        sources: 's',
        exceptions: { 'q.a': 'still matches', 'q.b': 'still matches too' },
      },
    })
    write(join(messagesRoot, 'en', 'q.json'), { 'q.a': 'Undo the operation', 'q.b': 'Activity log' })
    const errors = inspectTermbase({ messagesRoot, docsRoot, baseline: { drift: {} } }).schemaErrors.join('\n')
    expect(errors).toMatch(
      /nl\/terms\.json: operation: exception "q\.b" is stale: its English no longer matches the concept/,
    )
    expect(errors).not.toMatch(/"q\.a"/)
  })

  it("accepts a concept the locale proposed in its concepts-proposed.json, and validates that file's schema", () => {
    write(join(docsRoot, 'nl', 'concepts-proposed.json'), {
      queue: { en: 'queue', match: ['queue'], sense: 'the list' },
    })
    write(join(docsRoot, 'nl', 'terms.json'), { queue: { chosen: 'wachtrij', confidence: 'high', sources: 's' } })
    expect(inspectTermbase({ messagesRoot, docsRoot, baseline: { drift: {} } }).schemaErrors).toEqual([])

    write(join(docsRoot, 'nl', 'concepts-proposed.json'), { operation: { en: 'x', match: ['x'], sense: 's' } })
    expect(inspectTermbase({ messagesRoot, docsRoot, baseline: { drift: {} } }).schemaErrors.join('\n')).toMatch(
      /operation: already in concepts\.json/,
    )
  })
})
