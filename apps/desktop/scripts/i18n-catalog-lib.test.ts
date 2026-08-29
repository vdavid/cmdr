/**
 * Tests for the shared i18n catalog/ICU helper (`i18n-catalog-lib.ts`): the
 * pure cores consumed by the pseudolocale generator and the locale checks.
 *
 * Covered:
 *  - catalog split/merge (messages vs `@key` metadata),
 *  - ICU AST extraction (placeholders, `<tag>`s, plural/select categories) on
 *    representative shapes: plain, `{name}`, `<tag>`, `plural`, `select`, nested,
 *  - literal-text extraction (`visibleLiterals`), the identifiers-stripped mirror
 *    of that walk,
 *  - invalid-ICU detection (`ok: false`),
 *  - `sourceHash` determinism + 7-char hex shape,
 *  - locale classification (`resolveLocaleSource`) and catalog layering, the
 *    overlay vocabulary every i18n tool reads.
 *
 * All inputs are in-memory; no real catalog or filesystem is touched (the I/O
 * wrappers `loadCatalog`/`listLocales` are thin `node:fs` over these pure cores
 * and are exercised by the smoke run + the downstream check tests).
 */
import { describe, it, expect } from 'vitest'
import {
  splitCatalogFile,
  mergeCatalogFiles,
  parseMessage,
  visibleLiterals,
  showsOnlySourceText,
  sourceHash,
  isMetadataKey,
  isRawKey,
  rawTokens,
  hasWholeWord,
  hasBrandPresent,
  resolveLocaleSource,
  layerCatalogs,
} from './i18n-catalog-lib.ts'

describe('isMetadataKey', () => {
  it('flags @-prefixed keys only', () => {
    expect(isMetadataKey('@common.ok')).toBe(true)
    expect(isMetadataKey('common.ok')).toBe(false)
  })
})

describe('splitCatalogFile', () => {
  it('separates string messages from @key metadata objects', () => {
    const raw = {
      'common.ok': 'OK',
      '@common.ok': { description: 'Confirm button', placeholders: {} },
      'common.cancel': 'Cancel',
    }
    const { messages, metadata } = splitCatalogFile(raw)
    expect(messages).toEqual({ 'common.ok': 'OK', 'common.cancel': 'Cancel' })
    // Metadata is keyed WITHOUT the leading @, to line up with its message key.
    expect(metadata).toEqual({ 'common.ok': { description: 'Confirm button', placeholders: {} } })
  })

  it('ignores non-object @entries and non-string message values', () => {
    const raw = { 'a.b': 'msg', '@a.b': 'not-an-object', 'a.c': 42 }
    const { messages, metadata } = splitCatalogFile(raw)
    expect(messages).toEqual({ 'a.b': 'msg' })
    expect(metadata).toEqual({})
  })
})

describe('mergeCatalogFiles', () => {
  it('merges messages and metadata across area files', () => {
    const files = {
      'common.json': { 'common.ok': 'OK', '@common.ok': { description: 'm' } },
      'transfer.json': { 'transfer.trash': 'Trashed', 'transfer.delete': 'Deleted' },
    }
    const { messages, metadata } = mergeCatalogFiles(files)
    expect(messages).toEqual({ 'common.ok': 'OK', 'transfer.trash': 'Trashed', 'transfer.delete': 'Deleted' })
    expect(metadata).toEqual({ 'common.ok': { description: 'm' } })
  })
})

