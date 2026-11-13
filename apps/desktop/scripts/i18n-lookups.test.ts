/**
 * Tests for the translator lookups: `pnpm i18n:pile` (`i18n-pile-lib.ts`,
 * `i18n-pile.ts`) over a tiny fixture pile, and `pnpm i18n:term` (`i18n-term.ts`).
 *
 * The load-bearing promises: macOS is joined by KEY (an English-valued Siri file
 * in the target folder is never evidence), every Microsoft sense prints with its
 * definition, gettext mnemonics don't hide a match, and `--key` tells a concept
 * the English hits from one a `notMatch` keeps out.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { lookUp, normalizeLabel, parsePo, parseTbx, presentFolders } from './i18n-pile-lib.ts'
import { parsePileArgs } from './i18n-pile.ts'
import { conceptsForWord, keyVerdicts, parseTermArgs, senseBlock } from './i18n-term.ts'
import type { Concepts } from './i18n-termbase-lib.ts'

let pile: string

const TBX = `<martif><body>
      <termEntry id="1">
        <langSet xml:lang="en-US">
          <descripGrp><descrip type="definition">The temporary storage place on Windows for deleted files.</descrip></descripGrp>
          <ntig><termGrp><term id="1">Recycle Bin</term></termGrp></ntig>
        </langSet>
        <langSet xml:lang="de">
          <ntig><termGrp><term id="2">Papierkorb</term><termNote type="geographicalUsage">DEU</termNote></termGrp></ntig>
        </langSet>
      </termEntry>
      <termEntry id="2">
        <langSet xml:lang="en-US">
          <descripGrp><descrip type="definition">A SharePoint container &amp; its items.</descrip></descripGrp>
          <ntig><termGrp><term id="3">second-stage Recycle Bin</term></termGrp></ntig>
        </langSet>
        <langSet xml:lang="de">
          <ntig><termGrp><term id="4">endgültiger Papierkorb</term></termGrp></ntig>
        </langSet>
      </termEntry>
</body></martif>`

const PO = `msgid ""
msgstr ""
"Language: de\\n"

msgctxt "shortcuts dialog"
msgid "Mo_ve to Trash"
msgstr "In den _Papierkorb verschieben"

msgid ""
"Empty "
"Trash"
msgstr "Papierkorb leeren"

msgid "Untranslated"
msgstr ""
`

beforeAll(() => {
  pile = mkdtempSync(join(tmpdir(), 'cmdr-i18n-pile-'))
  mkdirSync(join(pile, 'en', 'macOS', 'Finder'), { recursive: true })
  mkdirSync(join(pile, 'de', 'macOS', 'Finder'), { recursive: true })
  mkdirSync(join(pile, 'de', 'microsoft-terminology'), { recursive: true })
  mkdirSync(join(pile, 'de', 'gnome-nautilus'), { recursive: true })
  writeFileSync(
    join(pile, 'en', 'macOS', 'Finder', 'Localizable.json'),
    JSON.stringify({ N1: 'Move to Trash', N2: 'Empty Trash…', S1: 'Show Trash' }),
  )
  // S1 is still English in the target: a Siri-intent file, not evidence.
  writeFileSync(
    join(pile, 'de', 'macOS', 'Finder', 'Localizable.json'),
    JSON.stringify({ N1: 'In den Papierkorb legen', N2: 'Papierkorb entleeren …', S1: 'Show Trash' }),
  )
  writeFileSync(join(pile, 'de', 'microsoft-terminology', 'GERMAN.tbx'), TBX)
  writeFileSync(join(pile, 'de', 'gnome-nautilus', 'nautilus.po'), PO)
})

afterAll(() => {
  rmSync(pile, { recursive: true, force: true })
})

describe('the pile parsers', () => {
  it('reads every TBX sense with its definition and region, entities decoded', () => {
    const entries = parseTbx(TBX)
    expect(entries).toHaveLength(2)
    expect(entries[0]).toEqual({
      definition: 'The temporary storage place on Windows for deleted files.',
      english: ['Recycle Bin'],
      targets: [{ term: 'Papierkorb', region: 'DEU' }],
    })
    expect(entries[1].definition).toBe('A SharePoint container & its items.')
  })

  it('reads gettext entries with context and continuation lines, dropping the header and untranslated ones', () => {
    expect(parsePo(PO)).toEqual([
      { context: 'shortcuts dialog', msgid: 'Mo_ve to Trash', msgstr: 'In den _Papierkorb verschieben' },
      { context: undefined, msgid: 'Empty Trash', msgstr: 'Papierkorb leeren' },
    ])
  })

  it('compares labels without mnemonics, a trailing ellipsis, or case', () => {
    expect(normalizeLabel('Mo_ve to &Trash…')).toBe('move to trash')
  })
})

describe('lookUp', () => {
  it('joins macOS by key, skips English-valued entries, and puts exact matches first', () => {
    const hits = lookUp({ pileRootDir: pile, folders: ['de'], query: { side: 'english', text: 'Trash' } })
    const macos = hits.filter((hit) => hit.source === 'macos')
    expect(macos.map((hit) => hit.target)).toEqual(['In den Papierkorb legen', 'Papierkorb entleeren …'])
    expect(macos.every((hit) => !hit.exact)).toBe(true)
    const exact = lookUp({ pileRootDir: pile, folders: ['de'], query: { side: 'english', text: 'Empty Trash' } })
    expect(exact.filter((hit) => hit.exact).map((hit) => hit.source)).toEqual(['macos', 'linux'])
  })

  it('prints every Microsoft sense, exact first', () => {
    const hits = lookUp({
      pileRootDir: pile,
      folders: ['de'],
      query: { side: 'english', text: 'recycle bin' },
      sources: ['microsoft'],
    })
    expect(hits.map((hit) => [hit.exact, hit.target])).toEqual([
      [true, 'Papierkorb (DEU)'],
      [false, 'endgültiger Papierkorb'],
    ])
  })

  it('looks up in reverse, by the translation', () => {
    const hits = lookUp({
      pileRootDir: pile,
      folders: ['de'],
      query: { side: 'target', text: 'Papierkorb leeren' },
      sources: ['linux'],
    })
    expect(hits.map((hit) => hit.english)).toEqual(['Empty Trash'])
  })

  it('reports a pile folder that is missing', () => {
    expect(presentFolders(pile, ['de', 'zh-TW'])).toEqual({ present: ['de'], missing: ['zh-TW'] })
  })

  it('parses the CLI flags', () => {
    expect(parsePileArgs(['--lang', 'de', '--term', 'Trash', '--target', 'Papierkorb'])).toMatchObject({
      lang: 'de',
      queries: [
        { side: 'english', text: 'Trash' },
        { side: 'target', text: 'Papierkorb' },
      ],
      limit: 6,
    })
    expect(() => parsePileArgs(['--lang', 'de'])).toThrow('--term')
    expect(() => parsePileArgs(['--lang', 'de', '--term', 'x', '--source', 'finder'])).toThrow('Unknown source')
    expect(() => parsePileArgs(['--lang', 'de', 'Move', 'to'])).toThrow('Quote a phrase')
  })
})

describe('pnpm i18n:term', () => {
  const concepts: Concepts = {
    window: { en: 'window', match: ['window', 'windows'], notMatch: ['taken by windows'], sense: 'An app window.' },
    edit: { en: 'edit', match: ['edit', 'edits'], sense: 'Editing a file.', distinct: ['text-editor'] },
    'text-editor': { en: 'text editor', match: ['text editor'], sense: 'The app that edits text.' },
  }

  it('finds every sense a word names: by ID, by match form, or by headword', () => {
    expect(conceptsForWord(concepts, 'edit')).toEqual(['edit'])
    expect(conceptsForWord(concepts, 'windows')).toEqual(['window'])
    expect(conceptsForWord(concepts, 'editor')).toEqual(['text-editor'])
  })

  it('tells a concept the English hits from one a notMatch keeps out', () => {
    expect(keyVerdicts(concepts, 'a.b', 'Usually taken by Windows.')).toEqual({ hit: [], excluded: ['window'] })
    expect(keyVerdicts(concepts, 'a.b', 'Close the window.')).toEqual({ hit: ['window'], excluded: [] })
  })

  it('prints a sense with its neighbors and each language ruling', () => {
    const lines = senseBlock({
      id: 'edit',
      concepts,
      termbases: new Map([
        ['de', { edit: { chosen: 'bearbeiten', confidence: 'high', sources: 'Finder' } }],
        ['fr', undefined],
      ]),
    })
    expect(lines).toContain('Distinct from text-editor: The app that edits text.')
    expect(lines).toContain('- de: **bearbeiten** (high)')
    expect(lines).toContain('- fr: no ruling')
  })

  it('parses a word or --key, never both', () => {
    expect(parseTermArgs(['move', '--lang', 'de,fr'])).toMatchObject({ word: 'move', langs: ['de', 'fr'] })
    expect(parseTermArgs(['--key', 'a.b'])).toMatchObject({ key: 'a.b', langs: undefined })
    expect(() => parseTermArgs([])).toThrow('Pass a word')
    expect(() => parseTermArgs(['move', '--key', 'a.b'])).toThrow('Pass a word')
  })
})
