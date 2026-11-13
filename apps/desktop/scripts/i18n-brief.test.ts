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
import { buildBrief, renderBrief } from './i18n-brief-lib.ts'
import { changedKeys, selectKeys, staleKeys } from './i18n-brief-select.ts'
import { parseBriefArgs } from './i18n-brief.ts'
import { changeBase } from './i18n-repo-lib.ts'
import { sourceHash } from './i18n-catalog-lib.ts'
import { contentWords, isGenericWord, nearestKeys } from './i18n-brief-memory.ts'
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

describe('staleKeys', () => {
  it('returns keys a target translated from other English, or carries with no hash, in catalog order', () => {
    const en = { 'a.x': 'X now', 'a.y': 'Y', 'a.z': 'Z' }
    const target = {
      messages: { 'a.x': 'x', 'a.y': 'y', 'a.z': 'z' },
      metadata: { 'a.x': { sourceHash: sourceHash('X before') }, 'a.y': { sourceHash: sourceHash('Y') } },
    }
    expect(staleKeys(en, [target])).toEqual(['a.x', 'a.z'])
  })
})

describe('parseBriefArgs', () => {
  it('reads the flags', () => {
    expect(parseBriefArgs(['--lang', 'nl', '--keys', 'a.b,c.*', '--include-stale', '--parallel'])).toMatchObject({
      lang: 'nl',
      keys: 'a.b,c.*',
      includeStale: true,
      parallel: true,
    })
  })
  it('refuses a stray argument, so `--keys a b c` never silently briefs only `a`', () => {
    expect(() => parseBriefArgs(['--lang', 'nl', '--keys', 'a.x', 'a.y', 'a.z'])).toThrow(/a\.y.*comma/s)
  })
})

