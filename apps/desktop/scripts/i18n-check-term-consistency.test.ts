/**
 * Tests for the TERM CONSISTENCY check (`i18n-check-term-consistency.ts`).
 *
 * The classifier is pure, so most of this exercises `findDivergences` directly:
 * two keys carrying the SAME English must render the same way in one locale, and
 * an OVERLAY is judged on what it actually renders (its own value where it forks,
 * the base value where it doesn't), which is what catches a half-forked term.
 */
import { describe, it, expect } from 'vitest'
import {
  findDivergences,
  normalizeForComparison,
  isAllowed,
  report,
  shrinkWrap,
} from './i18n-check-term-consistency.ts'
import type { Allowlist, DivergenceFinding, LocaleOutcome } from './i18n-check-term-consistency.ts'
import type { Catalog } from './i18n-catalog-lib.ts'

const cat = (messages: Record<string, string>): Catalog => ({ messages, metadata: {} })

describe('normalizeForComparison', () => {
  it('unescapes the ICU doubled apostrophe so an ICU key and a raw key compare equal', () => {
    expect(normalizeForComparison("doesn''t", 'fileOperations.a')).toBe("doesn't")
  })
  it('leaves a raw errors.* value alone (raw keys never double their apostrophes)', () => {
    expect(normalizeForComparison("doesn't", 'errors.a')).toBe("doesn't")
  })
  it('ignores a trailing ellipsis, in either shape', () => {
    expect(normalizeForComparison('Loading…', 'a.b')).toBe(normalizeForComparison('Loading...', 'a.b'))
  })
  it('ignores trailing sentence punctuation and collapses whitespace', () => {
    expect(normalizeForComparison('Done.', 'a.b')).toBe('Done')
    expect(normalizeForComparison('a  b', 'a.b')).toBe('a b')
  })
  it('ignores punctuation that WRAPS a term, whatever the script puts where', () => {
    // French spaces its `!` and `?` off the word (with a narrow no-break space),
    // Spanish opens with `¡` / `¿`, CJK closes with a full-width stop. All of it
    // decorates the same term, so all of it normalizes away.
    expect(normalizeForComparison('Copié\u202f!', 'a.b')).toBe('Copié')
    expect(normalizeForComparison('¡Copiado!', 'a.b')).toBe('Copiado')
    expect(normalizeForComparison('¿Eliminar?', 'a.b')).toBe('Eliminar')
    expect(normalizeForComparison('已复制。', 'a.b')).toBe('已复制')
  })
  it('keeps a placeholder brace and a percent sign, which are part of the term', () => {
    expect(normalizeForComparison('{percent}%', 'a.b')).toBe('{percent}%')
    expect(normalizeForComparison('{count} left', 'a.b')).toBe('{count} left')
  })
  it('keeps case, so a sentence-case label and a Title Case one stay distinct', () => {
    expect(normalizeForComparison('Hide others', 'a.b')).not.toBe(normalizeForComparison('Hide Others', 'a.b'))
  })
})

describe('findDivergences: full translation', () => {
  const source = cat({ 'menu.pal': 'Command palette', 'cmd.pal': 'Command palette', 'x.y': 'Other' })

  it('finds nothing when both keys render the same way', () => {
    const out = findDivergences(source, cat({ 'menu.pal': '指令面板', 'cmd.pal': '指令面板', 'x.y': '其他' }), false)
    expect(out).toEqual([])
  })

  it('flags one English value rendered two ways, listing both renderings and their keys', () => {
    const out = findDivergences(source, cat({ 'menu.pal': '命令選擇區', 'cmd.pal': '指令面板', 'x.y': '其他' }), false)
    expect(out).toHaveLength(1)
    expect(out[0].source).toBe('Command palette')
    expect(out[0].renderings.map((r) => r.value).sort()).toEqual(['命令選擇區', '指令面板'])
    expect(out[0].renderings.flatMap((r) => r.keys).sort()).toEqual(['cmd.pal', 'menu.pal'])
  })

  it('does not flag a difference that is only a trailing ellipsis', () => {
    expect(
      findDivergences(source, cat({ 'menu.pal': '指令面板…', 'cmd.pal': '指令面板', 'x.y': '其他' }), false),
    ).toEqual([])
  })

  it('ignores a source value that appears only once', () => {
    expect(findDivergences(cat({ 'a.b': 'Only one' }), cat({ 'a.b': 'Egy' }), false)).toEqual([])
  })
})

describe('findDivergences: overlay', () => {
  // `source` is what the overlay renders on top of (en, layered). The two keys share one English value.
  const source = cat({ 'errors.noTrash': 'no Trash here', 'ops.noTrash': 'no Trash here' })

  it('flags a HALF-forked term: one key forked to Bin, the sibling still renders Trash', () => {
    const out = findDivergences(source, cat({ 'errors.noTrash': 'no Bin here' }), true)
    expect(out).toHaveLength(1)
    expect(out[0].renderings.map((r) => r.value).sort()).toEqual(['no Bin here', 'no Trash here'])
  })

  it('is clean once both keys fork', () => {
    expect(
      findDivergences(source, cat({ 'errors.noTrash': 'no Bin here', 'ops.noTrash': 'no Bin here' }), true),
    ).toEqual([])
  })

  it('is clean when the overlay forks neither (both fall through to the same base value)', () => {
    expect(findDivergences(source, cat({}), true)).toEqual([])
  })
})

