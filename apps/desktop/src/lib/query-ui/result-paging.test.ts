import { describe, expect, it } from 'vitest'
import type { SearchResultEntry } from '#lib/tauri-commands/index.js'
import { appendUnseen, MAX_LISTED_RESULTS, nextPageLimit, RESULTS_PAGE_STEP } from './result-paging'

function row(path: string): SearchResultEntry {
  return {
    name: path.split('/').at(-1) ?? path,
    path,
    parentPath: '/',
    isDirectory: false,
    size: null,
    modifiedAt: null,
    iconId: 'file',
  }
}

describe('nextPageLimit', () => {
  it('asks for one step more than the list shows', () => {
    expect(nextPageLimit(30)).toBe(30 + RESULTS_PAGE_STEP)
  })

  it('stops at the ceiling', () => {
    expect(nextPageLimit(MAX_LISTED_RESULTS - 10)).toBe(MAX_LISTED_RESULTS)
  })
})

describe('appendUnseen', () => {
  it('keeps the rows on screen in their order and appends only the unseen ones', () => {
    const shown = [row('/b'), row('/a')]
    // A ranked page from the top: it leads with rows already shown, in another order.
    const merged = appendUnseen(shown, [row('/a'), row('/b'), row('/c'), row('/d')])
    expect(merged.map((r) => r.path)).toEqual(['/b', '/a', '/c', '/d'])
  })

  it('never drops a row the page no longer has', () => {
    const merged = appendUnseen([row('/gone'), row('/a')], [row('/a'), row('/c')])
    expect(merged.map((r) => r.path)).toEqual(['/gone', '/a', '/c'])
  })

  it('returns the same array when the page adds nothing', () => {
    const shown = [row('/a')]
    expect(appendUnseen(shown, [row('/a')])).toBe(shown)
    expect(appendUnseen(shown, [])).toBe(shown)
  })

  it('drops a duplicate within the page itself', () => {
    expect(appendUnseen([], [row('/a'), row('/a')]).map((r) => r.path)).toEqual(['/a'])
  })
})
