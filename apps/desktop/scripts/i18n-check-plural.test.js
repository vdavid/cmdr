/**
 * Tests for the plural-category coverage check (`i18n-check-plural.js`).
 *
 * Clean path: the committed pseudolocale fixture's plural message covers en-XA's
 * required categories (one, other). Negative paths: drop a required category from
 * the fixture (one → flagged), and exercise the pure classifier against a
 * richer-CLDR locale (Polish needs one/few/many/other) without committing any
 * real-language content — the data-driven required set comes from `Intl`.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, cpSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runPluralCheck, pluralCoverageDetail, requiredPluralCategories } from './i18n-check-plural.js'
import { EXIT_CLEAN, EXIT_ISSUES, localesToCheck } from './i18n-locale-check-lib.js'

const FIXTURE_ROOT = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')

function capture() {
  /** @type {string[]} */
  const lines = []
  return { lines, write: (/** @type {string} */ l) => void lines.push(l) }
}

describe('requiredPluralCategories — data-driven from Intl', () => {
  it('English needs one and other', () => {
    expect([...requiredPluralCategories('en')].sort()).toEqual(['one', 'other'])
  })

  it('Polish needs one, few, many, and other (a richer CLDR set than English)', () => {
    expect([...requiredPluralCategories('pl')].sort()).toEqual(['few', 'many', 'one', 'other'])
  })

  it('Japanese needs only other', () => {
    expect([...requiredPluralCategories('ja')]).toEqual(['other'])
  })
})

describe('pluralCoverageDetail — pure classifier', () => {
  it('is clean when a message covers the locale required set', () => {
    expect(pluralCoverageDetail('en', '{count, plural, one {# file} other {# files}}')).toBeNull()
  })

  it('is clean for a message with no plurals', () => {
    expect(pluralCoverageDetail('pl', 'Plain text {name}')).toBeNull()
  })

  it('flags a Polish plural that only covers one/other (missing few, many)', () => {
    const r = pluralCoverageDetail('pl', '{count, plural, one {# plik} other {# plików}}')
    expect(r).toMatch(/\{count\} missing plural categories few, many/)
  })

  it('flags an English plural missing the one category', () => {
    expect(pluralCoverageDetail('en', '{count, plural, other {# files}}')).toMatch(/\{count\} missing plural category one/)
  })
})

describe('runPluralCheck against the committed fixture', () => {
  it('is clean: en-XA needs one/other and the fixture plural covers both', () => {
    expect([...requiredPluralCategories('en-XA')].sort()).toEqual(['one', 'other'])
    const { lines, write } = capture()
    expect(runPluralCheck({ messagesRoot: FIXTURE_ROOT, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/en-XA: clean\./)
  })
})

describe('runPluralCheck negative case (temp catalog copy)', () => {
  /** @type {string} */
  let root
  /** @type {string} */
  let xaFile
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-plural-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    xaFile = join(root, 'en-XA', 'fixture.json')
  })
  afterEach(() => rmSync(root, { recursive: true, force: true }))

  it('flags exactly the plural key that dropped a required category', () => {
    const xa = JSON.parse(readFileSync(xaFile, 'utf8'))
    xa['fixture.fileCount'] = '{count, plural, other {# ḟíļéš}}' // dropped the `one` branch
    writeFileSync(xaFile, JSON.stringify(xa, null, 2) + '\n', 'utf8')
    const cap = capture()
    const code = runPluralCheck({ messagesRoot: root, write: cap.write })
    const text = cap.lines.join('\n')
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.fileCount → \{count\} missing plural category one/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })
})

describe('no-locales path (only en)', () => {
  /** @type {string} */
  let root
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-plural-only-en-'))
    mkdirSync(join(root, 'en'), { recursive: true })
    cpSync(join(FIXTURE_ROOT, 'en', 'fixture.json'), join(root, 'en', 'fixture.json'))
  })
  afterEach(() => rmSync(root, { recursive: true, force: true }))

  it('is a clean no-op', () => {
    expect(localesToCheck(root)).toEqual([])
    const { lines, write } = capture()
    expect(runPluralCheck({ messagesRoot: root, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/no non-en locales to check/)
  })
})
