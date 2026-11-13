/**
 * Tests for the locale key sync (`sync-locale-keys.ts`).
 *
 * Each case builds a throwaway `messages/` root with a tiny `en` catalog and one
 * hand-"translated" locale, runs `syncLocale`, and reads the result back.
 *
 * The load-bearing case is the FIRST one: sync must leave a kept key's stored
 * `@key.sourceHash` alone. The hash records which English value the translation
 * was made from, and the stale check (`i18n-check-stale.ts`) is the only thing
 * that tells a translator "this locale owes you a re-translation". A sync that
 * re-stamps the hash from the current English erases that signal without anyone
 * having re-translated, which is exactly the silent-drift failure the whole
 * `sourceHash` mechanism exists to prevent.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, mkdirSync, rmSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  parseSyncArgs,
  pruneTermbaseExceptions,
  restampOverlay,
  syncLocale,
  syncableLocales,
} from './sync-locale-keys.ts'
import { sourceHash } from './i18n-catalog-lib.ts'

const AREA = 'fixture.json'

describe('syncLocale', () => {
  let root: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-sync-'))
    mkdirSync(join(root, 'en'), { recursive: true })
    mkdirSync(join(root, 'de'), { recursive: true })
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  const write = (tag: string, obj: Record<string, unknown>) => {
    writeFileSync(join(root, tag, AREA), JSON.stringify(obj, null, 2) + '\n', 'utf8')
  }
  const read = (tag: string): Record<string, unknown> =>
    JSON.parse(readFileSync(join(root, tag, AREA), 'utf8')) as Record<string, unknown>
  const raw = (tag: string): string => readFileSync(join(root, tag, AREA), 'utf8')

  describe('a kept key', () => {
    it('keeps its stored hash when the English source changed, so the stale check still fires', () => {
      write('en', { 'a.label': 'Save the file' })
      write('de', { 'a.label': 'Datei speichern', '@a.label': { sourceHash: sourceHash('Save file') } })

      syncLocale('de', { messagesRoot: root })

      const de = read('de')
      expect(de['a.label']).toBe('Datei speichern')
      // The OLD hash survives, so `stored !== sourceHash(current en)` and the key reads as stale.
      expect(de['@a.label']).toEqual({ sourceHash: sourceHash('Save file') })
      expect(de['@a.label']).not.toEqual({ sourceHash: sourceHash('Save the file') })
    })

    it('keeps every other @key field verbatim', () => {
      write('en', { 'a.label': 'Dropbox account' })
      write('de', {
        'a.label': 'Dropbox',
        '@a.label': {
          sourceHash: sourceHash('Dropbox'),
          reviewed: true,
          sameAsSourceJustification: 'brand name',
        },
      })

      syncLocale('de', { messagesRoot: root })

      expect(read('de')['@a.label']).toEqual({
        sourceHash: sourceHash('Dropbox'),
        reviewed: true,
        sameAsSourceJustification: 'brand name',
      })
    })

    it('is a no-op when the English source is unchanged', () => {
      write('en', { 'a.label': 'Save' })
      write('de', { 'a.label': 'Speichern', '@a.label': { sourceHash: sourceHash('Save') } })
      const before = raw('de')

      const result = syncLocale('de', { messagesRoot: root })

      expect(raw('de')).toBe(before)
      expect(result).toMatchObject({ added: 0, kept: 1, dropped: 0, restamped: 0 })
    })
  })

  describe('key parity with en', () => {
    it('adds a missing en key as an English skeleton with a fresh hash', () => {
      write('en', { 'a.label': 'Save', 'a.hint': 'Press Enter' })
      write('de', { 'a.label': 'Speichern', '@a.label': { sourceHash: sourceHash('Save') } })

      const result = syncLocale('de', { messagesRoot: root })

      const de = read('de')
      expect(de['a.hint']).toBe('Press Enter')
      expect(de['@a.hint']).toEqual({ sourceHash: sourceHash('Press Enter') })
      expect(result).toMatchObject({ added: 1, kept: 1, dropped: 0 })
    })

    it('drops a locale key that no longer exists in en, with its @key', () => {
      write('en', { 'a.label': 'Save' })
      write('de', {
        'a.label': 'Speichern',
        '@a.label': { sourceHash: sourceHash('Save') },
        'a.gone': 'Weg',
        '@a.gone': { sourceHash: 'deadbee' },
      })

      const result = syncLocale('de', { messagesRoot: root })

      const de = read('de')
      expect('a.gone' in de).toBe(false)
      expect('@a.gone' in de).toBe(false)
      expect(result).toMatchObject({ dropped: 1 })
    })

    it('orders keys by en source order', () => {
      write('en', { 'a.one': 'One', 'a.two': 'Two', 'a.three': 'Three' })
      write('de', { 'a.three': 'Drei', '@a.three': { sourceHash: sourceHash('Three') } })

      syncLocale('de', { messagesRoot: root })

      expect(Object.keys(read('de')).filter((k) => !k.startsWith('@'))).toEqual(['a.one', 'a.two', 'a.three'])
    })

    it('is idempotent: a second run is a byte-identical no-op', () => {
      write('en', { 'a.label': 'Save', 'a.hint': 'Press Enter' })
      write('de', { 'a.label': 'Speichern', '@a.label': { sourceHash: sourceHash('Save changed') } })

      syncLocale('de', { messagesRoot: root })
      const after = raw('de')
      syncLocale('de', { messagesRoot: root })

      expect(raw('de')).toBe(after)
    })
  })

  describe('--restamp: the deliberate "translation is still right" refresh', () => {
    it('refreshes only the named key, and only where the hash was actually stale', () => {
      write('en', { 'a.label': 'May be larger', 'a.other': 'Changed too' })
      write('de', {
        'a.label': 'Kann größer sein',
        '@a.label': { sourceHash: sourceHash('Is larger') },
        'a.other': 'Auch geändert',
        '@a.other': { sourceHash: sourceHash('Something else') },
      })

      const result = syncLocale('de', { messagesRoot: root, restampKeys: ['a.label'] })

      const de = read('de')
      expect(de['@a.label']).toEqual({ sourceHash: sourceHash('May be larger') })
      // Untouched: it wasn't named, so it stays stale and keeps warning.
      expect(de['@a.other']).toEqual({ sourceHash: sourceHash('Something else') })
      expect(result.restamped).toBe(1)
      expect(result.restampedKeys).toEqual(['a.label'])
      // The value itself is never rewritten by a restamp.
      expect(de['a.label']).toBe('Kann größer sein')
    })

    it('drops reviewed and sameAsSourceJustification, which vouched for the OLD English', () => {
      write('en', { 'a.label': 'May be larger' })
      write('de', {
        'a.label': 'Kann größer sein',
        '@a.label': {
          sourceHash: sourceHash('Is larger'),
          description: 'kept',
          reviewed: true,
          sameAsSourceJustification: 'brand name',
        },
      })

      syncLocale('de', { messagesRoot: root, restampKeys: ['a.label'] })

      expect(read('de')['@a.label']).toEqual({ sourceHash: sourceHash('May be larger'), description: 'kept' })
    })

    it('reports nothing restamped for a key that is already fresh or misspelled', () => {
      write('en', { 'a.label': 'Save' })
      write('de', { 'a.label': 'Speichern', '@a.label': { sourceHash: sourceHash('Save') } })

      const result = syncLocale('de', { messagesRoot: root, restampKeys: ['a.label', 'a.typo'] })

      expect(result.restamped).toBe(0)
      expect(result.restampedKeys).toEqual([])
    })
  })

  it('refuses to sync the source locale', () => {
    expect(() => syncLocale('en', { messagesRoot: root })).toThrow(/source locale/)
  })

  it('creates a locale area file that only en has', () => {
    write('en', { 'a.label': 'Save' })
    rmSync(join(root, 'de'), { recursive: true, force: true })
    mkdirSync(join(root, 'de'), { recursive: true })

    syncLocale('de', { messagesRoot: root })

    expect(read('de')).toEqual({ 'a.label': 'Save', '@a.label': { sourceHash: sourceHash('Save') } })
  })
})

describe('parseSyncArgs', () => {
  it('reads positional tags', () => {
    expect(parseSyncArgs(['de', 'hu'])).toMatchObject({ tags: ['de', 'hu'] })
  })

  it('reads --messages-root without swallowing it as a tag', () => {
    expect(parseSyncArgs(['--messages-root', '/tmp/x', 'de'])).toMatchObject({
      messagesRoot: '/tmp/x',
      tags: ['de'],
    })
  })

  it('collects repeated --restamp values and keeps them out of the tag list', () => {
    expect(parseSyncArgs(['--restamp', 'a.one', '--restamp', 'a.two', 'de'])).toMatchObject({
      restampKeys: ['a.one', 'a.two'],
      tags: ['de'],
    })
  })

  it('reads --restamp-file lists (one key per line, # comments) and merges them with --restamp', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-i18n-restamp-file-'))
    const list = join(dir, 'keys.txt')
    writeFileSync(list, '# batch\na.two\n\na.three\n', 'utf8')
    expect(parseSyncArgs(['--restamp', 'a.one', '--restamp', 'a.two', '--restamp-file', list, 'de'])).toMatchObject({
      restampKeys: ['a.one', 'a.two', 'a.three'],
      tags: ['de'],
    })
    rmSync(dir, { recursive: true, force: true })
  })

  it('defaults to no tags, no root, and no restamps', () => {
    expect(parseSyncArgs([])).toEqual({ tags: [], messagesRoot: undefined, restampKeys: [] })
  })
})

describe('syncableLocales', () => {
  const available = ['de', 'en', 'en-GB', 'en-XA', 'pt', 'pt-PT']
  const runWith = (requested: string[]) => {
    const notes: string[] = []
    const { tags, overlays } = syncableLocales({ requested, available, note: (line) => void notes.push(line) })
    return { tags, overlays, notes }
  }

  it('sweeps every full translation and skips the overlays', () => {
    const { tags, overlays, notes } = runWith([])
    expect(tags).toEqual(['de', 'en-XA', 'pt'])
    expect(overlays).toEqual(['en-GB', 'pt-PT'])
    expect(notes).toHaveLength(2)
    expect(notes.join('\n')).toMatch(/Skipped en-GB\/: it's an overlay/)
  })

  it('skips an overlay even when it was asked for by name', () => {
    // Syncing `pt-PT` would clone every `en` key into a catalog that must carry
    // only its forks, and coverage would then flag every one of them.
    const { tags, overlays, notes } = runWith(['pt-PT', 'de'])
    expect(tags).toEqual(['de'])
    expect(overlays).toEqual(['pt-PT'])
    expect(notes.join('\n')).toMatch(/Skipped pt-PT\//)
  })

  it('refuses a tag that is no locale, naming --restamp when it looks like keys', () => {
    // A quoted `"a.b --restamp c.d"` arrives as one positional; it used to reach the
    // file system as a locale dir and die on ENAMETOOLONG.
    expect(() => runWith(['de', 'a.label --restamp b.label'])).toThrow(/Not a locale.*--restamp/s)
    expect(() => runWith(['xx'])).toThrow(/Not a locale: xx/)
  })
})

describe('restampOverlay', () => {
  let root: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-overlay-'))
    for (const tag of ['en', 'es', 'es-419']) mkdirSync(join(root, tag), { recursive: true })
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  const write = (tag: string, obj: Record<string, unknown>) => {
    writeFileSync(join(root, tag, AREA), JSON.stringify(obj, null, 2) + '\n', 'utf8')
  }
  const read = (tag: string): Record<string, unknown> =>
    JSON.parse(readFileSync(join(root, tag, AREA), 'utf8')) as Record<string, unknown>

  it('stamps a fork with the hash of the PARENT value it overrides, never adding or dropping keys', () => {
    write('en', { 'a.label': 'Add', 'a.other': 'Settings' })
    write('es', { 'a.label': 'Añadir otra vez', 'a.other': 'Ajustes' })
    write('es-419', {
      'a.label': 'Agregar otra vez',
      '@a.label': { sourceHash: sourceHash('Añadir'), reviewed: true },
    })

    const result = restampOverlay('es-419', { messagesRoot: root, restampKeys: ['a.label', 'a.other'] })

    expect(read('es-419')).toEqual({
      'a.label': 'Agregar otra vez',
      '@a.label': { sourceHash: sourceHash('Añadir otra vez') },
    })
    expect(result.restampedKeys).toEqual(['a.label'])
  })

  it('falls back to English for a key the parent lacks, the way the runtime resolves it', () => {
    write('en', { 'a.label': 'Add' })
    write('es', {})
    write('es-419', { 'a.label': 'Agregar', '@a.label': { sourceHash: 'deadbee' } })

    restampOverlay('es-419', { messagesRoot: root, restampKeys: ['a.label'] })

    expect(read('es-419')['@a.label']).toEqual({ sourceHash: sourceHash('Add') })
  })
})

describe('pruneTermbaseExceptions', () => {
  let root: string
  let messagesRoot: string
  let docsRoot: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-prune-'))
    messagesRoot = join(root, 'messages')
    docsRoot = join(root, 'docs')
    mkdirSync(join(messagesRoot, 'en'), { recursive: true })
    mkdirSync(join(docsRoot, 'de'), { recursive: true })
    writeFileSync(join(messagesRoot, 'en', AREA), JSON.stringify({ 'a.kept': 'Undo', 'b.kept': 'Archive' }), 'utf8')
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  const termsFile = () => join(docsRoot, 'de', 'terms.json')

  it('drops exceptions naming keys deleted from en, and an emptied exceptions map with them', () => {
    writeFileSync(
      termsFile(),
      JSON.stringify({
        undo: { chosen: 'rückgängig', exceptions: { 'a.kept': 'why', 'a.gone': 'retired pill' } },
        archive: { chosen: 'Archiv', exceptions: { 'b.gone': 'retired' } },
      }),
      'utf8',
    )

    const { dropped, changed } = pruneTermbaseExceptions('de', { messagesRoot, docsRoot })

    expect(changed).toBe(true)
    expect(dropped).toEqual(['archive: b.gone', 'undo: a.gone'])
    const terms = JSON.parse(readFileSync(termsFile(), 'utf8')) as Record<string, Record<string, unknown>>
    expect(terms.undo.exceptions).toEqual({ 'a.kept': 'why' })
    expect(terms.archive).toEqual({ chosen: 'Archiv' })
  })

  it('leaves a termbase whose exceptions all exist byte-identical, and tolerates a locale without one', () => {
    const text = '{\n  "undo": { "chosen": "rückgängig", "exceptions": { "a.kept": "why" } }\n}\n'
    writeFileSync(termsFile(), text, 'utf8')

    expect(pruneTermbaseExceptions('de', { messagesRoot, docsRoot })).toEqual({ dropped: [], changed: false })
    expect(readFileSync(termsFile(), 'utf8')).toBe(text)
    expect(pruneTermbaseExceptions('fr', { messagesRoot, docsRoot })).toEqual({ dropped: [], changed: false })
  })
})