describe('parseMessage', () => {
  /**
   * Convenience: parse and return plain arrays/objects for easy assertion.
   */
  const parsed = (value: string) => {
    const r = parseMessage(value)
    return {
      ok: r.ok,
      placeholders: [...r.placeholders].sort(),
      tags: [...r.tags].sort(),
      pluralCategories: Object.fromEntries([...r.pluralCategories].map(([k, v]) => [k, [...v].sort()])),
      selectCategories: Object.fromEntries([...r.selectCategories].map(([k, v]) => [k, [...v].sort()])),
    }
  }

  it('plain message has no structure', () => {
    expect(parsed('Just text')).toEqual({
      ok: true,
      placeholders: [],
      tags: [],
      pluralCategories: {},
      selectCategories: {},
    })
  })

  it('extracts a simple {name} placeholder', () => {
    expect(parsed('Hello {name}, welcome')).toEqual({
      ok: true,
      placeholders: ['name'],
      tags: [],
      pluralCategories: {},
      selectCategories: {},
    })
  })

  it('extracts multiple placeholders', () => {
    expect(parsed('{a} and {b} and {a}').placeholders).toEqual(['a', 'b'])
  })

  it('extracts <tag> names and walks their children', () => {
    expect(parsed('Click <link>{label}</link> now')).toEqual({
      ok: true,
      placeholders: ['label'],
      tags: ['link'],
      pluralCategories: {},
      selectCategories: {},
    })
  })

  it('extracts plural categories into pluralCategories (not selectCategories)', () => {
    expect(parsed('{count, plural, one {# file} other {# files}}')).toEqual({
      ok: true,
      placeholders: ['count'],
      tags: [],
      pluralCategories: { count: ['one', 'other'] },
      selectCategories: {},
    })
  })

  it('extracts select categories into selectCategories (not pluralCategories)', () => {
    expect(parsed('{kind, select, dir {Folder} file {File} other {Item}}')).toEqual({
      ok: true,
      placeholders: ['kind'],
      tags: [],
      pluralCategories: {},
      selectCategories: { kind: ['dir', 'file', 'other'] },
    })
  })

  it('handles nested select wrapping plural with inner placeholders, keeping the maps separate', () => {
    const msg =
      '{kind, select, ' +
      'copy {Copied {countText} {count, plural, one {file} other {files}}} ' +
      'other {Moved {countText} {count, plural, one {file} other {files}}}}'
    expect(parsed(msg)).toEqual({
      ok: true,
      placeholders: ['count', 'countText', 'kind'],
      tags: [],
      pluralCategories: { count: ['one', 'other'] },
      selectCategories: { kind: ['copy', 'other'] },
    })
  })

  it('treats number/date placeholders as placeholders', () => {
    expect(parsed('{n, number} on {when, date}').placeholders).toEqual(['n', 'when'])
  })

  it('flags invalid ICU as ok:false with an error and empty sets', () => {
    const r = parseMessage('Unclosed {arg')
    expect(r.ok).toBe(false)
    expect(typeof r.error).toBe('string')
    expect(r.error ?? '').not.toBe('')
    expect([...r.placeholders]).toEqual([])
  })

  it('flags a stray unescaped < (parsed as an unclosed tag) as invalid', () => {
    expect(parseMessage('Size <dir>').ok).toBe(false)
  })
})

describe('visibleLiterals', () => {
  it('returns a plain message unchanged', () => {
    expect(visibleLiterals('Move to Bin')).toBe('Move to Bin')
  })

  it('drops placeholder NAMES but keeps the copy around them', () => {
    // The trap this exists for: tintTriggerAria's only "color" is `{colorName}`,
    // an identifier, so a "color" sweep must not see it.
    expect(visibleLiterals('{label} (currently: {colorName})')).not.toMatch(/color/i)
    expect(visibleLiterals('{label} (currently: {colorName})')).toContain('(currently: ')
  })

  it('drops select/plural category LABELS but keeps each branch body', () => {
    const literals = visibleLiterals('{k, select, trash {Moving to trash} other {Working}}') ?? ''
    // `trash {` is the selector; `Moving to trash` is copy. One survives, one does not.
    expect(literals).toContain('Moving to trash')
    expect(literals).toContain('Working')
    expect(literals.match(/trash/g)).toHaveLength(1)
  })

  it('drops <tag> NAMES but keeps their children', () => {
    expect(visibleLiterals('Read the <colorNote>tint guide</colorNote>.')).toBe('Read the  tint guide .')
  })

  it('returns undefined on invalid ICU, so callers fall back explicitly', () => {
    expect(visibleLiterals('Unclosed {arg')).toBeUndefined()
  })
})

