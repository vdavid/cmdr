/**
 * Tests for the stale-translation check (`i18n-check-stale.ts`) and the reusable
 * locale-check scaffolding (`i18n-locale-check-lib.ts`) under it.
 *
 * The clean path runs against the COMMITTED fixture (`test/fixtures/
 * i18n-pseudolocale/`): its `en-XA` hashes were generated from its `en`, so they
 * match → no stale. The negative paths copy the fixture into a temp `messages/`
 * root and corrupt one thing at a time (mutate an `en` value, corrupt a stored
 * hash, drop a hash, set `reviewed: true` on a stale key, remove an `en` key) and
 * assert EXACTLY the affected key is flagged. The no-locales path uses a temp root
 * with only `en`.
 *
 * Exit codes: 0 = clean / no locales, 1 = at least one stale finding (normal,
 * warn lane). Strict mode (`strict: true`, set by the release flow) escalates a
 * stale finding to 2 (`EXIT_ERROR`, build-fail) while keeping a clean run at 0.
 * (2 is also the CLI's script-error catch path, exercised by the Go wrapper.)
 *
 * Review is never a gate: a stale `reviewed: true` key is REPORTED with a reset
 * note, but the absence of review never makes a clean key fail (covered below).
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, cpSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runStaleCheck, staleReason } from './i18n-check-stale.ts'
import {
  EXIT_CLEAN,
  EXIT_ISSUES,
  EXIT_ERROR,
  localesToCheck,
  reportFindings,
  newFindings,
} from './i18n-locale-check-lib.ts'
import { sourceHash } from './i18n-catalog-lib.ts'

const FIXTURE_ROOT = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')

/** Collect a run's output lines instead of printing them. */
function capture() {
  const lines: string[] = []
  const write = (l: string) => {
    lines.push(l)
  }
  return { lines, write }
}

describe('staleReason: pure classifier', () => {
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

  it('calls out a stale key whose sameAsSourceJustification no longer applies', () => {
    const r = staleReason(
      'a.b',
      { 'a.b': 'New text' },
      { sourceHash: sourceHash('Old text'), sameAsSourceJustification: 'brand name' },
    )
    expect(r).toMatch(/sameAsSourceJustification no longer applies/)
  })

  it('a justified key that is still fresh is NOT flagged', () => {
    expect(
      staleReason(
        'a.b',
        { 'a.b': 'Dropbox' },
        { sourceHash: sourceHash('Dropbox'), sameAsSourceJustification: 'brand name' },
      ),
    ).toBeNull()
  })
})

describe('runStaleCheck against the committed fixture', () => {
  it('is clean: en-XA hashes match en, so nothing is stale', () => {
    const { lines, write } = capture()
    expect(runStaleCheck({ messagesRoot: FIXTURE_ROOT, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/en-XA: clean\./)
  })

  it('a clean catalog passes in BOTH normal and strict mode', () => {
    expect(runStaleCheck({ messagesRoot: FIXTURE_ROOT, write: () => {} })).toBe(EXIT_CLEAN)
    expect(runStaleCheck({ messagesRoot: FIXTURE_ROOT, strict: true, write: () => {} })).toBe(EXIT_CLEAN)
  })
})

describe('runStaleCheck negative cases (temp catalog copies)', () => {
  let root: string
  let enFile: string
  let xaFile: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-stale-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    enFile = join(root, 'en', 'fixture.json')
    xaFile = join(root, 'en-XA', 'fixture.json')
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  const read = (file: string): Record<string, unknown> =>
    JSON.parse(readFileSync(file, 'utf8')) as Record<string, unknown>
  const write = (file: string, obj: Record<string, unknown>) => {
    writeFileSync(file, JSON.stringify(obj, null, 2) + '\n', 'utf8')
  }

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
    // Only that one key: the report lists exactly one issue line under en-XA.
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags a translation whose stored hash is missing', () => {
    const xa = read(xaFile)
    delete (xa['@fixture.greeting'] as Record<string, unknown>).sourceHash
    write(xaFile, xa)

    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.greeting → no source hash/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags a corrupted stored hash', () => {
    const xa = read(xaFile)
    ;(xa['@fixture.greeting'] as Record<string, unknown>).sourceHash = '0000000'
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
    ;(xa['@fixture.plainLabel'] as Record<string, unknown>).reviewed = true
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

describe('release-strict mode escalates a stale finding to an error exit', () => {
  let root: string
  let enFile: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-stale-strict-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    enFile = join(root, 'en', 'fixture.json')
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  /** Make one en value drift so en-XA goes stale, then run in the given mode. */
  const runStaleFixture = (strict: boolean) => {
    const en = JSON.parse(readFileSync(enFile, 'utf8')) as Record<string, unknown>
    en['fixture.plainLabel'] = 'Dismiss' // was "Cancel"
    writeFileSync(enFile, JSON.stringify(en, null, 2) + '\n', 'utf8')
    const { lines, write } = capture()
    return { code: runStaleCheck({ messagesRoot: root, strict, write }), text: lines.join('\n') }
  }

  it('normal mode: a stale finding is a WARN (exit 1)', () => {
    const { code, text } = runStaleFixture(false)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → source changed since translation/)
  })

  it('strict mode: the SAME stale finding becomes an ERROR (exit 2)', () => {
    const { code, text } = runStaleFixture(true)
    expect(code).toBe(EXIT_ERROR)
    // Same report content; only the exit code is escalated.
    expect(text).toMatch(/fixture\.plainLabel → source changed since translation/)
  })
})

describe('review is never a gate', () => {
  let root: string
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-stale-noreview-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  // The committed fixture carries no `reviewed` flags, so it's already a "no review
  // recorded" catalog. It must pass in both modes: missing review never fails a check.
  it('an unreviewed but fresh catalog passes in normal AND strict mode', () => {
    expect(runStaleCheck({ messagesRoot: root, write: () => {} })).toBe(EXIT_CLEAN)
    expect(runStaleCheck({ messagesRoot: root, strict: true, write: () => {} })).toBe(EXIT_CLEAN)
  })

  it('a fresh key marked reviewed: true is still clean (review state never forces a finding)', () => {
    // staleReason returns null for a fresh key regardless of reviewed, so no gate.
    expect(staleReason('a.b', { 'a.b': 'Cancel' }, { sourceHash: sourceHash('Cancel'), reviewed: true })).toBeNull()
    // ...and a fresh key with NO reviewed flag is equally clean (review is optional metadata).
    expect(staleReason('a.b', { 'a.b': 'Cancel' }, { sourceHash: sourceHash('Cancel') })).toBeNull()
  })
})

describe('no-locales path (only en)', () => {
  let root: string
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

describe('reportFindings: the shared report core (reused by M3)', () => {
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
