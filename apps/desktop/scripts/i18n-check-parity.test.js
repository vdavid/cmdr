/**
 * Tests for the placeholder/tag parity check (`i18n-check-parity.js`).
 *
 * Clean path: the committed pseudolocale fixture preserves every placeholder,
 * tag, and raw `{token}`, so it passes. Negative paths copy the fixture into a
 * temp `messages/` root and corrupt ONE key's structure (drop a placeholder,
 * rename a tag, add an extra arg, drop a raw error token) and assert EXACTLY that
 * key is flagged.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, cpSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runParityCheck, parityDetail } from './i18n-check-parity.js'
import { EXIT_CLEAN, EXIT_ISSUES, localesToCheck } from './i18n-locale-check-lib.js'

const FIXTURE_ROOT = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')

function capture() {
  /** @type {string[]} */
  const lines = []
  return { lines, write: (/** @type {string} */ l) => void lines.push(l) }
}

describe('parityDetail — pure comparison', () => {
  it('is clean when an ICU value preserves placeholders and tags', () => {
    expect(parityDetail('a.b', 'Hello {name}', 'Hola {name}')).toBeNull()
    expect(parityDetail('a.b', 'Open <link>{x}</link>', 'Abrir <link>{x}</link>')).toBeNull()
  })

  it('flags a dropped placeholder', () => {
    expect(parityDetail('a.b', 'Hello {name}', 'Hola')).toMatch(/placeholders expected \{name\}, got \{\(none\)\}/)
  })

  it('flags a renamed placeholder (missing AND extra)', () => {
    expect(parityDetail('a.b', 'Hello {name}', 'Hola {nombre}')).toMatch(/expected \{name\}, got \{nombre\}/)
  })

  it('flags a renamed tag', () => {
    expect(parityDetail('a.b', 'Click <link>{x}</link>', 'Clic <enlace>{x}</enlace>')).toMatch(
      /tags expected <link>, got <enlace>/,
    )
  })

  it('is clean when a raw errors.* value preserves its {token} set', () => {
    expect(parityDetail('errors.x.suggestion', 'Open {system_settings} <folder>', 'Abrir {system_settings} <folder>')).toBeNull()
  })

  it('flags a raw errors.* value that dropped a {token}', () => {
    expect(parityDetail('errors.x.suggestion', 'Open {system_settings}', 'Abrir ajustes')).toMatch(
      /token mismatch: expected \{system_settings\}, got \{\(none\)\}/,
    )
  })
})

describe('runParityCheck against the committed fixture', () => {
  it('is clean: en-XA preserves every placeholder, tag, and raw token', () => {
    const { lines, write } = capture()
    expect(runParityCheck({ messagesRoot: FIXTURE_ROOT, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/en-XA: clean\./)
  })
})

describe('runParityCheck negative cases (temp catalog copies)', () => {
  /** @type {string} */
  let root
  /** @type {string} */
  let xaFile

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-parity-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    xaFile = join(root, 'en-XA', 'fixture.json')
  })
  afterEach(() => rmSync(root, { recursive: true, force: true }))

  const read = () => JSON.parse(readFileSync(xaFile, 'utf8'))
  /** @param {Record<string, any>} obj */
  const writeXa = (obj) => writeFileSync(xaFile, JSON.stringify(obj, null, 2) + '\n', 'utf8')
  const run = () => {
    const cap = capture()
    const code = runParityCheck({ messagesRoot: root, write: cap.write })
    return { code, text: cap.lines.join('\n') }
  }

  it('flags exactly the ICU key whose placeholder was dropped', () => {
    const xa = read()
    xa['fixture.greeting'] = 'Ŵéļçöṁé ḅáçḱ' // dropped {name}
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.greeting → placeholders expected \{name\}, got \{\(none\)\}/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags exactly the key whose tag was renamed', () => {
    const xa = read()
    xa['fixture.openSettings'] = 'Öṗéñ <broken>{label}</broken> ţö çöñţíñüé'
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.openSettings → tags expected <link>, got <broken>/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('flags an extra placeholder the locale invented', () => {
    const xa = read()
    xa['fixture.plainLabel'] = 'Çáñçéļ {oops}'
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.plainLabel → placeholders expected \{\(none\)\}, got \{oops\}/)
  })

  it('flags a raw errors.* key that dropped its {system_settings} token', () => {
    const xa = read()
    xa['errors.fixture.suggestion'] = 'Öṗéñ ajustes, ţḣéñ ŕüñ `ļšöḟ <ḟöļḋéŕ-ṗáţḣ>`.'
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/errors\.fixture\.suggestion → token mismatch: expected \{system_settings\}, got \{\(none\)\}/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })
})

describe('no-locales path (only en)', () => {
  /** @type {string} */
  let root
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-parity-only-en-'))
    mkdirSync(join(root, 'en'), { recursive: true })
    cpSync(join(FIXTURE_ROOT, 'en', 'fixture.json'), join(root, 'en', 'fixture.json'))
  })
  afterEach(() => rmSync(root, { recursive: true, force: true }))

  it('localesToCheck is empty and the check is a clean no-op', () => {
    expect(localesToCheck(root)).toEqual([])
    const { lines, write } = capture()
    expect(runParityCheck({ messagesRoot: root, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/no non-en locales to check/)
  })
})
