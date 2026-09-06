/**
 * End-to-end wiring for the Name column's shrink-wrap in `QueryResults.svelte`.
 *
 * The math itself is pinned in `name-column-width.test.ts`; this file pins that the
 * component actually reaches it and writes ONE template onto both grid containers. jsdom
 * has no Canvas 2D, so `@chenglou/pretext` is stubbed with a 10px-per-character font —
 * which is also why the visible-range math degrades to "the whole list" here (`clientHeight`
 * and `getBoundingClientRect()` are 0), leaving the widest-name behavior as what we assert.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResults from './QueryResults.svelte'
import type { SearchResultEntry } from '$lib/tauri-commands'

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => undefined,
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
  }
})

vi.mock('$lib/tauri-commands', () => ({}))

const CHAR_PX = 10

vi.mock('@chenglou/pretext', () => ({
  prepareWithSegments: (text: string) => ({ text }),
  measureNaturalWidth: (prepared: { text: string }) => prepared.text.length * CHAR_PX,
}))

const baseProps = {
  results: [] as SearchResultEntry[],
  cursorIndex: -1,
  isIndexAvailable: true,
  isIndexReady: true,
  isSearching: false,
  hasSearched: true,
  query: '*',
  sizeFilter: 'any',
  dateFilter: 'any',
  scanning: false,
  entriesScanned: 0,
  totalCount: 0,
  indexEntryCount: 1000,
  countOnly: false,
  showPathColumn: true,
  onShowResults: undefined as (() => void) | undefined,
  iconCacheVersion: 0,
  aiEnabled: false,
  onResultClick: () => {},
  onHover: () => {},
  onPickExample: () => {},
  onPickPath: () => {},
  onRowMenu: () => {},
}

function entry(name: string, dir = '/opt/homebrew/lib/python3.13'): SearchResultEntry {
  return {
    path: `${dir}/${name}`,
    name,
    parentPath: dir,
    isDirectory: false,
    size: 1,
    modifiedAt: 0,
    iconId: 'ext:txt',
  }
}

/** Mounts, then lets the dynamic `import('@chenglou/pretext')` and its effect settle. */
async function mountAndSettle(results: SearchResultEntry[]): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SearchResults, { target, props: { ...baseProps, results, totalCount: results.length } })
  for (let i = 0; i < 6; i++) {
    await tick()
    await Promise.resolve()
  }
  return target
}

function nameTrack(target: HTMLDivElement): string {
  const header = target.querySelector<HTMLElement>('.column-header')
  // `24px <name> minmax(120px, 1fr) 10ch 16ch`
  return header?.style.gridTemplateColumns.split(' ')[1] ?? ''
}

describe('QueryResults Name column shrink-wrap', () => {
  it('narrows the track to the rows on screen instead of reserving 22ch', async () => {
    // David's case: every row is the word "test", and Path was mid-truncating anyway.
    const target = await mountAndSettle([
      entry('test', '/opt/homebrew/lib/python3.13'),
      entry('test', '/opt/homebrew/lib/python3.13/site-packages'),
      entry('test', '/usr/local/lib'),
    ])
    const track = nameTrack(target)
    expect(track).toMatch(/^\d+px$/)
    // The 22ch ceiling would be 220px with this font; the floor is 80px.
    expect(parseInt(track, 10)).toBe(80)
  })

  it('widens for a longer name, and still caps at 22ch', async () => {
    const medium = await mountAndSettle([entry('quarterly-report.pdf')])
    expect(parseInt(nameTrack(medium), 10)).toBe('quarterly-report.pdf'.length * CHAR_PX + 2)

    const huge = await mountAndSettle([entry('a-really-very-extremely-long-file-name.tar.gz')])
    expect(parseInt(nameTrack(huge), 10)).toBe(22 * CHAR_PX)
  })

  it('keeps the header and the rows on the same measured template', async () => {
    const target = await mountAndSettle([entry('one.txt'), entry('two-longer-name.txt')])
    const header = target.querySelector<HTMLElement>('.column-header')
    const rows = target.querySelectorAll<HTMLElement>('.result-row')
    expect(rows.length).toBe(2)
    for (const row of rows) {
      expect(row.style.gridTemplateColumns).toBe(header?.style.gridTemplateColumns)
    }
  })
})
