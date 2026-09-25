/**
 * Tests for `pnpm i18n:check-locale` (`i18n-check-locale.ts`): every
 * locale-scoped i18n check for the given tags, one line each when clean, full
 * detail only for what isn't.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { runLocaleChecks } from './i18n-check-locale.ts'
import { sourceHash } from './i18n-catalog-lib.ts'

describe('runLocaleChecks (fixture tree)', () => {
  let root: string
  let messagesRoot: string
  let docsRoot: string
  const write = (path: string, data: unknown) => {
    mkdirSync(join(path, '..'), { recursive: true })
    writeFileSync(path, JSON.stringify(data, null, 2))
  }
  const run = (tags: string[]) => {
    const lines: string[] = []
    const code = runLocaleChecks({
      tags,
      messagesRoot,
      docsRoot,
      termAllowlistPath: join(root, 'no-allowlist.json'),
      external: false,
      write: (l) => void lines.push(l),
    })
    return { code, text: lines.join('\n'), lines }
  }

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'check-locale-'))
    messagesRoot = join(root, 'messages')
    docsRoot = join(root, 'docs')
    write(join(messagesRoot, 'en', 'a.json'), { 'a.open': 'Open {name}', 'a.close': 'Close' })
    const stamped = (values: Record<string, string>): Record<string, unknown> =>
      Object.fromEntries(
        Object.entries(values).flatMap(([key, value]): [string, unknown][] => [
          [key, value],
          [`@${key}`, { sourceHash: sourceHash(key === 'a.open' ? 'Open {name}' : 'Close') }],
        ]),
      )
    write(join(messagesRoot, 'sv', 'a.json'), stamped({ 'a.open': 'Öppna {name}', 'a.close': 'Stäng' }))
    // de drops its placeholder: a parity error, which an sv run must never see.
    write(join(messagesRoot, 'de', 'a.json'), stamped({ 'a.open': 'Öffnen', 'a.close': 'Schließen' }))
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('prints one line per check for a clean locale, and never another locale’s findings', () => {
    const { code, text, lines } = run(['sv'])
    expect(code).toBe(0)
    expect(text).not.toMatch(/\bde\b/)
    expect(lines.every((line) => line.startsWith('✓ '))).toBe(true)
    for (const check of ['parity', 'stale', 'icu', 'plural', 'coverage', 'dont-translate', 'aria-label']) {
      expect(text).toMatch(new RegExp(`✓ ${check}\\b`))
    }
    expect(text).toMatch(/✓ term-consistency/)
    expect(text).toMatch(/✓ termbase/)
    expect(text).toMatch(/✓ mechanics/)
    expect(text).toMatch(/✓ quoted-labels/)
  })

  it('shows the detail of a failing check only, and fails', () => {
    const { code, text } = run(['de'])
    expect(code).toBe(1)
    expect(text).toMatch(/✗ parity/)
    expect(text).toContain('a.open')
    expect(text).toMatch(/✓ stale/)
  })

  it('takes several tags', () => {
    const { code, text } = run(['sv', 'de'])
    expect(code).toBe(1)
    expect(text).toMatch(/✗ parity/)
  })

  it('refuses an unknown tag', () => {
    expect(() => run(['xx'])).toThrow(/No catalog for xx/)
  })
})
