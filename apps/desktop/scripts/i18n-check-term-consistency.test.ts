/**
 * Tests for the TERM CONSISTENCY check (`i18n-check-term-consistency.ts`).
 *
 * The classifier is pure, so most of this exercises `findDivergences` directly:
 * two keys carrying the SAME English must render the same way in one locale, and
 * an OVERLAY is judged on what it actually renders (its own value where it forks,
 * the base value where it doesn't), which is what catches a half-forked term.
 */
import { describe, it, expect } from 'vitest'
import { findDivergences, normalizeForComparison, isAllowed } from './i18n-check-term-consistency.ts'
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
  // `source` is what the overlay renders on top of (en, layered) — the two keys share one English value.
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
