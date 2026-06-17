/**
 * Tests for the pseudolocale generator (`gen-pseudolocale.js`) — the universal
 * i18n test fixture that M2/M3 trust, so correctness of placeholder/ICU
 * preservation is the load-bearing property here.
 *
 * Three layers:
 *  1. Unit tests on the transform of representative shapes (plain, placeholder,
 *     tag, plural, select, multi-placeholder, raw error) + determinism + accent +
 *     expansion + the `@key.sourceHash` stamp.
 *  2. The ACCEPTANCE BAR over the WHOLE real `en` catalog: for every ICU key,
 *     `parseMessage(pseudo)` token sets equal `parseMessage(en)`'s and the pseudo
 *     is valid ICU; for every raw `errors.*` key, the `{…}` brace-token set is
 *     preserved. This proves the generator mangles nothing structural.
 *  3. The committed fixture (`test/fixtures/i18n-pseudolocale/`) matches what the
 *     generator produces (pins the fixture to the generator so M2/M3 test against
 *     a faithful copy).
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { loadCatalog, parseMessage, sourceHash, isMetadataKey, readLocaleFiles } from './i18n-catalog-lib.js'
import { buildPseudoFile, isRawKey, pseudoIcu, pseudoRaw, pseudoValue, PSEUDO_LOCALE } from './gen-pseudolocale.js'

/**
 * Set equality.
 * @param {Set<string>} a
 * @param {Set<string>} b
 * @returns {boolean}
 */
const setEq = (a, b) => a.size === b.size && [...a].every((x) => b.has(x))
/**
 * `Map<string, Set<string>>` equality.
 * @param {Map<string, Set<string>>} a
 * @param {Map<string, Set<string>>} b
 * @returns {boolean}
 */
const mapEq = (a, b) => {
  if (a.size !== b.size) return false
  for (const [k, v] of a) {
    const w = b.get(k)
    if (!w || !setEq(v, w)) return false
  }
  return true
}
/**
 * The `{name}` brace-tokens of a raw (non-ICU) string — the substitution targets.
 * @param {string} s
 * @returns {Set<string>}
 */
const braceTokens = (s) => new Set([...s.matchAll(/\{([^{}]+)\}/g)].map((m) => m[1]))

describe('pseudoIcu — transforms literal text, preserves structure', () => {
  /**
   * Parse both sides and assert token-set equality + valid pseudo ICU.
   * @param {string} en
   * @returns {string} the pseudo value
   */
  const expectStructurePreserved = (en) => {
    const pseudo = pseudoIcu(en)
    const pe = parseMessage(en)
    const pp = parseMessage(pseudo)
    expect(pp.ok, `pseudo not valid ICU: ${pseudo}`).toBe(true)
    expect(setEq(pp.placeholders, pe.placeholders)).toBe(true)
    expect(setEq(pp.tags, pe.tags)).toBe(true)
    expect(mapEq(pp.pluralCategories, pe.pluralCategories)).toBe(true)
    return pseudo
  }

  it('a plain label is accented and expanded', () => {
    const pseudo = pseudoIcu('Cancel')
    expect(pseudo).not.toBe('Cancel')
    expect([...pseudo].length).toBeGreaterThan('Cancel'.length)
    // No ASCII letters survive (all mapped).
    expect(/[A-Za-z]/.test(pseudo)).toBe(false)
  })

  it('preserves a {placeholder} arg name verbatim', () => {
    const pseudo = expectStructurePreserved('Welcome back, {name}')
    expect(pseudo).toContain('{name}')
  })

  it('preserves <tag> names and the placeholder inside', () => {
    const pseudo = expectStructurePreserved('Open <link>{label}</link> now')
    expect(pseudo).toContain('<link>')
    expect(pseudo).toContain('</link>')
    expect(pseudo).toContain('{label}')
  })

  it('preserves plural structure, categories, and #', () => {
    const pseudo = expectStructurePreserved('{count, plural, one {# file} other {# files}}')
    expect(pseudo).toContain('{count, plural,')
    expect(pseudo).toContain(' one {')
    expect(pseudo).toContain(' other {')
    expect(pseudo).toContain('#')
  })

  it('preserves select structure and categories', () => {
    const pseudo = expectStructurePreserved('{side, select, left {Left pane} other {Right pane}}')
    expect(pseudo).toContain('{side, select,')
    expect(pseudo).toContain(' left {')
    expect(pseudo).toContain(' other {')
  })

  it('preserves multiple placeholders in a sentence', () => {
    const pseudo = expectStructurePreserved('Copied {fileText} from {source} to {target}')
    for (const tok of ['{fileText}', '{source}', '{target}']) expect(pseudo).toContain(tok)
  })

  it('preserves =N explicit plural categories', () => {
    expectStructurePreserved('{count, plural, =0 {none} one {# item} other {# items}}')
  })

  it('handles nested select wrapping plural', () => {
    expectStructurePreserved(
      '{kind, select, copy {Copied {count, plural, one {file} other {files}}} other {Moved {count, plural, one {file} other {files}}}}',
    )
  })

  it('doubles a lone apostrophe so the pseudo re-parses (ICU escape rule)', () => {
    const pseudo = expectStructurePreserved("It''s here {x}")
    expect(pseudo).toContain("''")
  })

  it('leaves an all-placeholder message structurally identical (no literal text to grow)', () => {
    const en = '{prefix} {value} {unit}'
    const pseudo = pseudoIcu(en)
    expect(parseMessage(pseudo).ok).toBe(true)
    expect(setEq(parseMessage(pseudo).placeholders, parseMessage(en).placeholders)).toBe(true)
  })
})

