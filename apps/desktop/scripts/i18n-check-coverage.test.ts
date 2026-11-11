/**
 * Tests for the key-parity / untranslated-visibility check
 * (`i18n-check-coverage.ts`).
 *
 * Clean path: the committed pseudolocale fixture defines every English key and
 * accents every value, so nothing is missing or identical. Negative paths: drop a
 * key from the locale (→ missing) and copy an English value verbatim (→
 * identical), asserting exactly those keys are flagged.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, cpSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  EXIT_WARN,
  runCoverageCheck,
  coverageStatus,
  englishBranches,
  untranslatedPlatformBranches,
} from './i18n-check-coverage.ts'
import { sourceHash } from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ISSUES, localesToCheck } from './i18n-locale-check-lib.ts'

const FIXTURE_ROOT = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')

function capture() {
  const lines: string[] = []
  return { lines, write: (l: string) => void lines.push(l) }
}

describe('coverageStatus: pure classifier', () => {
  it('null when the locale has a distinct value', () => {
    expect(coverageStatus('a.b', 'Cancel', { 'a.b': 'Avbryt' })).toBeNull()
  })
  it('missing when the key is absent', () => {
    expect(coverageStatus('a.b', 'Cancel', {})).toBe('missing')
  })
  it('identical when the locale value equals English byte-for-byte', () => {
    expect(coverageStatus('a.b', 'Cancel', { 'a.b': 'Cancel' })).toBe('identical')
  })
  it('null (exempt) when an identical value carries a non-empty sameAsSourceJustification', () => {
    expect(
      coverageStatus('a.b', 'Dropbox', { 'a.b': 'Dropbox' }, { 'a.b': { sameAsSourceJustification: 'brand name' } }),
    ).toBeNull()
  })
  it('still identical when the justification is present but empty', () => {
    expect(coverageStatus('a.b', 'Cancel', { 'a.b': 'Cancel' }, { 'a.b': { sameAsSourceJustification: '' } })).toBe(
      'identical',
    )
  })
  it('sourceTextOnly when only the plural CATEGORIES differ and every branch reads as English', () => {
    // The blind spot this closes: Portuguese needs a `many` English lacks, so a
    // verbatim-English counter is never byte-identical and used to pass as covered.
    const en = '{countText} {count, plural, one {dir} other {dirs}}'
    const pt = '{countText} {count, plural, one {dir} many {dirs} other {dirs}}'
    expect(coverageStatus('a.b', en, { 'a.b': pt })).toBe('sourceTextOnly')
  })
  it("null once the richer plural carries the locale's own words", () => {
    const en = '{countText} {count, plural, one {dir} other {dirs}}'
    const pt = '{countText} {count, plural, one {pasta} many {pastas} other {pastas}}'
    expect(coverageStatus('a.b', en, { 'a.b': pt })).toBeNull()
  })
  it('null when the locale only respaced the text (German writes `50 %`)', () => {
    expect(coverageStatus('a.b', '{percent}%, {eta}', { 'a.b': '{percent} %, {eta}' })).toBeNull()
  })
  it('a sameAsSourceJustification exempts the plural-shaped case too', () => {
    const en = '{countText} {count, plural, one {token} other {tokens}}'
    const vi = '{countText} {count, plural, other {token}}'
    expect(coverageStatus('a.b', en, { 'a.b': vi }, { 'a.b': { sameAsSourceJustification: 'loanword' } })).toBeNull()
  })
  it('leaves the raw errors.* family a byte comparison (it never reaches the ICU engine)', () => {
    expect(coverageStatus('errors.a', "doesn't {x}", { 'errors.a': "doesn't {x}" })).toBe('identical')
    expect(coverageStatus('errors.a', "doesn't {x}", { 'errors.a': 'geht nicht {x}' })).toBeNull()
  })
  it('a justification never excuses a MISSING key', () => {
    expect(coverageStatus('a.b', 'Cancel', {}, { 'a.b': { sameAsSourceJustification: 'brand name' } })).toBe('missing')
  })
})

describe('englishBranches: a branch left in English beside its translated sibling', () => {
  const en = '{countText} {count, plural, one {dir} other {dirs}}'
  it('flags a many branch that copies English while other is translated', () => {
    const pt = '{countText} {count, plural, one {pasta} many {dirs} other {pastas}}'
    expect(englishBranches(en, pt, 'pt')).toEqual(['count: `many` repeats the English `other` branch'])
  })
  it('flags a branch that repeats another category’s English, inside a nested node too', () => {
    const enNested = '{kind, select, a {{count, plural, one {file} other {files}}} other {x}}'
    const fr = '{kind, select, a {{count, plural, one {files} other {fichiers}}} other {y}}'
    expect(englishBranches(enNested, fr, 'fr')).toEqual(['count: `one` repeats the English `other` branch'])
  })
  it('stays quiet on a loanword the language shares, and on a fully translated or fully English value', () => {
    expect(
      englishBranches('{n, plural, one {# app} other {# apps}}', '{n, plural, one {# app} other {# appar}}', 'sv'),
    ).toEqual([])
    expect(englishBranches(en, '{countText} {count, plural, one {pasta} many {pastas} other {pastas}}', 'pt')).toEqual(
      [],
    )
    expect(englishBranches(en, '{countText} {count, plural, other {commit}}', 'vi')).toEqual([])
    expect(englishBranches(en, en, 'pt')).toEqual([])
  })
})

describe('englishBranches: platform blocks', () => {
  it('compares each platform branch with its own English branch, never across OSes', () => {
    // A key stale before the migration: only the branch macOS reads carries the
    // translation, and the `windows` branch is English text that happens to equal
    // English's `macos`. That's stale's business, not a branch pasted from a sibling.
    const en = '{platform, select, macos {Show in Finder} windows {Show in Finder} other {Show in Files}}'
    const de = '{platform, select, macos {Im Finder zeigen} windows {Show in Finder} other {Show in Files}}'
    expect(englishBranches(en, de, 'de')).toEqual([])
  })

  it('still finds a plural branch left in English inside one platform branch, naming the branch', () => {
    const en =
      '{platform, select, macos {{count, plural, one {# file} other {# files}}} other {{count, plural, one {# item} other {# items}}}}'
    const pt =
      '{platform, select, macos {{count, plural, one {# arquivo} many {# files} other {# arquivos}}} other {{count, plural, one {# item} many {# itens} other {# itens}}}}'
    expect(englishBranches(en, pt, 'pt')).toEqual(['macos branch, count: `many` repeats the English `other` branch'])
  })
})

describe('untranslatedPlatformBranches: a fresh key whose platform branch is still English', () => {
  const en = '{platform, select, macos {Move to Trash} windows {Move to Recycle Bin} other {Move to trash}}'
  const fresh = { sourceHash: sourceHash(en) }

  it('flags each English branch that differs from what macOS reads', () => {
    const de = '{platform, select, macos {In den Papierkorb} windows {Move to Recycle Bin} other {Move to trash}}'
    expect(untranslatedPlatformBranches('a.b', en, de, fresh)).toEqual(['windows', 'other'])
  })

  it('is clean once every branch is translated', () => {
    const de = '{platform, select, macos {In den Papierkorb} windows {In den Papierkorb (Windows)} other {In den Müll}}'
    expect(untranslatedPlatformBranches('a.b', en, de, fresh)).toEqual([])
  })

  it('never judges a stale key (stale owns it: task 28’s worklist)', () => {
    const de = '{platform, select, macos {In den Papierkorb} windows {Move to Recycle Bin} other {Move to trash}}'
    expect(untranslatedPlatformBranches('a.b', en, de, { sourceHash: 'abcdef0' })).toEqual([])
    expect(untranslatedPlatformBranches('a.b', en, de, undefined)).toEqual([])
  })

  it('honors a sameAsSourceJustification', () => {
    const de = '{platform, select, macos {In den Papierkorb} windows {Move to Recycle Bin} other {Move to trash}}'
    expect(
      untranslatedPlatformBranches('a.b', en, de, { ...fresh, sameAsSourceJustification: 'Windows UI is English' }),
    ).toEqual([])
  })

  it('leaves a branch that equals what macOS reads to the translation of that text', () => {
    // `windows` keeps the macOS text on purpose: one translation serves both.
    const shared = '{platform, select, macos {Get Info} windows {Get Info} other {Properties}}'
    const de = '{platform, select, macos {Informationen} windows {Get Info} other {Eigenschaften}}'
    expect(untranslatedPlatformBranches('a.b', shared, de, { sourceHash: sourceHash(shared) })).toEqual(['windows'])
    const deRight = '{platform, select, macos {Informationen} windows {Informationen} other {Eigenschaften}}'
    expect(untranslatedPlatformBranches('a.b', shared, deRight, { sourceHash: sourceHash(shared) })).toEqual([])
  })

  it('reads macOS’s branch as `other` in a {windows, other} block', () => {
    const winOnly = '{platform, select, windows {Recycle Bin} other {Trash}}'
    const de = '{platform, select, windows {Recycle Bin} other {Papierkorb}}'
    expect(untranslatedPlatformBranches('a.b', winOnly, de, { sourceHash: sourceHash(winOnly) })).toEqual(['windows'])
  })

  it('sees English text under a richer plural set as untranslated, like the whole-value rule', () => {
    const counter =
      '{platform, select, macos {{n, plural, one {# tab} other {# tabs}}} other {{n, plural, one {# pane} other {# panes}}}}'
    const pt =
      '{platform, select, macos {{n, plural, one {# aba} many {# abas} other {# abas}}} other {{n, plural, one {# pane} many {# panes} other {# panes}}}}'
    expect(untranslatedPlatformBranches('a.b', counter, pt, { sourceHash: sourceHash(counter) })).toEqual(['other'])
  })

  it('judges a raw block by bytes', () => {
    const raw = '{platform, select, macos {Open {system_settings}.} other {Open Settings.}}'
    const de = '{platform, select, macos {Öffne {system_settings}.} other {Open Settings.}}'
    expect(untranslatedPlatformBranches('errors.x', raw, de, { sourceHash: sourceHash(raw) })).toEqual(['other'])
  })

  it('has nothing to say about a plain value', () => {
    expect(untranslatedPlatformBranches('a.b', 'Cancel', 'Cancel', { sourceHash: sourceHash('Cancel') })).toEqual([])
  })
})

describe('runCoverageCheck against the committed fixture', () => {
  it('is clean: en-XA has every key and every value differs from English', () => {
    const { lines, write } = capture()
    expect(runCoverageCheck({ messagesRoot: FIXTURE_ROOT, only: ['en-XA'], write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/en-XA: clean\./)
  })
})

describe('runCoverageCheck negative cases (temp catalog copies)', () => {
  let root: string
  let xaFile: string
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-coverage-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    xaFile = join(root, 'en-XA', 'fixture.json')
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  const read = (): Record<string, unknown> => JSON.parse(readFileSync(xaFile, 'utf8')) as Record<string, unknown>
  const writeXa = (obj: Record<string, unknown>) => {
    writeFileSync(xaFile, JSON.stringify(obj, null, 2) + '\n', 'utf8')
  }
  // The fixture's only translation is the pseudolocale, which a check inspects only when named.
  const run = () => {
    const cap = capture()
    const code = runCoverageCheck({ messagesRoot: root, write: cap.write, only: ['en-XA'] })
    return { code, text: cap.lines.join('\n') }
  }

  it('leaves a stale pseudolocale out of a default run, which then agrees with CI (where en-XA never exists)', () => {
    const xa = read()
    delete xa['fixture.plainLabel'] // `en` gained a key since the last `pnpm i18n:pseudo`
    writeXa(xa)
    const { lines, write } = capture()
    expect(runCoverageCheck({ messagesRoot: root, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).not.toMatch(/en-XA/)
  })

  it('flags a key missing from the locale', () => {
    const xa = read()
    delete xa['fixture.plainLabel']
    delete xa['@fixture.plainLabel']
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → missing; renders the English fallback/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags a value identical to English as possibly untranslated', () => {
    const xa = read()
    xa['fixture.plainLabel'] = 'Cancel' // verbatim English
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → identical to English; possibly untranslated/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('warns (not errors) on a plural branch left in English beside its translated sibling', () => {
    const xa = read()
    xa['fixture.fileCount'] = '{count, plural, one {# ḟíļé⟦áé⟧} few {# files} other {# ḟíļéš⟦áé⟧}}'
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_WARN)
    expect(text).toMatch(/fixture\.fileCount → count: `few` repeats the English `other` branch/)
  })

  /** Adds one block key to the temp tree: English plus the en-XA value, stamped fresh unless `hash` says otherwise. */
  const addBlockKey = (english: string, pseudo: string, hash = sourceHash(english)) => {
    const enFile = join(root, 'en', 'fixture.json')
    const enCat = JSON.parse(readFileSync(enFile, 'utf8')) as Record<string, unknown>
    enCat['fixture.revealLabel'] = english
    enCat['@fixture.revealLabel'] = { description: 'Reveal command label, per OS.' }
    writeFileSync(enFile, JSON.stringify(enCat, null, 2) + '\n', 'utf8')
    writeXa({ ...read(), 'fixture.revealLabel': pseudo, '@fixture.revealLabel': { sourceHash: hash } })
  }
  const english = '{platform, select, macos {Show in Finder} windows {Show in File Explorer} other {Show in Files}}'

  it('is clean for a migrated key in the shape English lands in: translated macOS branch, stale hash', () => {
    addBlockKey(
      english,
      '{platform, select, macos {Šḣöŵ íñ Finder} windows {Show in File Explorer} other {Show in Files}}',
      'abcdef0',
    )
    const { code, text } = run()
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/en-XA: clean\./)
  })

  it('flags a fresh key whose Windows and Linux branches are still English', () => {
    addBlockKey(
      english,
      '{platform, select, macos {Šḣöŵ íñ Finder} windows {Show in File Explorer} other {Show in Files}}',
    )
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.revealLabel → untranslated platform branch\(es\) windows, other/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('is clean for a fresh key with every branch translated', () => {
    addBlockKey(
      english,
      '{platform, select, macos {Šḣöŵ íñ Finder} windows {Šḣöŵ íñ Ḟíļé Éẋṗļöŕéŕ} other {Šḣöŵ íñ Ḟíļéš}}',
    )
    expect(run().code).toBe(EXIT_CLEAN)
  })

  it('does NOT flag an identical value that carries a sameAsSourceJustification', () => {
    const xa = read()
    xa['fixture.plainLabel'] = 'Cancel' // verbatim English, but deliberately so
    xa['@fixture.plainLabel'] = {
      ...(xa['@fixture.plainLabel'] as Record<string, unknown>),
      sameAsSourceJustification: 'brand name; do not translate',
    }
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/en-XA: clean\./)
  })
})

describe('no-locales path (only en)', () => {
  let root: string
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-coverage-only-en-'))
    mkdirSync(join(root, 'en'), { recursive: true })
    cpSync(join(FIXTURE_ROOT, 'en', 'fixture.json'), join(root, 'en', 'fixture.json'))
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('is a clean no-op', () => {
    expect(localesToCheck(root)).toEqual([])
    const { lines, write } = capture()
    expect(runCoverageCheck({ messagesRoot: root, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/no non-en locales to check/)
  })
})
