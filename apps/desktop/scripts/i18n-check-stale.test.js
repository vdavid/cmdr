/**
 * Tests for the stale-translation check (`i18n-check-stale.js`) and the reusable
 * locale-check scaffolding (`i18n-locale-check-lib.js`) under it.
 *
 * The clean path runs against the COMMITTED fixture (`test/fixtures/
 * i18n-pseudolocale/`): its `en-XA` hashes were generated from its `en`, so they
 * match → no stale. The negative paths copy the fixture into a temp `messages/`
 * root and corrupt one thing at a time (mutate an `en` value, corrupt a stored
 * hash, drop a hash, set `reviewed: true` on a stale key, remove an `en` key) and
 * assert EXACTLY the affected key is flagged. The no-locales path uses a temp root
 * with only `en`.
 *
 * Exit codes: 0 = clean / no locales, 1 = at least one stale finding. (2, the
 * script-error code, is the CLI's catch path, exercised by the Go wrapper.)
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, cpSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runStaleCheck, staleReason } from './i18n-check-stale.js'
import { EXIT_CLEAN, EXIT_ISSUES, localesToCheck, reportFindings, newFindings } from './i18n-locale-check-lib.js'
import { sourceHash } from './i18n-catalog-lib.js'

const FIXTURE_ROOT = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')

/** Collect a run's output lines instead of printing them. */
function capture() {
  /** @type {string[]} */
  const lines = []
  /** @param {string} l */
  const write = (l) => {
    lines.push(l)
  }
  return { lines, write }
}

describe('staleReason — pure classifier', () => {
  it('is fresh when the stored hash matches the current English value', () => {
    expect(staleReason('a.b', { 'a.b': 'Cancel' }, { sourceHash: sourceHash('Cancel') })).toBeNull()
  })

  it('flags a source change (stored hash no longer matches)', () => {
    const r = staleReason('a.b', { 'a.b': 'Cancel changed' }, { sourceHash: sourceHash('Cancel') })
    expect(r).toBe('source changed since translation')
  })

  it('flags a present translation with no stored hash', () => {
    expect(staleReason('a.b', { 'a.b': 'Cancel' }, {})).toMatch(/no source hash/)
    expect(staleReason('a.b', { 'a.b': 'Cancel' }, undefined)).toMatch(/no source hash/)
  })

  it('flags a key whose English source was removed', () => {
    expect(staleReason('a.gone', {}, { sourceHash: 'deadbee' })).toMatch(/source removed/)
  })

  it('calls out a stale key that was marked reviewed (the sign-off no longer applies)', () => {
    const r = staleReason('a.b', { 'a.b': 'New text' }, { sourceHash: sourceHash('Old text'), reviewed: true })
    expect(r).toMatch(/reviewed flag no longer applies/)
  })

  it('a reviewed key that is still fresh is NOT flagged', () => {
    expect(staleReason('a.b', { 'a.b': 'Cancel' }, { sourceHash: sourceHash('Cancel'), reviewed: true })).toBeNull()
  })
})

describe('runStaleCheck against the committed fixture', () => {
  it('is clean: en-XA hashes match en, so nothing is stale', () => {
    const { lines, write } = capture()
    expect(runStaleCheck({ messagesRoot: FIXTURE_ROOT, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/en-XA: clean\./)
  })
})

describe('runStaleCheck negative cases (temp catalog copies)', () => {
  /** @type {string} */
  let root
  /** @type {string} */
  let enFile
  /** @type {string} */
  let xaFile

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-stale-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    enFile = join(root, 'en', 'fixture.json')
    xaFile = join(root, 'en-XA', 'fixture.json')
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  /** @param {string} file @returns {Record<string, any>} */
  const read = (file) => JSON.parse(readFileSync(file, 'utf8'))
  /** @param {string} file @param {Record<string, any>} obj */
  const write = (file, obj) => writeFileSync(file, JSON.stringify(obj, null, 2) + '\n', 'utf8')

  /** Run and return { code, text }. */
  const run = () => {
    const cap = capture()
    const code = runStaleCheck({ messagesRoot: root, write: cap.write })
    return { code, text: cap.lines.join('\n') }
  }

  it('flags exactly the key whose English value changed', () => {
    const en = read(enFile)
    en['fixture.plainLabel'] = 'Dismiss' // was "Cancel"
    write(enFile, en)

    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → source changed since translation/)
    // Only that one key — the report lists exactly one issue line under en-XA.
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags a translation whose stored hash is missing', () => {
    const xa = read(xaFile)
    delete xa['@fixture.greeting'].sourceHash
    write(xaFile, xa)

    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.greeting → no source hash/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags a corrupted stored hash', () => {
    const xa = read(xaFile)
    xa['@fixture.greeting'].sourceHash = '0000000'
    write(xaFile, xa)

    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.greeting → source changed since translation/)
  })

  it('calls out a stale key that carries reviewed: true', () => {
    const en = read(enFile)
    en['fixture.plainLabel'] = 'Dismiss'
    write(enFile, en)
    const xa = read(xaFile)
    xa['@fixture.plainLabel'].reviewed = true
    write(xaFile, xa)

    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → source changed since translation \(the reviewed flag no longer applies/)
  })

  it('flags a translated key whose English source was removed', () => {
    const en = read(enFile)
    delete en['fixture.plainLabel']
    delete en['@fixture.plainLabel']
    write(enFile, en)

    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → English source removed/)
  })
})

describe('no-locales path (only en)', () => {
  /** @type {string} */
  let root
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-stale-only-en-'))
    mkdirSync(join(root, 'en'), { recursive: true })
    cpSync(join(FIXTURE_ROOT, 'en', 'fixture.json'), join(root, 'en', 'fixture.json'))
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('localesToCheck is empty', () => {
    expect(localesToCheck(root)).toEqual([])
  })

  it('the check is a clean no-op', () => {
    const { lines, write } = capture()
    expect(runStaleCheck({ messagesRoot: root, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/no non-en locales to check/)
  })
})

describe('reportFindings — the shared report core (reused by M3)', () => {
  it('returns clean with no locales', () => {
    expect(reportFindings({ title: 'X', findings: [], write: () => {} })).toBe(EXIT_CLEAN)
  })

  it('returns clean when every locale has zero issues', () => {
    const f = newFindings('de')
    expect(reportFindings({ title: 'X', findings: [f], write: () => {} })).toBe(EXIT_CLEAN)
  })

  it('returns issues and lists each finding when a locale has any', () => {
    const f = newFindings('de')
    f.add('a.b', 'broke')
    const { lines, write } = capture()
    expect(reportFindings({ title: 'X', findings: [f], write })).toBe(EXIT_ISSUES)
    expect(lines.join('\n')).toMatch(/a\.b → broke/)
  })
})