describe('showsOnlySourceText', () => {
  const EN_TOKENS = '{countText} {count, plural, one {token} other {tokens}}'

  it('sees through a plural category English does not have', () => {
    // Portuguese needs `many`; filling it with English text leaves the reader
    // English, even though the branch SET is right and the bytes differ.
    expect(
      showsOnlySourceText(EN_TOKENS, '{countText} {count, plural, one {token} many {tokens} other {tokens}}'),
    ).toBe(true)
  })

  it('sees through a locale that COLLAPSES English branches without translating them', () => {
    expect(showsOnlySourceText(EN_TOKENS, '{countText} {count, plural, other {token}}')).toBe(true)
  })

  it("is false once a single branch carries the locale's own word", () => {
    expect(
      showsOnlySourceText(EN_TOKENS, '{countText} {count, plural, one {token} many {fichas} other {fichas}}'),
    ).toBe(false)
  })

  it('is false when the text AROUND the plural changed', () => {
    // German spaces its percent sign off the number. That's a translation.
    expect(showsOnlySourceText('{percent}%, {eta}', '{percent} %, {eta}')).toBe(false)
  })

  it('is false when the locale reorders the placeholders', () => {
    expect(showsOnlySourceText('{a} of {b}', '{b} of {a}')).toBe(false)
  })

  it('is true for a byte-identical message, plural or not', () => {
    expect(showsOnlySourceText('Cancel', 'Cancel')).toBe(true)
    expect(showsOnlySourceText(EN_TOKENS, EN_TOKENS)).toBe(true)
  })

  it('compares <tag> children too', () => {
    expect(showsOnlySourceText('Read the <b>guide</b>.', 'Read the <b>guide</b>.')).toBe(true)
    expect(showsOnlySourceText('Read the <b>guide</b>.', 'Lies den <b>Leitfaden</b>.')).toBe(false)
  })

  it('falls back to a byte comparison when either side is not parseable ICU', () => {
    expect(showsOnlySourceText('Unclosed {arg', 'Unclosed {arg')).toBe(true)
    expect(showsOnlySourceText('Unclosed {arg', 'Nicht geschlossen {arg')).toBe(false)
  })
})

describe('sourceHash', () => {
  it('is deterministic for the same input', () => {
    expect(sourceHash('Hello {name}')).toBe(sourceHash('Hello {name}'))
  })

  it('is 7 lowercase hex chars', () => {
    expect(sourceHash('anything at all')).toMatch(/^[0-9a-f]{7}$/)
  })

  it('changes when the value changes (even by one byte)', () => {
    expect(sourceHash('Cancel')).not.toBe(sourceHash('Cancel.'))
  })

  it('matches a known sha256-prefix value (pins the algorithm)', () => {
    // First 7 hex of sha256("Cancel").
    expect(sourceHash('Cancel')).toBe('19766ed')
  })
})

describe('isRawKey', () => {
  it('flags the errors.* family as raw (resolved via getMessage, no ICU)', () => {
    expect(isRawKey('errors.listing.notFound.suggestion')).toBe(true)
    expect(isRawKey('errors.git.dirty.title')).toBe(true)
  })

  it('treats every non-errors key as ICU', () => {
    expect(isRawKey('common.ok')).toBe(false)
    expect(isRawKey('transfer.summary')).toBe(false)
  })
})

describe('rawTokens', () => {
  it('extracts brace-token names from a raw message', () => {
    expect([...rawTokens('Open {system_settings}, then run `lsof <folder-path>`.')].sort()).toEqual(['system_settings'])
  })

  it('extracts multiple distinct tokens and ignores literal <…>', () => {
    expect([...rawTokens('{a} then {b}, see <x> and {a}')].sort()).toEqual(['a', 'b'])
  })

  it('returns an empty set when there are no tokens', () => {
    expect([...rawTokens('No tokens here, just `code` and <literal>')]).toEqual([])
  })
})

describe('hasWholeWord', () => {
  it('matches the bare word but not a substring or compound', () => {
    expect(hasWholeWord('Built for macOS.', 'macOS')).toBe(true)
    expect(hasWholeWord('Runs on macOSes', 'macOS')).toBe(false)
    expect(hasWholeWord('See Cmdrs', 'Cmdr')).toBe(false)
  })
})