describe('isAllowed', () => {
  it('accepts an entry whose source matches and whose reason is non-empty', () => {
    expect(isAllowed('Done', [{ source: 'Done', reason: 'a checklist step vs an operation outcome' }])).toBe(true)
  })
  it('rejects an entry with an empty reason: the allowlist exists to record WHY the split is right', () => {
    expect(isAllowed('Done', [{ source: 'Done', reason: '   ' }])).toBe(false)
  })
  it('rejects a source that is not listed', () => {
    expect(isAllowed('Done', [{ source: 'Running', reason: 'process vs task' }])).toBe(false)
  })
})

describe('report: the summary line has to be true', () => {
  const finding = (index: number): DivergenceFinding => ({
    source: `term ${String(index)}`,
    renderings: [
      { value: 'a', keys: ['x.a'] },
      { value: 'b', keys: ['x.b'] },
    ],
  })

  /**
   * @param divergent how many terms the locale renders two ways
   * @param baseline the recorded `notYetReviewed` count, if the locale has one
   * @param allowed how many of those divergences carry a reasoned allowlist entry
   */
  const outcome = (locale: string, divergent: number, baseline?: number, allowed = 0): LocaleOutcome => {
    const divergences = Array.from({ length: divergent }, (_, index) => finding(index))
    return {
      locale,
      isOverlay: false,
      divergences,
      unallowed: divergences.slice(allowed),
      staleAllows: [],
      baseline,
    }
  }

  const linesFor = (outcomes: LocaleOutcome[]): { lines: string[]; code: number } => {
    const lines: string[] = []
    const code = report(outcomes, (line) => lines.push(line))
    return { lines, code }
  }

  it('names the untriaged total instead of claiming every locale is consistent', () => {
    // A baselined locale exits clean, which is the right design: the count only
    // ratchets down and the check stays warn-only. But it may not then say the
    // problem does not exist.
    const { lines, code } = linesFor([outcome('hu', 28, 28), outcome('de', 20, 20)])
    expect(code).toBe(0)
    const summary = lines[lines.length - 1]
    expect(summary).toContain('48')
    expect(summary).toContain('2 locales')
    expect(summary).not.toBe('Term consistency: every locale names one thing one way (or says why not).')
  })

  it('still claims the clean sweep when nothing is awaiting triage', () => {
    const { lines, code } = linesFor([outcome('en-GB', 0)])
    expect(code).toBe(0)
    expect(lines[lines.length - 1]).toBe('Term consistency: every locale names one thing one way (or says why not).')
  })

  it('leaves the warn path alone: a grown baseline still exits non-zero', () => {
    const { code } = linesFor([outcome('hu', 30, 28)])
    expect(code).toBe(1)
  })

  it('judges a baselined locale on what is still UNEXPLAINED, so one term can be triaged at a time', () => {
    // A locale awaiting triage is exactly where a genuine split is most likely to
    // be found, and recording it is the only honest way to move the number: the
    // alternative is a translation edit that makes the copy worse.
    const { lines, code } = linesFor([outcome('de', 9, 8, 1)])
    expect(code).toBe(0)
    expect(lines[0]).toContain('8 divergent terms')
    expect(lines[0]).not.toContain('up from')
  })

  it('counts only the unexplained ones as awaiting triage', () => {
    const { lines } = linesFor([outcome('de', 9, 8, 1)])
    expect(lines[lines.length - 1]).toContain('8 divergences across 1 locale')
  })

  it('a baselined locale keeps ratcheting: one more reason drops it under the baseline', () => {
    const { lines } = linesFor([outcome('de', 9, 8, 2)])
    expect(lines[0]).toContain('down from 8; ratchet the baseline')
  })

  it('flags a stale allowlist entry on a baselined locale too', () => {
    const { lines, code } = linesFor([{ ...outcome('de', 8, 8), staleAllows: ['Purple'] }])
    expect(code).toBe(1)
    expect(lines.some((line) => line.includes('stale allowlist entry: "Purple"'))).toBe(true)
  })
})

describe('shrinkWrap: the baseline follows the unexplained count', () => {
  it('ratchets down to what is still unexplained, not to the raw divergence count', () => {
    const allowlist: Allowlist = { reviewed: {}, notYetReviewed: { de: 9 } }
    const divergences: DivergenceFinding[] = Array.from({ length: 9 }, (_, index) => ({
      source: `term ${String(index)}`,
      renderings: [
        { value: 'a', keys: ['x.a'] },
        { value: 'b', keys: ['x.b'] },
      ],
    }))
    const lowered = shrinkWrap(
      [{ locale: 'de', isOverlay: false, divergences, unallowed: divergences.slice(2), staleAllows: [], baseline: 9 }],
      allowlist,
      // A path that is never written: `shrinkWrap` only touches disk when it lowers
      // something, and this assertion is about the number it computes.
      '/dev/null',
    )
    expect(lowered).toEqual(['de'])
    expect(allowlist.notYetReviewed.de).toBe(7)
  })
})
