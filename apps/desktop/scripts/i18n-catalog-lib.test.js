/**
 * Tests for the shared i18n catalog/ICU helper (`i18n-catalog-lib.js`): the
 * pure cores consumed by the pseudolocale generator and the locale checks.
 *
 * Covered:
 *  - catalog split/merge (messages vs `@key` metadata),
 *  - ICU AST extraction (placeholders, `<tag>`s, plural/select categories) on
 *    representative shapes: plain, `{name}`, `<tag>`, `plural`, `select`, nested,
 *  - invalid-ICU detection (`ok: false`),
 *  - `sourceHash` determinism + 7-char hex shape.
 *
 * All inputs are in-memory; no real catalog or filesystem is touched (the I/O
 * wrappers `loadCatalog`/`listLocales` are thin `node:fs` over these pure cores
 * and are exercised by the smoke run + the downstream check tests).
 */
import { describe, it, expect } from 'vitest'
import { splitCatalogFile, mergeCatalogFiles, parseMessage, sourceHash, isMetadataKey } from './i18n-catalog-lib.js'

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
   * @param {string} value
   */
  const parsed = (value) => {
    const r = parseMessage(value)
    return {
      ok: r.ok,
      placeholders: [...r.placeholders].sort(),
      tags: [...r.tags].sort(),
      pluralCategories: Object.fromEntries([...r.pluralCategories].map(([k, v]) => [k, [...v].sort()])),
    }
  }

  it('plain message has no structure', () => {
    expect(parsed('Just text')).toEqual({ ok: true, placeholders: [], tags: [], pluralCategories: {} })
  })

  it('extracts a simple {name} placeholder', () => {
    expect(parsed('Hello {name}, welcome')).toEqual({
      ok: true,
      placeholders: ['name'],
      tags: [],
      pluralCategories: {},
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
    })
  })

  it('extracts plural categories and the count arg', () => {
    expect(parsed('{count, plural, one {# file} other {# files}}')).toEqual({
      ok: true,
      placeholders: ['count'],
      tags: [],
      pluralCategories: { count: ['one', 'other'] },
    })
  })

  it('extracts select categories', () => {
    expect(parsed('{kind, select, dir {Folder} file {File} other {Item}}')).toEqual({
      ok: true,
      placeholders: ['kind'],
      tags: [],
      pluralCategories: { kind: ['dir', 'file', 'other'] },
    })
  })

  it('handles nested select wrapping plural with inner placeholders', () => {
    const msg =
      '{kind, select, ' +
      'copy {Copied {countText} {count, plural, one {file} other {files}}} ' +
      'other {Moved {countText} {count, plural, one {file} other {files}}}}'
    expect(parsed(msg)).toEqual({
      ok: true,
      placeholders: ['count', 'countText', 'kind'],
      tags: [],
      pluralCategories: { kind: ['copy', 'other'], count: ['one', 'other'] },
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
