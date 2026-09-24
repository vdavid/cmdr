/**
 * Tests for the translation brief (`i18n-brief-lib.ts`): which keys a batch
 * selects, which shipped keys make its translation memory, which terms and
 * decisions are "in play", and that a blind run (`excludeTargetValues`) leaks none
 * of the batch's own current translations.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { buildBrief, changedKeys, renderBrief, selectKeys } from './i18n-brief-lib.ts'
import { nearestKeys } from './i18n-brief-memory.ts'
import type { BriefOptions } from './i18n-brief-lib.ts'

describe('selectKeys', () => {
  const en = { 'a.x': 'X', 'a.y': 'Y', 'b.z': 'Z' }
  it('expands a wildcard and keeps catalog order', () => {
    expect(selectKeys({ en, patterns: ['a.*'] })).toEqual(['a.x', 'a.y'])
  })
  it('throws on an exact key English lacks, naming it', () => {
    expect(() => selectKeys({ en, patterns: ['a.gone'] })).toThrow(/a\.gone/)
  })
  it('selects keys missing from any of the given locales', () => {
    expect(
      selectKeys({
        en,
        missingIn: [
          { 'a.x': 'x', 'b.z': 'z' },
          { 'a.x': 'x', 'a.y': 'y' },
        ],
      }),
    ).toEqual(['a.y', 'b.z'])
  })
})

describe('changedKeys', () => {
  it('returns keys added or whose value changed, in current order', () => {
    expect(changedKeys({ a: '1', b: '2', c: '3' }, { a: '1', b: 'old' })).toEqual(['b', 'c'])
  })
})

describe('nearestKeys', () => {
  const en = {
    'servers.sheet.title': 'Connect to server',
    'servers.sheet.user': 'User name',
    'servers.sheet.pass': 'Password',
    'servers.sheet.remember': 'Remember password in Keychain',
    'settings.keychain': 'Forget a password saved in Keychain',
    'other.cats': 'Cats and dogs',
  }
  const target = { ...en }
  it('puts siblings first, then the best word overlap across the catalog, skipping the batch', () => {
    const near = nearestKeys({
      key: 'servers.sheet.remember',
      en,
      targets: [target],
      batch: new Set(['servers.sheet.remember', 'servers.sheet.title']),
      count: 3,
    })
    expect(near).toHaveLength(3)
    expect(near[0].startsWith('servers.sheet.')).toBe(true)
    expect(near).toContain('settings.keychain')
    expect(near).not.toContain('servers.sheet.title')
    expect(near).not.toContain('other.cats')
  })
  it('keeps a far neighbor only on two shared words, or when all its words are in the key', () => {
    const far = {
      'main.hint': 'Exit full screen with Escape',
      'shortcuts.errorScreen': 'Error screen',
      'settings.fullScreen': 'Leave full screen',
      'queue.escape': 'Escape',
    }
    const near = nearestKeys({ key: 'main.hint', en: far, targets: [far], batch: new Set(['main.hint']), count: 4 })
    expect(near).toContain('settings.fullScreen')
    expect(near).toContain('queue.escape')
    expect(near).not.toContain('shortcuts.errorScreen')
  })
  it('skips a neighbor no selected locale has translated', () => {
    const near = nearestKeys({
      key: 'servers.sheet.remember',
      en,
      targets: [{ 'other.cats': 'x' }],
      batch: new Set(['servers.sheet.remember']),
      count: 4,
    })
    expect(near).toEqual([])
  })
})

describe('buildBrief (fixture tree)', () => {
  let root: string
  let opts: BriefOptions
  const write = (path: string, data: unknown) => {
    mkdirSync(join(path, '..'), { recursive: true })
    writeFileSync(path, typeof data === 'string' ? data : JSON.stringify(data, null, 2))
  }

  beforeAll(() => {
    root = mkdtempSync(join(tmpdir(), 'brief-'))
    const messagesRoot = join(root, 'messages')
    const docsRoot = join(root, 'docs')
    write(join(messagesRoot, 'en', 'queue.json'), {
      'queue.title': 'Operation queue',
      '@queue.title': { description: 'Window title of the queue.' },
      'queue.row.label': '{count, plural, one {# transfer} other {# transfers}} to {folder}',
      '@queue.row.label': {
        description: 'A queue row.',
        placeholders: { count: 'how many', folder: { type: 'String', example: 'Backup' } },
      },
      'queue.empty': 'Nothing queued',
      'queue.undo': 'Undo the operation',
      'queue.hint': 'Nothing sparkly: Cmdr still keeps it offline',
      'queue.offlineA': 'Make available offline',
      'queue.offlineB': 'Offline copy',
    })
    write(join(messagesRoot, 'nl', 'queue.json'), {
      'queue.title': 'Bewerkingenwachtrij: {app} houdt de wachtrij vast.',
      'queue.row.label': '{count, plural, one {# overdracht} other {# overdrachten}} naar {folder}',
      'queue.empty': 'Niets in de wachtrij',
      'queue.undo': 'Maak de bewerking ongedaan',
    })
    write(join(messagesRoot, 'de', 'queue.json'), {
      'queue.empty': 'Nichts in der Warteschlange',
      'queue.undo': 'Vorgang rückgängig machen',
    })
    write(join(docsRoot, 'concepts.json'), {
      operation: {
        en: 'operation',
        match: ['operation', 'operations'],
        sense: 'The file work Cmdr performs.',
        distinct: ['transfer', 'task', 'queue-word'],
      },
      transfer: { en: 'transfer', match: ['transfer*'], sense: 'Bytes moving between places.' },
      unrelated: { en: 'cat', match: ['cat'], sense: 'Not in play.' },
      task: { en: 'task', match: ['task', 'tasks'], sense: 'A to-do item, nowhere in this batch.' },
      'queue-word': { en: 'queue', match: ['=queue'], sense: 'The bare Queue label.' },
    })
    write(
      join(docsRoot, 'translator-instructions.md'),
      '# Translator instructions\n\nIntro for humans.\n\n## Instructions\n\nTranslate into {{LANGUAGE}}; read docs/i18n/{{TAG}}/style.md.\n',
    )
    write(
      join(docsRoot, 'translation-principles.md'),
      '# Translation principles\n\nWhy this file exists.\n\n## Principles\n\n- Pick the everyday word.\n',
    )
    write(join(docsRoot, 'nl', 'mechanics.json'), {
      quotes: { primary: ['‘', '’'], nested: ['“', '”'] },
      apostrophes: ['’'],
      hedges: [{ pattern: '\\(n\\)', why: 'a bracketed plural ending' }],
    })
    write(join(docsRoot, 'nl', 'terms.json'), {
      operation: {
        chosen: 'bewerking',
        forms: 'window title Bewerkingenwachtrij; with an app named, {app} houdt de wachtrij vast',
        accept: ['bewerkingen'],
        proseAccept: ['klus'],
        avoid: [{ form: 'actie', why: 'reads as a user action' }],
        confidence: 'high',
        sources: 'macOS',
        exceptions: { 'queue.row.label': 'names transfers, not operations' },
        decision: 'Operation queue (`queue.title`)',
      },
    })
    write(
      join(docsRoot, 'nl', 'style.md'),
      '# nl\n\nIntro.\n\n## Digest\n\n- Use `je`.\n- The queue window is Bewerkingenwachtrij.\n\n## Voice\n\nLong elaboration.',
    )
    write(
      join(docsRoot, 'nl', 'decisions.md'),
      `# nl decisions\n\n## Operation queue (\`queue.title\`)\n\nWe renamed it to Bewerkingenwachtrij.\n\n## Unrelated (\`settings.x\`)\n\nNope.\n\n## Wide (\`queue.*\`)\n\n${'Long text. '.repeat(400)}\n`,
    )
    opts = {
      langs: ['nl'],
      keys: ['queue.title', 'queue.row.label'],
      selection: '--keys queue.title,queue.row.label',
      messagesRoot,
      docsRoot,
      pileRoot: '/pile',
      repoRoot: root,
    }
  })
  afterAll(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('prints the header, the digest, and each key with its description, placeholders, and current value', () => {
    const text = renderBrief(buildBrief(opts))
    expect(text).toContain('/pile/nl/')
    expect(text).toContain('docs/nl/style.md')
    expect(text).toContain('- Use `je`.')
    expect(text).not.toContain('Long elaboration.')
    expect(text).toContain('Window title of the queue.')
    expect(text).toContain('{folder}: String, e.g. "Backup"')
    expect(text).toContain('{count}: how many')
    expect(text).toContain('Bewerkingenwachtrij')
  })

  it('lists the terms in play with their distinct neighbors, and none that are not', () => {
    const text = renderBrief(buildBrief(opts))
    expect(text).toContain('The file work Cmdr performs.')
    expect(text).toContain('Bytes moving between places.')
    expect(text).not.toContain('Not in play.')
    expect(text).toMatch(/nl: \*\*bewerking\*\*/)
    expect(text).toContain('avoid: actie (reads as a user action)')
    expect(text).toContain('names transfers, not operations')
    expect(text).toMatch(/transfer[\s\S]*nl: no ruling/)
  })

  it('prints a concept once and one line per language in a multi-language run', () => {
    const text = renderBrief(buildBrief({ ...opts, langs: ['de', 'nl'] }))
    expect(text.split('The file work Cmdr performs.').length - 1).toBe(1)
    expect(text).toMatch(/de: no ruling/)
    expect(text).toMatch(/nl: \*\*bewerking\*\*/)
  })

  it('pulls decisions citing a batch key or a covering wildcard, capped with a pointer', () => {
    const text = renderBrief(buildBrief(opts))
    expect(text).toContain('We renamed it to Bewerkingenwachtrij.')
    expect(text).not.toContain('Nope.')
    expect(text).toMatch(/cut; full section: docs\/nl\/decisions\.md:\d+/)
  })

  it('carries the translation memory: shipped neighbors with their target values, never batch keys', () => {
    const brief = buildBrief(opts)
    const memory = brief.sections.find((section) => section.name === 'memory')?.text ?? ''
    expect(memory).toContain('queue.undo')
    expect(memory).toContain('Maak de bewerking ongedaan')
    expect(memory).not.toContain('- `queue.title`')
    expect(memory).not.toContain('- `queue.row.label`')
  })

  it('leaks none of the batch keys own translations in a blind run, even quoted in a ruling or the digest', () => {
    const text = renderBrief(buildBrief({ ...opts, excludeTargetValues: true }))
    expect(text).not.toContain('Bewerkingenwachtrij')
    expect(text).not.toContain('houdt de wachtrij vast')
    expect(text).not.toContain('overdracht')
    expect(text).not.toContain('names transfers, not operations')
    expect(text).not.toContain('decision: "Operation queue')
    expect(text).toContain('- Use `je`.')
    expect(text).toContain('Maak de bewerking ongedaan')
  })

  it('embeds the translator instructions once, rendered for the language', () => {
    const text = renderBrief(buildBrief(opts))
    expect(text).toContain('Translate into Dutch (nl); read docs/i18n/nl/style.md.')
    expect(text).not.toContain('Intro for humans.')
    const multi = renderBrief(buildBrief({ ...opts, langs: ['de', 'nl'] }))
    expect(multi.split('Translate into').length - 1).toBe(1)
    expect(multi).toContain('Translate into German (de), Dutch (nl); read docs/i18n/<tag>/style.md.')
  })

  it('embeds the shared principles once, right after the instructions', () => {
    const brief = buildBrief({ ...opts, langs: ['de', 'nl'] })
    const names = brief.sections.map((section) => section.name)
    expect(names.indexOf('principles')).toBe(names.indexOf('instructions') + 1)
    const text = renderBrief(brief)
    expect(text.split('Pick the everyday word.').length - 1).toBe(1)
    expect(text).not.toContain('Why this file exists.')
  })

  it("shows each language's declared mechanics beside its digest, and says when none are declared", () => {
    const digest =
      buildBrief({ ...opts, langs: ['de', 'nl'] }).sections.find((section) => section.name === 'digest')?.text ?? ''
    expect(digest).toContain('quotes ‘…’, nested “…”')
    expect(digest).toContain('apostrophe ’')
    expect(digest).toContain('`\\(n\\)` (a bracketed plural ending)')
    expect(digest).toMatch(/de[\s\S]*no mechanics\.json yet/)
  })

  it('marks proseAccept forms as prose only', () => {
    const text = renderBrief(buildBrief(opts))
    expect(text).toContain('prose only: klus')
  })

  it('lists an easy-to-confuse neighbor only when its words appear in the batch English', () => {
    const text = renderBrief(buildBrief(opts))
    expect(text).toContain('The bare Queue label.')
    expect(text).not.toContain('A to-do item, nowhere in this batch.')
  })

  it("names each key's content words that no concept covers, so missing concepts are explicit", () => {
    const keys = buildBrief(opts).sections.find((section) => section.name === 'keys')?.text ?? ''
    // `{folder}` is a placeholder and `to` a stopword; `transfers` is covered by `transfer*`.
    expect(keys.slice(keys.indexOf('`queue.row.label`'))).not.toContain('No concept yet')
  })

  it('leaves generic English and one-off words out of "No concept yet"', () => {
    const brief = buildBrief({ ...opts, keys: ['queue.hint'] })
    const keys = brief.sections.find((section) => section.name === 'keys')?.text ?? ''
    // "offline" recurs in three keys; "nothing", "still", "Cmdr" are generic or a brand; "sparkly" is a one-off.
    expect(keys).toMatch(/No concept yet: offline\n|No concept yet: offline$/)
  })

  it('reports per-section sizes for --stats', () => {
    const brief = buildBrief(opts)
    expect(brief.sections.map((section) => section.name)).toEqual(
      expect.arrayContaining(['header', 'instructions', 'digest', 'keys', 'terms', 'memory', 'decisions']),
    )
  })
})