describe('pseudoRaw — accents text, preserves {tokens} and literal markup', () => {
  it('preserves {token} substitution targets and accents the rest', () => {
    const en = 'Open {system_settings} to continue'
    const pseudo = pseudoRaw(en)
    expect(pseudo).toContain('{system_settings}')
    expect(pseudo).not.toContain('Open')
    expect(setEq(braceTokens(pseudo), braceTokens(en))).toBe(true)
  })

  it('treats <…> as literal text (accents it), unlike the ICU path', () => {
    const pseudo = pseudoRaw('run `lsof <folder-path>`')
    // Backtick and angle brackets survive; the letters inside are accented.
    expect(pseudo).toContain('`')
    expect(pseudo).toContain('<')
    expect(pseudo).toContain('>')
    expect(pseudo).not.toContain('folder')
  })

  it('preserves markdown (**, backtick, newline, list dash) and a lone apostrophe', () => {
    const en = "**Bold** and `code`\n- item\nHere's why"
    const pseudo = pseudoRaw(en)
    expect(pseudo).toContain('**')
    expect(pseudo).toContain('`')
    expect(pseudo).toContain('\n- ')
    // Raw path does NOT double apostrophes (the raw pipeline isn't ICU).
    expect(pseudo).toContain("'")
    expect(pseudo).not.toContain("''")
  })
})

describe('isRawKey / pseudoValue route by family', () => {
  it('errors.* are raw, everything else is ICU', () => {
    expect(isRawKey('errors.listing.x.suggestion')).toBe(true)
    expect(isRawKey('common.ok')).toBe(false)
  })

  it('pseudoValue picks the raw path for errors.* (keeps a literal <…>)', () => {
    const v = 'See `lsof <x>` {path}'
    expect(pseudoValue('errors.fixture.s', v)).toBe(pseudoRaw(v))
    // ICU path on the same string would treat <x> as an (unclosed) tag → throw.
    expect(() => pseudoValue('common.fixture', v)).toThrow()
  })
})

describe('brand-word preservation', () => {
  it('keeps brand/system words verbatim while accenting surrounding text (ICU)', () => {
    const out = pseudoIcu('Cmdr runs on macOS')
    expect(out).toContain('Cmdr')
    expect(out).toContain('macOS')
    // The non-brand word "runs" is accented, so the output isn't the input.
    expect(out).not.toBe('Cmdr runs on macOS')
    expect(out).toMatch(/Cmdr .*macOS/)
  })

  it('keeps brand words verbatim on the raw path too', () => {
    const out = pseudoRaw('Open GitHub to report, see SMB docs')
    expect(out).toContain('GitHub')
    expect(out).toContain('SMB')
  })

  it('only protects whole words (a brand word as a substring is still accented)', () => {
    // "Rusty" contains "Rust" but isn't the brand word, so it's accented whole.
    expect(pseudoIcu('Rusty')).not.toContain('Rust')
  })
})

describe('determinism', () => {
  it('same input → byte-identical output (ICU)', () => {
    const en = 'Copied {fileText} from {source} to {target}'
    expect(pseudoIcu(en)).toBe(pseudoIcu(en))
  })

  it('same input → byte-identical output (raw)', () => {
    const en = "Open {system_settings}. Here's `cmd`."
    expect(pseudoRaw(en)).toBe(pseudoRaw(en))
  })

  it('buildPseudoFile is deterministic for a whole file', () => {
    const raw = { 'a.b': 'Hello {x}', '@a.b': { description: 'd' }, 'a.c': 'Plain' }
    expect(buildPseudoFile(raw)).toEqual(buildPseudoFile(raw))
  })
})

