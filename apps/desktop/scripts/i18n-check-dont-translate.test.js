/**
 * Tests for the don't-translate-tokens check (`i18n-check-dont-translate.js`).
 *
 * Clean path: the committed pseudolocale fixture preserves `{system_settings}`
 * (its only listed token) in en-XA. Negative paths (temp copies): drop the system
 * token from the locale value, and introduce a brand word in English that the
 * locale value omits, asserting exactly those keys are flagged.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, cpSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runDontTranslateCheck, droppedTokens, BRAND_WORDS, SYSTEM_TOKENS } from './i18n-check-dont-translate.js'
import { EXIT_CLEAN, EXIT_ISSUES, localesToCheck } from './i18n-locale-check-lib.js'

const FIXTURE_ROOT = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')

function capture() {
  /** @type {string[]} */
  const lines = []
  return { lines, write: (/** @type {string} */ l) => void lines.push(l) }
}

describe('curated lists', () => {
  it('include the brand and system tokens the spec names', () => {
    for (const w of ['Cmdr', 'macOS', 'GitHub', 'SMB', 'MTP']) expect(BRAND_WORDS).toContain(w)
    expect(SYSTEM_TOKENS).toContain('{system_settings}')
  })
})

describe('droppedTokens: pure detector', () => {
  it('clean when the locale keeps the brand word and the system token', () => {
    expect(droppedTokens('Open {system_settings} in macOS', 'Abrir {system_settings} en macOS')).toEqual([])
  })
  it('flags a dropped brand word', () => {
    expect(droppedTokens('Built for macOS', 'Hecho para Mac')).toEqual(['macOS'])
  })
  it('flags a dropped system token', () => {
    expect(droppedTokens('Open {system_settings}', 'Abrir ajustes')).toEqual(['{system_settings}'])
  })
  it('matches brand words whole-word only (no substring false positive)', () => {
    // "Cmdr" is not present as a whole word in "Cmdrs", but the English here has no
    // whole-word Cmdr either, so nothing to drop.
    expect(droppedTokens('See Cmdr docs', 'Ver Cmdr docs')).toEqual([])
    expect(droppedTokens('See Cmdr docs', 'Ver documentos')).toEqual(['Cmdr'])
  })
})

describe('runDontTranslateCheck against the committed fixture', () => {
  it('is clean: en-XA keeps {system_settings} and the Cmdr/macOS brand words verbatim', () => {
    const { lines, write } = capture()
    expect(runDontTranslateCheck({ messagesRoot: FIXTURE_ROOT, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/en-XA: clean\./)
  })
})

describe('runDontTranslateCheck negative cases (temp catalog copies)', () => {
  /** @type {string} */
  let root
  /** @type {string} */
  let enFile
  /** @type {string} */
  let xaFile
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-dt-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    enFile = join(root, 'en', 'fixture.json')
    xaFile = join(root, 'en-XA', 'fixture.json')
  })
  afterEach(() => rmSync(root, { recursive: true, force: true }))

  /** @param {string} f */
  const read = (f) => JSON.parse(readFileSync(f, 'utf8'))
  /** @param {string} f @param {Record<string, any>} o */
  const writeJson = (f, o) => writeFileSync(f, JSON.stringify(o, null, 2) + '\n', 'utf8')
  const run = () => {
    const cap = capture()
    return { code: runDontTranslateCheck({ messagesRoot: root, write: cap.write }), text: cap.lines.join('\n') }
  }

  it('flags a locale value that dropped {system_settings}', () => {
    const xa = read(xaFile)
    xa['errors.fixture.suggestion'] = xa['errors.fixture.suggestion'].replace('{system_settings}', 'ajustes')
    writeJson(xaFile, xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/errors\.fixture\.suggestion → dropped \{system_settings\} \(keep verbatim\)/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags a locale value that dropped a brand word English carries', () => {
    const en = read(enFile)
    en['fixture.plainLabel'] = 'Cancel in macOS'
    writeJson(enFile, en)
    const xa = read(xaFile)
    xa['fixture.plainLabel'] = 'Çáñçéļ' // dropped "macOS"
    writeJson(xaFile, xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → dropped macOS \(keep verbatim\)/)
  })
})

describe('no-locales path (only en)', () => {
  /** @type {string} */
  let root
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-dt-only-en-'))
    mkdirSync(join(root, 'en'), { recursive: true })
    cpSync(join(FIXTURE_ROOT, 'en', 'fixture.json'), join(root, 'en', 'fixture.json'))
  })
  afterEach(() => rmSync(root, { recursive: true, force: true }))

  it('is a clean no-op', () => {
    expect(localesToCheck(root)).toEqual([])
    const { lines, write } = capture()
    expect(runDontTranslateCheck({ messagesRoot: root, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/no non-en locales to check/)
  })
})
