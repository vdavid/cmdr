/**
 * Tests for the shared termbase helpers (`i18n-termbase-lib.ts`): how a concept's
 * `match` forms hit English copy, how `decisions.md` splits into sections and which
 * keys each heading cites, and how the digest comes out of `style.md`. The brief and
 * the termbase check both stand on these, so a matcher bug here would mislead both.
 */
import { describe, it, expect } from 'vitest'
import {
  compileMatch,
  conceptsInText,
  englishMatchText,
  extractDigest,
  parseDecisions,
  sectionCitesKey,
  localeValueCarriesTerm,
} from './i18n-termbase-lib.ts'
import type { Concepts } from './i18n-termbase-lib.ts'

describe('compileMatch', () => {
  it('matches a whole word case-insensitively', () => {
    const hits = compileMatch(['operation'])
    expect(hits('Undo the Operation')).toBe(true)
    expect(hits('operations')).toBe(false)
    expect(hits('cooperation')).toBe(false)
  })
  it('treats a trailing star as a prefix', () => {
    const hits = compileMatch(['index*'])
    expect(hits('Indexing your drive')).toBe(true)
    expect(hits('the index')).toBe(true)
    expect(hits('reindex')).toBe(false)
  })
  it('matches a multi-word phrase across any whitespace', () => {
    const hits = compileMatch(['go to path'])
    expect(hits('Go  to path…')).toBe(true)
    expect(hits('go to paths')).toBe(false)
  })
  it('matches a form that ends in punctuation', () => {
    expect(compileMatch(['browse…'])('Browse…')).toBe(true)
    expect(compileMatch(['browse...'])('Browse...')).toBe(true)
  })
  it('treats a leading = as a whole-value match, ignoring edge punctuation and case', () => {
    const hits = compileMatch(['=back'])
    expect(hits('Back')).toBe(true)
    expect(hits(' Back… ')).toBe(true)
    expect(hits('Go back')).toBe(false)
    expect(compileMatch(['=go back'])('Go  back')).toBe(true)
  })
  it('treats curly apostrophes and quotes as straight ones, in the form and in the text', () => {
    expect(compileMatch(["isn't complete"])('The copy isn’t complete')).toBe(true)
    expect(compileMatch(['isn’t complete'])("The copy isn't complete")).toBe(true)
    expect(compileMatch(['=“done”'])('"Done"')).toBe(true)
  })
  it('keeps accented letters inside the word boundary', () => {
    expect(compileMatch(['caf'])('café')).toBe(false)
  })
})

describe('englishMatchText', () => {
  it('drops placeholder names, plural categories, and tag names, keeping the visible copy', () => {
    const text = englishMatchText('a.b', '{count, plural, one {# operation} other {# operations}} in <b>queue</b>')
    expect(text).toContain('operation')
    expect(text).toContain('queue')
    expect(text).not.toContain('count')
    expect(text).not.toContain('plural')
  })
  it('strips raw {tokens} from a raw-family value', () => {
    expect(englishMatchText('errors.x', 'Open {system_settings} now')).toBe('Open  now')
  })
  it('drops markdown code spans, which are literal commands rather than copy', () => {
    expect(englishMatchText('errors.x', 'Try `ping <hostname>` in Terminal')).toBe('Try  in Terminal')
  })
  it('drops markdown link targets and tag-shaped tokens from a raw value', () => {
    expect(englishMatchText('errors.x', 'Open [Settings](x-apple.systempreferences:com.apple.folder) for <name>')).toBe(
      'Open [Settings] for ',
    )
  })
  it('unescapes the doubled ICU apostrophe', () => {
    expect(englishMatchText('a.b', "Couldn''t copy")).toBe("Couldn't copy")
  })
})

describe('conceptsInText', () => {
  const concepts: Concepts = {
    operation: { en: 'operation', match: ['operation', 'operations'], sense: 'file work' },
    transfer: { en: 'transfer', match: ['transfer*'], sense: 'bytes moving' },
  }
  it('returns every concept whose match hits, sorted by id', () => {
    expect(conceptsInText(concepts, 'Transferring this operation')).toEqual(['operation', 'transfer'])
  })
  it('drops a concept whose notMatch hits the same text, whatever its match says', () => {
    const withNot: Concepts = {
      operation: { en: 'operation', match: ['operation'], notMatch: ['math operation', '=operation'], sense: 's' },
    }
    expect(conceptsInText(withNot, 'Undo the operation')).toEqual(['operation'])
    expect(conceptsInText(withNot, 'A math operation failed')).toEqual([])
    expect(conceptsInText(withNot, 'Operation')).toEqual([])
  })
  it('returns nothing on a miss', () => {
    expect(conceptsInText(concepts, 'Nothing here')).toEqual([])
  })
})