describe('buildPseudoFile — values + sourceHash metadata', () => {
  const raw = {
    'a.label': 'Cancel',
    '@a.label': { description: 'A label' },
    'a.greet': 'Hi {name}',
  }
  const built = buildPseudoFile(raw)

  it('emits a pseudo value for every message key', () => {
    expect(typeof built['a.label']).toBe('string')
    expect(typeof built['a.greet']).toBe('string')
  })

  it('stamps @key.sourceHash = sourceHash(en value) for every key', () => {
    expect(built['@a.label']).toEqual({ sourceHash: sourceHash('Cancel') })
    expect(built['@a.greet']).toEqual({ sourceHash: sourceHash('Hi {name}') })
  })

  it('carries only sourceHash metadata (not the en description)', () => {
    expect(built['@a.label']).not.toHaveProperty('description')
  })
})

describe('ACCEPTANCE BAR — whole real en catalog round-trips with no structural loss', () => {
  const en = loadCatalog('en')

  it('every ICU key: pseudo is valid ICU and token sets equal the source', () => {
    const failures = []
    for (const [key, value] of Object.entries(en.messages)) {
      if (isRawKey(key)) continue
      const pseudo = pseudoValue(key, value)
      const pe = parseMessage(value)
      const pp = parseMessage(pseudo)
      if (!pp.ok) failures.push(`${key}: invalid pseudo ICU (${pp.error})`)
      else if (
        !setEq(pp.placeholders, pe.placeholders) ||
        !setEq(pp.tags, pe.tags) ||
        !mapEq(pp.pluralCategories, pe.pluralCategories)
      ) {
        failures.push(`${key}: token-set mismatch`)
      }
    }
    expect(failures).toEqual([])
  })

  it('every raw errors.* key: {…} brace-token set is preserved', () => {
    const failures = []
    for (const [key, value] of Object.entries(en.messages)) {
      if (!isRawKey(key)) continue
      const pseudo = pseudoValue(key, value)
      if (!setEq(braceTokens(pseudo), braceTokens(value))) failures.push(key)
    }
    expect(failures).toEqual([])
  })

  it('every key: @key.sourceHash equals sourceHash(en value)', () => {
    const enFiles = readLocaleFiles('en')
    for (const rawFile of Object.values(enFiles)) {
      const built = buildPseudoFile(rawFile)
      for (const [key, value] of Object.entries(rawFile)) {
        if (isMetadataKey(key) || typeof value !== 'string') continue
        expect(built[`@${key}`]).toEqual({ sourceHash: sourceHash(value) })
      }
    }
  })

  it('accents and lengthens: most keys grow and lose all ASCII letters', () => {
    let grew = 0
    let total = 0
    for (const [key, value] of Object.entries(en.messages)) {
      total++
      const pseudo = pseudoValue(key, value)
      if ([...pseudo].length > [...value].length) grew++
    }
    // All but the rare all-placeholder messages grow.
    expect(grew).toBeGreaterThan(total * 0.98)
  })
})

describe('committed fixture matches the generator', () => {
  const fixtureDir = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')
  /**
   * @param {string} rel
   * @returns {Record<string, any>}
   */
  const readJson = (rel) => JSON.parse(readFileSync(join(fixtureDir, rel), 'utf8'))

  it('en-XA/fixture.json is exactly buildPseudoFile(en/fixture.json)', () => {
    const en = readJson('en/fixture.json')
    const enXa = readJson('en-XA/fixture.json')
    expect(buildPseudoFile(en)).toEqual(enXa)
  })

  it('covers a plain label, placeholder, tag, plural, select, multi-placeholder, and raw error', () => {
    const en = readJson('en/fixture.json')
    expect(en['fixture.plainLabel']).toBeDefined()
    expect(en['fixture.greeting']).toContain('{name}')
    expect(en['fixture.openSettings']).toContain('<link>')
    expect(en['fixture.fileCount']).toContain('plural,')
    expect(en['fixture.paneSide']).toContain('select,')
    expect(en['fixture.transferSummary']).toContain('{source}')
    expect(Object.keys(en).some((k) => isRawKey(k))).toBe(true)
  })

  it('PSEUDO_LOCALE is the en-XA tag the fixture dir uses', () => {
    expect(PSEUDO_LOCALE).toBe('en-XA')
  })
})
