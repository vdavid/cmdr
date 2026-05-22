/**
 * Round 2 fixup tests for `SearchResults.svelte` states.
 *
 * Pins:
 * - D1: Searching state renders the project's normal spinner (`.spinner`), not the
 *   glowing-dot pulse. The "Searching..." label sits underneath.
 * - D2: When `isSearching` is true post-debounce, the result list area is REPLACED
 *   by the spinner + label (no rows visible during the active fetch).
 * - D3: Status bar is EMPTY while the content area shows "Searching...".
 * - D4: No-results state: content shows `No files match these criteria:` followed
 *   by a bulleted list of the active criteria. Status bar empty.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResults from './QueryResults.svelte'
import type { SearchResultEntry } from '$lib/tauri-commands'

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => undefined,
    iconCacheVersion: writable(0),
  }
})

vi.mock('$lib/tauri-commands', () => ({
  formatBytes: (n: number) => `${String(n)} B`,
}))

const baseProps = {
  results: [] as SearchResultEntry[],
  cursorIndex: -1,
  isIndexAvailable: true,
  isIndexReady: true,
  isSearching: false,
  hasSearched: false,
  query: '',
  sizeFilter: 'any',
  dateFilter: 'any',
  scanning: false,
  entriesScanned: 0,
  totalCount: 0,
  indexEntryCount: 1000,
  iconCacheVersion: 0,
  aiEnabled: false,
  onResultClick: () => {},
  onHover: () => {},
  onPickExample: () => {},
  onPickPath: () => {},
  onRowMenu: () => {},
}

function mountWith(props: Partial<typeof baseProps>): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SearchResults, { target, props: { ...baseProps, ...props } })
  return target
}

describe('SearchResults round 2 states', () => {
  it('D1: searching state renders the project spinner, not the glowing dot', async () => {
    const target = mountWith({ isSearching: true, hasSearched: true, query: '*.jpg' })
    await tick()
    // Must use the project's standard `.spinner` from app.css.
    expect(target.querySelector('.spinner')).toBeTruthy()
    // Must NOT keep using the old glowing `.loading-pulse` dot.
    expect(target.querySelector('.loading-pulse')).toBeFalsy()
  })

  it('D1: searching state shows the "Searching..." label underneath the spinner', async () => {
    const target = mountWith({ isSearching: true, hasSearched: true, query: '*.jpg' })
    await tick()
    expect(target.textContent).toContain('Searching...')
  })

  it('D2: while isSearching the result rows are NOT rendered', async () => {
    // Provide stale results — these would have been rendered before the new search fired.
    const stale: SearchResultEntry[] = [
      {
        path: '/a.txt',
        name: 'a.txt',
        parentPath: '/',
        isDirectory: false,
        size: 1,
        modifiedAt: 0,
        iconId: 'ext:txt',
      },
    ]
    const target = mountWith({
      isSearching: true,
      hasSearched: true,
      query: '*.jpg',
      results: stale,
      totalCount: 1,
    })
    await tick()
    // No row elements should be present during the active fetch.
    expect(target.querySelector('.result-row')).toBeFalsy()
    // The spinner area takes the place of the list.
    expect(target.querySelector('.spinner')).toBeTruthy()
  })

  it('D3: status bar is empty during isSearching', async () => {
    const target = mountWith({ isSearching: true, hasSearched: true, query: '*.jpg' })
    await tick()
    const status = target.querySelector('.status-bar .status-text')
    expect(status?.textContent ?? '').toBe('')
  })

  it('D4: no-results state renders the bulleted criteria heading', async () => {
    const target = mountWith({
      isSearching: false,
      hasSearched: true,
      query: '*.foobar',
      sizeFilter: 'any',
      dateFilter: 'any',
      results: [],
      totalCount: 0,
    })
    await tick()
    expect(target.textContent).toContain('No files match these criteria')
    // Should render a bulleted list (one <li> per active criterion).
    const items = target.querySelectorAll('.no-results-criteria li')
    expect(items.length).toBeGreaterThan(0)
  })

  it('D4: no-results status bar is empty (was duplicating "No results")', async () => {
    const target = mountWith({
      isSearching: false,
      hasSearched: true,
      query: '*.foobar',
      results: [],
      totalCount: 0,
    })
    await tick()
    const status = target.querySelector('.status-bar .status-text')
    expect(status?.textContent ?? '').toBe('')
  })

  it('D4: criteria list includes the query when a query is set', async () => {
    const target = mountWith({
      isSearching: false,
      hasSearched: true,
      query: '*.foobar',
      results: [],
      totalCount: 0,
    })
    await tick()
    const text = target.querySelector('.no-results-criteria')?.textContent ?? ''
    expect(text).toContain('*.foobar')
  })

  it('D4: criteria list includes a size criterion when one is set', async () => {
    const target = mountWith({
      isSearching: false,
      hasSearched: true,
      query: '',
      sizeFilter: 'gte',
      results: [],
      totalCount: 0,
    })
    await tick()
    const text = target.querySelector('.no-results-criteria')?.textContent ?? ''
    expect(text.toLowerCase()).toContain('size')
  })

  // R4 status-bar dedup: when the result list area shows "Loading drive index...",
  // the status bar must NOT also say "Loading index...". David flagged the duplication
  // and asked that this become the general pattern (content area is the source of truth;
  // status bar stays empty when it would duplicate).
  it('R4: status bar is empty while the content shows "Loading drive index..."', async () => {
    const target = mountWith({
      isIndexAvailable: true,
      isIndexReady: false,
      hasSearched: true,
      query: '*.jpg',
    })
    await tick()
    // Content must show the loading message (sanity check the precondition).
    expect(target.textContent).toContain('Loading drive index')
    // Status bar must be empty.
    const status = target.querySelector('.status-bar .status-text')
    expect(status?.textContent ?? '').toBe('')
  })
})