describe('localeValueCarriesTerm', () => {
  it('is a case-insensitive substring test over chosen and accept', () => {
    const term = { chosen: 'bewerking', accept: ['Bewerkingen'], confidence: 'high' as const, sources: 's' }
    expect(localeValueCarriesTerm('nl', 'Bewerkingenwachtrij', term)).toBe(true)
    expect(localeValueCarriesTerm('nl', 'De actie', term)).toBe(false)
  })
  it('reads a trailing * on an accept form as a word-start prefix, where a bare form is any substring', () => {
    const term = { chosen: 'bewerking', accept: ['wachtrij*'], confidence: 'high' as const, sources: 's' }
    expect(localeValueCarriesTerm('nl', 'Wachtrijen leeg', term)).toBe(true)
    expect(localeValueCarriesTerm('nl', 'Bewerkingenwachtrij', { ...term, chosen: 'x' })).toBe(false)
    expect(localeValueCarriesTerm('nl', 'Bewerkingenwachtrij', { ...term, chosen: 'x', accept: ['wachtrij'] })).toBe(true)
  })
  it('treats a curly apostrophe in the translation or the form as a straight one', () => {
    const term = { chosen: "n'est", confidence: 'high' as const, sources: 's' }
    expect(localeValueCarriesTerm('fr', 'La copie n’est pas terminée', term)).toBe(true)
    expect(localeValueCarriesTerm('fr', "La copie n''est pas terminée", { ...term, chosen: 'n’est' })).toBe(true)
  })
  it("reads an ICU-doubled apostrophe as one, so an accept form like foto's matches", () => {
    const term = { chosen: "foto's", confidence: 'high' as const, sources: 's' }
    expect(localeValueCarriesTerm('nl', "{count} foto''s", term)).toBe(true)
    expect(localeValueCarriesTerm('nl', "Geen foto''s: {reason} en {x", term)).toBe(true)
  })
  it('ignores placeholder names, so a term inside a {name} never counts', () => {
    const term = { chosen: 'count', confidence: 'high' as const, sources: 's' }
    expect(localeValueCarriesTerm('nl', '{count} bestanden', term)).toBe(false)
  })
})

describe('parseDecisions', () => {
  const md = [
    '# nl decisions',
    '',
    '## Operation queue: de hernoeming (`queue.*`, `commands.queueShow.label`, 2026-08-08)',
    'Body one.',
    '',
    '### Sub about `queue.row.status`',
    'Sub body.',
    '',
    '## Analytics (`settings.analytics.enabled.label`/`.description`, `onboarding.stepBeta.analyticsLede`/`analyticsTitle`)',
    '```',
    '## not a heading inside a fence',
    '```',
    '## Rollback (`rollbackConfirm.body`, `errors.mutation.trash*`)',
    'x',
  ].join('\n')
  const sections = parseDecisions(md)

  it('splits on ## and ### headings outside code fences, with 1-based heading lines', () => {
    expect(sections.map((s) => [s.level, s.line])).toEqual([
      [2, 3],
      [3, 6],
      [2, 9],
      [2, 13],
    ])
    expect(sections[0].heading).toBe(
      'Operation queue: de hernoeming (`queue.*`, `commands.queueShow.label`, 2026-08-08)',
    )
  })
  it('gives a ## section its ### children in the body, and a ### only its own', () => {
    expect(sections[0].body).toContain('Sub body.')
    expect(sections[1].body).not.toContain('Body one.')
    expect(sections[2].body).toContain('## not a heading inside a fence')
  })
  it('resolves relative citations against the previous one in the heading', () => {
    expect(sections[2].citations).toEqual([
      'settings.analytics.enabled.label',
      'settings.analytics.enabled.description',
      'onboarding.stepBeta.analyticsLede',
      'onboarding.stepBeta.analyticsTitle',
    ])
  })
  it('keeps wildcards and namespace-less citations as written', () => {
    expect(sections[3].citations).toEqual(['rollbackConfirm.body', 'errors.mutation.trash*'])
  })
})

describe('sectionCitesKey', () => {
  const namespaces = new Set(['queue', 'fileOperations', 'errors', 'settings', 'servers'])
  const cites = (citation: string, key: string) => sectionCitesKey([citation], key, namespaces)
  it('matches an exact key', () => {
    expect(cites('queue.row.status', 'queue.row.status')).toBe(true)
  })
  it('matches a namespace wildcard and a prefix star', () => {
    expect(cites('queue.*', 'queue.row.status')).toBe(true)
    expect(cites('errors.mutation.trash*', 'errors.mutation.trashFull')).toBe(true)
    expect(cites('errors.mutation.trash*', 'errors.mutation.rename')).toBe(false)
  })
  it('matches a namespace-less citation as a segment-aligned suffix', () => {
    expect(cites('rollbackConfirm.body', 'fileOperations.rollbackConfirm.body')).toBe(true)
    expect(cites('rollbackConfirm.body', 'fileOperations.xrollbackConfirm.body')).toBe(false)
  })
  it('resolves a wildcard in the middle of a citation', () => {
    const ns = new Set(['mtp', 'fileExplorer'])
    expect(parseDecisions('## Retry (`mtp.*.retry`)\nx')[0].citations).toEqual(['mtp.*.retry'])
    expect(sectionCitesKey(['mtp.*.retry'], 'mtp.connect.retry', ns)).toBe(true)
    expect(sectionCitesKey(['mtp.*.retry'], 'mtp.connect.cancel', ns)).toBe(false)
    expect(sectionCitesKey(['mtp.*.retry'], 'fileExplorer.mtp.connect.retry', ns)).toBe(false)
  })
  it('never suffix-matches a citation that starts at a real namespace', () => {
    expect(cites('servers.*', 'settings.servers.title')).toBe(false)
  })
})

describe('extractDigest', () => {
  it('returns the ## Digest section up to the next ## heading, keeping its ### subsections', () => {
    const md = '# nl\n\nIntro.\n\n## Digest\n\n- je\n\n### Traps\n\n- x\n\n## Voice\n\nMore.'
    expect(extractDigest(md)).toBe('- je\n\n### Traps\n\n- x')
  })
  it('returns undefined when the style guide has no digest', () => {
    expect(extractDigest('# nl\n\n## Voice\n')).toBeUndefined()
  })
})