describe('changeBase', () => {
  it('diffs against where the branch left the ref, so keys the ref changed since then stay out', () => {
    const calls: string[][] = []
    const base = changeBase('main', (args) => {
      calls.push(args)
      return args[0] === 'merge-base' ? 'abc123' : 'ok'
    })
    expect(base).toBe('abc123')
    expect(calls).toContainEqual(['merge-base', 'main', 'HEAD'])
  })
  it('says so when the ref is unknown', () => {
    expect(() => changeBase('nope', () => undefined)).toThrow(/"nope" isn't a git ref/)
  })
})

describe('contentWords', () => {
  it('leaves out function words, contraction stubs, and domains', () => {
    const words = contentWords('a.b', 'It hasn’t worked since the move; see getcmdr.com')
    for (const noise of ['hasn', 'since', 'getcmdr', 'com']) expect(words).not.toContain(noise)
    expect(words).toEqual(['worked', 'move', 'see'])
  })
  it('counts everyday English as generic, never as a missing concept', () => {
    const everyday = ['got', 'sitting', 'longer', 'called', 'others', 'reach', 'usual', 'leaving']
    // Words the SMB negotiation toasts listed as missing concepts, with nothing to mine.
    const smbToasts = ['security', 'agreed', 'misbehaving', 'cause', 'someone', 'stays', 'directly']
    for (const word of [...everyday, ...smbToasts]) {
      expect(isGenericWord(word), word).toBe(true)
    }
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
      'queue.reveal': '{platform, select, macos {Show {name} in Finder} other {Show {name} in the file manager}}',
      'errors.queue.revealHint':
        '{platform, select, macos {Open {system_settings} and allow it.} other {Check the folder permissions.}}',
      'errors.queue.undoHint': '{platform, select, macos {Undo the operation from the Edit menu.} other {Try again.}}',
      'queue.trash': '{platform, select, windows {Delete} other {Move to trash}}',
    })
    write(join(messagesRoot, 'en', 'settings.json'), {
      'settings.queue.keywords': 'operation, offline, jobs',
      '@settings.queue.keywords': { description: 'Search keywords for the queue setting.' },
    })
    write(join(messagesRoot, 'nl', 'queue.json'), {
      'queue.title': 'Bewerkingenwachtrij: {app} houdt de wachtrij vast.',
      'queue.row.label': '{count, plural, one {# overdracht} other {# overdrachten}} naar {folder}',
      'queue.empty': 'Niets in de wachtrij',
      'queue.undo': 'Maak de bewerking ongedaan',
    })
    write(join(messagesRoot, 'nl-BE', 'queue.json'), { 'queue.title': 'Wachtrij van bewerkingen' })
    write(join(messagesRoot, 'es', 'queue.json'), { 'queue.empty': 'Nada en la cola' })
    write(join(messagesRoot, 'es-419', 'queue.json'), {})
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
        windows: { chosen: 'taak', accept: ['taken'] },
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

  it('names each language’s run directory, its values file, and the batch-scoped check command', () => {
    const header = buildBrief({ ...opts, workDirs: { nl: '/work/nl/20261113-020000' } }).sections[0].text
    expect(header).toContain('`/work/nl/20261113-020000/values.json`')
    expect(header).toContain('`pnpm i18n:check-locale nl --keys-file /work/nl/20261113-020000/keys.txt`')
  })

  it('notes every key whose macOS text is frozen, plain values included, with the way to fix a wrong one', () => {
    const keys = buildBrief({ ...opts, frozenMacos: { nl: new Set(['queue.title']) } }).sections.find(
      (section) => section.name === 'keys',
    )?.text
    const titleBlock = keys?.split('\n- `queue.row.label`')[0] ?? ''
    expect(titleBlock).toContain('Frozen macOS text (nl)')
    expect(titleBlock).toContain('macosChangeReason')
    expect(keys?.match(/Frozen macOS text/g)?.length).toBe(1)
  })

  it('lists a per-OS block’s placeholders and tokens branch by branch, never the injected platform or a branch body', () => {
    const text = renderBrief(buildBrief({ ...opts, keys: ['queue.reveal', 'errors.queue.revealHint'] }))
    const notes = text.split('\n').filter((line) => line.startsWith('  - {'))
    expect(notes).toEqual(['  - {name}', '  - {system_settings}'])
    expect(text).not.toContain('{platform}')
    expect(
      text.match(/`macos` is macOS \(pile: `macOS\/`\), and macOS reads it, so a translation already there stays/g)
        ?.length,
    ).toBe(2)
    expect(
      text.match(
        /`other` is Windows and Linux \(pile: `microsoft-terminology\/`, `microsoft-style-guides\/`; `gnome-nautilus\/`/g,
      )?.length,
    ).toBe(2)
  })

  it('names which OS reads each branch a block has, so a Mac reading `other` keeps its translation there', () => {
    const text = renderBrief(buildBrief({ ...opts, keys: ['queue.trash'] }))
    expect(text).toContain('`windows` is Windows (pile: `microsoft-terminology/`, `microsoft-style-guides/`);')
    expect(text).toContain(
      '`other` is macOS and Linux (pile: `macOS/`; `gnome-nautilus/`, `kde-dolphin/`, `xfce-thunar/`), and macOS reads it, so a translation already there stays.',
    )
  })

  it('finds a concept in a raw block’s branch, where a whole-value strip would delete every body', () => {
    const text = renderBrief(buildBrief({ ...opts, keys: ['errors.queue.undoHint'] }))
    expect(text).toContain('The file work Cmdr performs.')
  })

  it('prints a ruling’s per-OS forms beside its base form', () => {
    const text = renderBrief(buildBrief(opts))
    expect(text).toMatch(/nl: \*\*bewerking\*\*.* · Windows: \*\*taak\*\* \(accept: taken\)/)
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

  it('shows what changed in English since a stale translation, as a word diff', () => {
    const keys =
      buildBrief({ ...opts, previousEnglish: { 'queue.title': { nl: 'Operations queue' } } }).sections.find(
        (section) => section.name === 'keys',
      )?.text ?? ''
    expect(keys).toContain('- English changed since nl translated it: [-Operations-] {+Operation+} queue')
    expect(keys.match(/English changed/g)).toHaveLength(1)
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

  it('exempts a search keyword list from the termbase: no concepts, no "No concept yet", and says why', () => {
    const brief = buildBrief({ ...opts, keys: ['settings.queue.keywords'] })
    const keys = brief.sections.find((section) => section.name === 'keys')?.text ?? ''
    const terms = brief.sections.find((section) => section.name === 'terms')?.text ?? ''
    expect(keys).not.toContain('No concept yet')
    expect(keys).toContain('Search keywords: hidden synonyms')
    expect(terms).toContain('No registered concept appears in this batch.')
    expect(terms).not.toContain('The file work Cmdr performs.')
  })

  it('names the overlay forks a batch touches, and how to write and stamp them against the parent', () => {
    const overlays = buildBrief(opts).sections.find((section) => section.name === 'overlays')?.text ?? ''
    expect(overlays).toContain('nl-BE')
    expect(overlays).toContain('`queue.title`')
    expect(overlays).not.toContain('`queue.row.label`')
    expect(overlays).toContain('pnpm i18n:apply --lang nl-BE')
    const de = buildBrief({ ...opts, langs: ['de'] }).sections.map((section) => section.name)
    expect(de).not.toContain('overlays')
  })

  it('warns that an `es` value with a Spain-only form needs an `es-419` fork, even when the batch forks none', () => {
    const overlays = buildBrief({ ...opts, langs: ['es'] }).sections.find((section) => section.name === 'overlays')
    expect(overlays?.text).toContain('It forks none of this batch yet.')
    expect(overlays?.text).toMatch(/Spain-only form.*`Ajustes`.*`es-419` fork.*i18n-es-overlay\.test\.ts.*fails/s)
  })

  it('points Portuguese at the Brazilian pile, which its style guide requires', () => {
    const single = buildBrief({ ...opts, langs: ['pt'] }).sections.find((section) => section.name === 'header')
    expect(single?.text).toContain('/pile/pt-BR/')
    const multi = buildBrief({ ...opts, langs: ['nl', 'pt'] }).sections.find((section) => section.name === 'header')
    expect(multi?.text).toMatch(/pt: `\/pile\/pt-BR\/`/)
  })

  it('names the other keys stale in the target locales, so an English edit outside the batch is not missed', () => {
    const header = buildBrief(opts).sections.find((section) => section.name === 'header')?.text ?? ''
    // nl carries no hashes in the fixture, so every nl key it has is stale.
    expect(header).toMatch(/stale.*`queue\.empty`.*`queue\.undo`.*--include-stale/s)
    expect(header).not.toMatch(/stale.*`queue\.title`/s)
  })

  it('routes shared termbase and queue writes to proposals in a parallel run', () => {
    const text = renderBrief(buildBrief({ ...opts, parallel: true }))
    expect(text).toContain('docs/i18n/nl/concepts-proposed.json')
    expect(text).toMatch(/Parallel run/)
    expect(renderBrief(buildBrief(opts))).not.toMatch(/Parallel run/)
  })

  it('reports per-section sizes for --stats', () => {
    const brief = buildBrief(opts)
    expect(brief.sections.map((section) => section.name)).toEqual(
      expect.arrayContaining(['header', 'instructions', 'digest', 'keys', 'terms', 'memory', 'decisions']),
    )
  })
})
