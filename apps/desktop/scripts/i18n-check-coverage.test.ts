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
import { runCoverageCheck, coverageStatus } from './i18n-check-coverage.ts'
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
  it('a justification never excuses a MISSING key', () => {
    expect(coverageStatus('a.b', 'Cancel', {}, { 'a.b': { sameAsSourceJustification: 'brand name' } })).toBe('missing')
  })
})

describe('runCoverageCheck against the committed fixture', () => {
  it('is clean: en-XA has every key and every value differs from English', () => {
    const { lines, write } = capture()
    expect(runCoverageCheck({ messagesRoot: FIXTURE_ROOT, write })).toBe(EXIT_CLEAN)
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
  const run = () => {
    const cap = capture()
    return { code: runCoverageCheck({ messagesRoot: root, write: cap.write }), text: cap.lines.join('\n') }
  }

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
