/**
 * The order a finished live search leaves its rows in.
 *
 * The rules mirror the backend's (`ranking::stem_for` / `classify_match`), and the
 * mirror is the point: if the two disagree about which row leads, the same query looks
 * different depending on whether the drive happened to be indexed — which is the whole
 * thing this effort exists to remove.
 */

import { describe, it, expect } from 'vitest'
import type { SearchResultEntry } from '$lib/tauri-commands'
import { rankLiveResults, rankingStem } from './live-ranking'

function entry(name: string, modifiedAt: number | null = 1_700_000_000): SearchResultEntry {
  return {
    name,
    path: `/Users/test/${name}`,
    parentPath: '/Users/test',
    isDirectory: false,
    size: 10,
    modifiedAt,
    iconId: 'ext:txt',
  }
}

const names = (entries: SearchResultEntry[]): string[] => entries.map((e) => e.name)

describe('rankingStem', () => {
  it('is the query itself when it is a plain substring', () => {
    expect(rankingStem('report', 'filename')).toBe('report')
    expect(rankingStem('  report  ', 'filename')).toBe('report')
  })

  it('is empty for anything with no exact-vs-prefix gradient', () => {
    // A wildcard glob, a regex, or nothing typed: every row lands in one band and
    // recency decides, exactly as the backend does it.
    expect(rankingStem('report*', 'filename')).toBe('')
    expect(rankingStem('*.pdf', 'filename')).toBe('')
    expect(rankingStem('re?ort', 'filename')).toBe('')
    expect(rankingStem('^report$', 'regex')).toBe('')
    expect(rankingStem('   ', 'filename')).toBe('')
  })
})

describe('rankLiveResults', () => {
  const opts = { query: 'report', mode: 'filename' as const, caseSensitive: false }

  it('leads with the exact name, then the prefixes, then the rest', () => {
    const ranked = rankLiveResults([entry('quarterly-report.txt'), entry('report-draft.txt'), entry('report')], opts)
    expect(names(ranked)).toEqual(['report', 'report-draft.txt', 'quarterly-report.txt'])
  })

  it('orders newest first inside a band', () => {
    const ranked = rankLiveResults(
      [entry('report-a.txt', 100), entry('report-c.txt', 300), entry('report-b.txt', 200)],
      opts,
    )
    expect(names(ranked)).toEqual(['report-c.txt', 'report-b.txt', 'report-a.txt'])
  })

  it('does not let a row Cmdr cannot date lead on the strength of not knowing', () => {
    const ranked = rankLiveResults([entry('report-undated.txt', null), entry('report-old.txt', 1)], opts)
    expect(names(ranked)).toEqual(['report-old.txt', 'report-undated.txt'])
  })

  it('breaks a full tie by path, so the same set always renders the same way', () => {
    const ranked = rankLiveResults([entry('report-b.txt', 5), entry('report-a.txt', 5)], opts)
    expect(names(ranked)).toEqual(['report-a.txt', 'report-b.txt'])
  })

  it('honors case sensitivity when the user asked for it', () => {
    const insensitive = rankLiveResults([entry('older.txt', 9), entry('REPORT', 1)], opts)
    expect(names(insensitive)[0]).toBe('REPORT')

    const sensitive = rankLiveResults([entry('older.txt', 9), entry('REPORT', 1)], { ...opts, caseSensitive: true })
    expect(names(sensitive)[0]).toBe('older.txt')
  })

  it('falls back to pure recency when the pattern carries no gradient', () => {
    const ranked = rankLiveResults([entry('a.pdf', 1), entry('b.pdf', 3), entry('c.pdf', 2)], {
      query: '*.pdf',
      mode: 'filename',
      caseSensitive: false,
    })
    expect(names(ranked)).toEqual(['b.pdf', 'c.pdf', 'a.pdf'])
  })

  it('leaves the input array alone', () => {
    const input = [entry('quarterly-report.txt'), entry('report')]
    rankLiveResults(input, opts)
    expect(names(input)).toEqual(['quarterly-report.txt', 'report'])
  })
})