describe('hasBrandPresent (suffix-aware locale-side test)', () => {
  it('matches the bare brand', () => {
    expect(hasBrandPresent('Megnyitás Cmdr', 'Cmdr')).toBe(true)
  })

  it('matches a brand with a lowercase inflectional suffix (incl. accented)', () => {
    expect(hasBrandPresent('Megnyitás Cmdrben', 'Cmdr')).toBe(true) // Hungarian inessive
    expect(hasBrandPresent('Cmdrs fönster', 'Cmdr')).toBe(true) // Swedish genitive
    expect(hasBrandPresent('A Cmdrről', 'Cmdr')).toBe(true) // Hungarian delative, accented
  })

  it('does NOT match an embedded or uppercase-compounded brand', () => {
    expect(hasBrandPresent('Open in MacCmdr', 'Cmdr')).toBe(false) // letter before
    expect(hasBrandPresent('Open in CmdrFoo', 'Cmdr')).toBe(false) // uppercase compound
  })

  it('does NOT match when the brand is absent', () => {
    expect(hasBrandPresent('Megnyitás a fájlkezelőben', 'Cmdr')).toBe(false)
  })
})

describe('resolveLocaleSource', () => {
  const shipped = ['de', 'en', 'en-GB', 'en-XA', 'pt', 'pt-BR', 'pt-PT', 'zh', 'zh-Hant-TW']

  it('treats a language-base catalog as a full translation of en', () => {
    expect(resolveLocaleSource('de', shipped)).toEqual({ overrides: 'en', isOverlay: false })
  })

  it('treats a variant whose base language ships as an overlay of that base', () => {
    expect(resolveLocaleSource('pt-PT', shipped)).toEqual({ overrides: 'pt', isOverlay: true })
    expect(resolveLocaleSource('pt-BR', shipped)).toEqual({ overrides: 'pt', isOverlay: true })
  })

  it('treats an en variant as an overlay of en', () => {
    expect(resolveLocaleSource('en-GB', shipped)).toEqual({ overrides: 'en', isOverlay: true })
  })

  it('treats a variant whose base language does NOT ship as a full translation', () => {
    expect(resolveLocaleSource('fr-CA', shipped)).toEqual({ overrides: 'en', isOverlay: false })
  })

  it('never treats the generated pseudolocale as an overlay', () => {
    expect(resolveLocaleSource('en-XA', shipped)).toEqual({ overrides: 'en', isOverlay: false })
  })

  // A script boundary is a wall, not a papercut: a catalog a reader can't read is
  // never something to inherit from. Same rule as the Rust resolver's guard; see
  // `apps/desktop/src-tauri/src/intl/DETAILS.md`
  // § The script guard, and why regional fallback survives it.
  it('never treats a different-script variant as an overlay of its language base', () => {
    // `zh` is Simplified. A Traditional catalog forks NOTHING from it: it's a
    // full translation, and its missing keys must fall back to English.
    expect(resolveLocaleSource('zh-Hant', shipped)).toEqual({ overrides: 'en', isOverlay: false })
  })

  it('reads the script off the REGION when the tag names no script', () => {
    // CLDR: zh-TW is Traditional, so `zh` (Simplified) is still a wall.
    expect(resolveLocaleSource('zh-TW', shipped)).toEqual({ overrides: 'en', isOverlay: false })
  })

  it('keeps a same-script variant an overlay', () => {
    expect(resolveLocaleSource('zh-CN', shipped)).toEqual({ overrides: 'zh', isOverlay: true })
  })

  it('overlays the nearest SAME-SCRIPT ancestor, not the language base', () => {
    // With a Traditional catalog shipped, `zh-Hant-TW` forks it; `zh` stays out
    // of reach, mirroring the runtime chain.
    expect(resolveLocaleSource('zh-Hant-TW', [...shipped, 'zh-Hant'])).toEqual({
      overrides: 'zh-Hant',
      isOverlay: true,
    })
  })
})

describe('layerCatalogs', () => {
  const en = { messages: { a: 'A', b: 'B' }, metadata: { a: { description: 'first' } } }
  const pt = { messages: { a: 'Á' }, metadata: { a: { sourceHash: '1234567' } } }

  it('lets the more specific catalog win, key by key', () => {
    expect(layerCatalogs(en, pt).messages).toEqual({ a: 'Á', b: 'B' })
  })

  it('layers metadata the same way', () => {
    expect(layerCatalogs(en, pt).metadata).toEqual({ a: { sourceHash: '1234567' } })
  })

  it('leaves the inputs untouched', () => {
    layerCatalogs(en, pt)
    expect(en.messages).toEqual({ a: 'A', b: 'B' })
  })
})
