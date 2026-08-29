/**
 * Tests for OVERLAY-catalog semantics across the three base-aware locale checks
 * (coverage, parity, stale).
 *
 * An overlay is a regional-variant catalog (`en-GB`, `pt-PT`) whose base language
 * also ships a catalog, so it carries ONLY the keys it forks and everything else
 * resolves through the runtime's fallback chain. The rules invert versus a full
 * translation, and they invert across three checks at once, so they're tested
 * together against ONE fixture rather than scattered over three files: the
 * interesting cases are exactly the ones where "the catalog it overrides" is NOT
 * `en` (a `pt-PT` overlaying `pt`), which no single check's fixture would show.
 *
 * The fixture is built from scratch in a temp dir (never a shipped catalog dir):
 *   en/     3 keys, the source of truth
 *   pt/     a full translation of all 3
 *   pt-PT/  an overlay that forks 1
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { sourceHash } from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import { runCoverageCheck } from './i18n-check-coverage.ts'
import { runParityCheck } from './i18n-check-parity.ts'
import { runStaleCheck } from './i18n-check-stale.ts'

/** English source values, keyed by message key. */
const EN = {
  'app.trash': 'Move to Trash',
  'app.color': 'Color theme',
  'app.greet': 'Hi {name}',
} as const

/** A full Portuguese translation of every English key. */
const PT = {
  'app.trash': 'Mover para o lixo',
  'app.color': 'Tema de cores',
  'app.greet': 'Olá {name}',
} as const

/** One area file's worth of entries: each value plus its `@key.sourceHash` stamp. */
function withHashes(values: Record<string, string>, source: Record<string, string>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(values)) {
    out[key] = value
    out[`@${key}`] = key in source ? { sourceHash: sourceHash(source[key]) } : {}
  }
  return out
}

let root: string

/** Writes one locale dir's single area file. */
function writeLocale(tag: string, entries: Record<string, unknown>): void {
  mkdirSync(join(root, tag), { recursive: true })
  writeFileSync(join(root, tag, 'app.json'), JSON.stringify(entries, null, 2) + '\n', 'utf8')
}

/** Collects the report lines a check writes. */
function capture() {
  const lines: string[] = []
  return { lines, write: (l: string) => void lines.push(l) }
}

/** Runs one check over the fixture root and returns its exit code + rendered report. */
function run(check: (opts: { messagesRoot: string; write: (line: string) => void }) => number) {
  const cap = capture()
  const code = check({ messagesRoot: root, write: cap.write })
  return { code, text: cap.lines.join('\n') }
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-overlay-'))
  writeLocale('en', EN)
  writeLocale('pt', withHashes(PT, EN))
})
afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

describe('coverage: an overlay carries only the keys it forks', () => {
  it('is clean when the overlay forks one key and leaves the rest to the fallback', () => {
    writeLocale('pt-PT', withHashes({ 'app.trash': 'Mover para o caixote do lixo' }, PT))
    const { code, text } = run(runCoverageCheck)
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/pt-PT: clean\./)
    expect(text).toMatch(/pt: clean\./)
  })

  it('flags a key identical to the catalog it overrides as redundant', () => {
    writeLocale('pt-PT', withHashes({ 'app.color': PT['app.color'] }, PT))
    const { code, text } = run(runCoverageCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/app\.color → identical to pt/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('does NOT flag a fork that happens to equal English, since it overrides pt', () => {
    writeLocale('pt-PT', withHashes({ 'app.color': EN['app.color'] }, PT))
    const { code, text } = run(runCoverageCheck)
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/pt-PT: clean\./)
  })

  it('ignores sameAsSourceJustification on an overlay: the fix is always deletion', () => {
    writeLocale('pt-PT', {
      'app.color': PT['app.color'],
      '@app.color': { sourceHash: sourceHash(PT['app.color']), sameAsSourceJustification: 'brand name' },
    })
    const { code, text } = run(runCoverageCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/app\.color → identical to pt/)
  })

  it('flags a key that exists in neither the overlay base nor en', () => {
    writeLocale('pt-PT', { 'app.invented': 'Inventado', '@app.invented': { sourceHash: 'deadbee' } })
    const { code, text } = run(runCoverageCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/app\.invented → unknown key/)
  })
})

describe('coverage: an overlay of en itself (en-GB)', () => {
  it('is clean with one forked key and flags one identical to en', () => {
    writeLocale('en-GB', withHashes({ 'app.trash': 'Move to Bin' }, EN))
    expect(run(runCoverageCheck).code).toBe(EXIT_CLEAN)

    writeLocale('en-GB', withHashes({ 'app.trash': 'Move to Bin', 'app.color': EN['app.color'] }, EN))
    const { code, text } = run(runCoverageCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/app\.color → identical to en/)
  })
})

describe('coverage: the pseudolocale is a full translation, never an overlay', () => {
  it('still reports a key missing from en-XA', () => {
    writeLocale('en-XA', withHashes({ 'app.trash': '[Mövé tö Trásh]' }, EN))
    const { code, text } = run(runCoverageCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/app\.color → missing; renders the English fallback/)
  })
})

describe('parity: an overlay is compared against the catalog it overrides', () => {
  it('is clean when the fork keeps the placeholder', () => {
    writeLocale('pt-PT', withHashes({ 'app.greet': 'Viva {name}' }, PT))
    expect(run(runParityCheck).code).toBe(EXIT_CLEAN)
  })

  it('flags a fork that drops a placeholder', () => {
    writeLocale('pt-PT', withHashes({ 'app.greet': 'Viva' }, PT))
    const { code, text } = run(runParityCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/app\.greet → placeholders expected \{name\}, got \{\(none\)\}/)
  })
})

describe('stale: an overlay hashes the value it overrides, not English', () => {
  it('is clean when the stored hash matches the pt value', () => {
    writeLocale('pt-PT', withHashes({ 'app.trash': 'Mover para o caixote do lixo' }, PT))
    expect(run(runStaleCheck).code).toBe(EXIT_CLEAN)
  })

  it('flags a hash taken from the English value instead of the pt one', () => {
    writeLocale('pt-PT', withHashes({ 'app.trash': 'Mover para o caixote do lixo' }, EN))
    const { code, text } = run(runStaleCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/app\.trash → source changed since translation/)
  })

  it('clears a reviewed flag when the overridden pt value changes', () => {
    writeLocale('pt-PT', {
      'app.trash': 'Mover para o caixote do lixo',
      '@app.trash': { sourceHash: sourceHash('Mover para o lixo velho'), reviewed: true },
    })
    const { code, text } = run(runStaleCheck)
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/the reviewed flag no longer applies/)
  })
})
