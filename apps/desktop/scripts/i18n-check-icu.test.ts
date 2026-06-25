/**
 * Tests for the ICU-validity check (`i18n-check-icu.ts`).
 *
 * Clean path: the committed pseudolocale fixture is valid ICU (its raw
 * `errors.*` value is correctly skipped). Negative path: corrupt one ICU value
 * to break parsing and assert exactly that key is flagged; plus a guard that a
 * raw `errors.*` value that ISN'T valid ICU is NOT flagged.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, cpSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runIcuCheck, icuError } from './i18n-check-icu.ts'
import { EXIT_CLEAN, EXIT_ISSUES, localesToCheck } from './i18n-locale-check-lib.ts'

const FIXTURE_ROOT = join(import.meta.dirname, '..', 'test', 'fixtures', 'i18n-pseudolocale')

function capture() {
  const lines: string[] = []
  return { lines, write: (l: string) => void lines.push(l) }
}

describe('icuError: pure classifier', () => {
  it('is clean for valid ICU', () => {
    expect(icuError('a.b', 'Hello {name}')).toBeNull()
    expect(icuError('a.b', '{count, plural, one {# file} other {# files}}')).toBeNull()
  })

  it('flags an unclosed placeholder', () => {
    expect(icuError('a.b', 'Hello {name')).toMatch(/invalid ICU:/)
  })

  it('flags a stray unescaped < parsed as an unclosed tag', () => {
    expect(icuError('a.b', 'Size <dir>')).toMatch(/invalid ICU:/)
  })

  it('SKIPS raw errors.* keys even when the value is not valid ICU', () => {
    // A lone apostrophe + literal <…> is valid raw copy but invalid ICU; must be skipped.
    expect(icuError('errors.x.suggestion', "Here's why, see <folder-path>")).toBeNull()
  })
})

describe('runIcuCheck against the committed fixture', () => {
  it('is clean: en-XA is valid ICU and the raw errors.* value is skipped', () => {
    const { lines, write } = capture()
    expect(runIcuCheck({ messagesRoot: FIXTURE_ROOT, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/en-XA: clean\./)
  })
})

describe('runIcuCheck negative cases (temp catalog copies)', () => {
  let root: string
  let xaFile: string
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-icu-'))
    cpSync(FIXTURE_ROOT, root, { recursive: true })
    xaFile = join(root, 'en-XA', 'fixture.json')
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  const read = (): Record<string, string> => JSON.parse(readFileSync(xaFile, 'utf8')) as Record<string, string>
  const writeXa = (obj: Record<string, string>) => {
    writeFileSync(xaFile, JSON.stringify(obj, null, 2) + '\n', 'utf8')
  }
  const run = () => {
    const cap = capture()
    return { code: runIcuCheck({ messagesRoot: root, write: cap.write }), text: cap.lines.join('\n') }
  }

  it('flags exactly the ICU key that no longer parses', () => {
    const xa = read()
    xa['fixture.greeting'] = 'Ŵéļçöṁé {name' // unclosed brace
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.greeting → invalid ICU:/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('does NOT flag a raw errors.* value that is invalid ICU', () => {
    const xa = read()
    // Lone apostrophe + literal <…>: invalid ICU, valid raw error copy.
    xa['errors.fixture.suggestion'] = "Öṗéñ {system_settings}, ḣéŕé's ŵḣý <ḟöļḋéŕ-ṗáţḣ>."
    writeXa(xa)
    const { code, text } = run()
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/en-XA: clean\./)
  })
})

describe('no-locales path (only en)', () => {
  let root: string
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-icu-only-en-'))
    mkdirSync(join(root, 'en'), { recursive: true })
    cpSync(join(FIXTURE_ROOT, 'en', 'fixture.json'), join(root, 'en', 'fixture.json'))
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('is a clean no-op', () => {
    expect(localesToCheck(root)).toEqual([])
    const { lines, write } = capture()
    expect(runIcuCheck({ messagesRoot: root, write })).toBe(EXIT_CLEAN)
    expect(lines.join('\n')).toMatch(/no non-en locales to check/)
  })
})
