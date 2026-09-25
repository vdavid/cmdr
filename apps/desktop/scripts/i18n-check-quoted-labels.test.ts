/**
 * Tests for the QUOTED-LABEL check (`i18n-check-quoted-labels.ts`): text that
 * quotes another key's label must carry that label as the locale writes it.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { quotedLabelPairs, mismatchedQuotes, runQuotedLabelsCheck } from './i18n-check-quoted-labels.ts'
import { EXIT_CLEAN, EXIT_ISSUES } from './i18n-locale-check-lib.ts'

const en = {
  'a.dontShow': 'Don’t show again',
  'a.reopen': 'Reopen closed tab…',
  'a.hint': 'Click “Don’t show again” to hide it for good.',
  'a.menu': 'Use “Reopen closed tab” (⌘⇧T) to bring it back.',
  'a.word': 'Type “hello” to test, or look under “added”.',
  'a.added': 'added',
  'a.long': 'This is a long sentence that nobody quotes anywhere else at all.',
}

describe('quotedLabelPairs', () => {
  it('pairs English text with the keys whose whole value it quotes, ignoring a trailing ellipsis', () => {
    expect(quotedLabelPairs(en)).toEqual([
      { key: 'a.hint', label: 'Don’t show again', sources: ['a.dontShow'] },
      { key: 'a.menu', label: 'Reopen closed tab', sources: ['a.reopen'] },
    ])
  })
})

describe('mismatchedQuotes', () => {
  const pairs = quotedLabelPairs(en)
  it('flags a quote that paraphrases the label, and passes one that copies it in any case or quote style', () => {
    const es = {
      'a.dontShow': 'No volver a mostrar',
      'a.reopen': 'Reabrir la pestaña cerrada…',
      'a.hint': 'Haz clic en “No mostrar más” para ocultarlo.',
      'a.menu': 'Usa «reabrir la pestaña cerrada» (⌘⇧T).',
    }
    expect(mismatchedQuotes(pairs, es)).toEqual([
      {
        key: 'a.hint',
        detail: 'quotes “Don’t show again”, but not its label here: “No volver a mostrar” (a.dontShow)',
      },
    ])
  })
  it('skips a pair whose key or label the locale lacks', () => {
    expect(mismatchedQuotes(pairs, { 'a.hint': 'x' })).toEqual([])
  })
})

describe('runQuotedLabelsCheck (fixture tree)', () => {
  let root: string
  const write = (path: string, data: unknown) => {
    mkdirSync(join(path, '..'), { recursive: true })
    writeFileSync(path, JSON.stringify(data, null, 2))
  }
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'quoted-labels-'))
    write(join(root, 'en', 'a.json'), en)
    write(join(root, 'es', 'a.json'), {
      ...en,
      'a.dontShow': 'No volver a mostrar',
      'a.hint': 'Pulsa “No volver a mostrar”.',
    })
    write(join(root, 'es-419', 'a.json'), { 'a.dontShow': 'No mostrar de nuevo' })
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('reads an overlay through its base: a forked label the quoting text still spells the old way is a finding', () => {
    const lines: string[] = []
    expect(runQuotedLabelsCheck({ messagesRoot: root, write: (line) => void lines.push(line) })).toBe(EXIT_ISSUES)
    expect(lines.join('\n')).toMatch(/es: clean/)
    expect(lines.join('\n')).toMatch(/es-419[\s\S]*a\.hint → quotes “Don’t show again”/)
  })

  it('is clean once the overlay forks the quoting text too', () => {
    write(join(root, 'es-419', 'a.json'), {
      'a.dontShow': 'No mostrar de nuevo',
      'a.hint': 'Pulsa “No mostrar de nuevo”.',
    })
    expect(runQuotedLabelsCheck({ messagesRoot: root, write: () => {} })).toBe(EXIT_CLEAN)
  })
})
