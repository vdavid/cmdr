/**
 * Tests for `QueryResults.svelte` states.
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
  countOnly: false,
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

  it('clears the spinner and restores the status text once isSearching flips off', async () => {
    // The spinner shows for any in-flight fetch — the AI translate round-trip drives the same
    // `isSearching` flag in QueryDialog, so this renderer contract covers both paths.
    const found: SearchResultEntry[] = [
      {
        path: '/a.jpg',
        name: 'a.jpg',
        parentPath: '/',
        isDirectory: false,
        size: 1,
        modifiedAt: 0,
        iconId: 'ext:jpg',
      },
    ]
    const target = mountWith({
      isSearching: false,
      hasSearched: true,
      query: '*.jpg',
      results: found,
      totalCount: 1,
    })
    await tick()
    // Not searching → no spinner, rows render, status bar reports the result count.
    expect(target.querySelector('.spinner')).toBeFalsy()
    expect(target.querySelector('.result-row')).toBeTruthy()
    expect(target.querySelector('.status-bar .status-text')?.textContent ?? '').toContain('1 of 1')
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

describe('SearchResults row rendering (font-bump sizing)', () => {
  function makeResults(n: number): SearchResultEntry[] {
    return Array.from({ length: n }, (_, i) => ({
      path: `/dir/file-${String(i)}.txt`,
      name: `file-${String(i)}.txt`,
      parentPath: '/dir',
      isDirectory: false,
      size: i,
      modifiedAt: 0,
      iconId: 'ext:txt',
    }))
  }

  // The results list is plain DOM (no virtualization: Search caps at 30 rows, Selection
  // lists a single folder), so the dialog's one-step-larger font can't desync from a
  // fixed row-height constant — there is none. This pins the invariant the font bump
  // relies on: every result renders its own `.result-row`, so the rendered count tracks
  // the data exactly at any font size. If someone ever virtualizes this list, they'll
  // have to re-derive the row height for the bumped font, and this count check guards it.
  it('renders one row per result (no windowing, no clipped rows)', async () => {
    const results = makeResults(30)
    const target = mountWith({ results, hasSearched: true, query: '*.txt', totalCount: 30 })
    await tick()
    expect(target.querySelectorAll('.result-row').length).toBe(30)
  })

  // The under-cursor row routes the muted columns (path / size / modified) to
  // `--color-text-primary` for AA contrast on the accent-tinted cursor bg. That CSS
  // hangs off the `is-under-cursor` class, so pin that exactly one row carries it and
  // it's the cursor row.
  it('marks exactly the cursor row with is-under-cursor (drives the AA color override)', async () => {
    const results = makeResults(5)
    const target = mountWith({ results, hasSearched: true, query: '*.txt', totalCount: 5, cursorIndex: 2 })
    await tick()
    const cursorRows = target.querySelectorAll('.result-row.is-under-cursor')
    expect(cursorRows.length).toBe(1)
    expect(cursorRows[0].textContent).toContain('file-2.txt')
  })
})

describe('SearchResults count-only mode', () => {
  it('shows the grouped total plus a pluralized label instead of rows', async () => {
    const target = mountWith({
      countOnly: true,
      hasSearched: true,
      query: '*.jpg',
      totalCount: 12345,
    })
    await tick()
    const summary = target.querySelector('.count-only-summary')
    expect(summary).toBeTruthy()
    expect(summary?.querySelector('.count-only-number')?.textContent).toBe('12,345')
    expect(summary?.textContent).toContain('results')
    // No rows and no listbox role in count-only mode.
    expect(target.querySelectorAll('.result-row').length).toBe(0)
    expect(target.querySelector('[role="listbox"]')).toBeFalsy()
  })

  it('renders a zero-match count (not the no-results criteria list)', async () => {
    const target = mountWith({ countOnly: true, hasSearched: true, query: 'nomatch', totalCount: 0 })
    await tick()
    expect(target.querySelector('.count-only-summary')?.querySelector('.count-only-number')?.textContent).toBe('0')
    expect(target.querySelector('.no-results')).toBeFalsy()
  })

  it('uses the singular label for a count of one', async () => {
    const target = mountWith({ countOnly: true, hasSearched: true, query: 'unique', totalCount: 1 })
    await tick()
    const summary = target.querySelector('.count-only-summary')
    expect(summary?.querySelector('.count-only-number')?.textContent).toBe('1')
    expect(summary?.textContent).toContain('result')
    expect(summary?.textContent).not.toContain('results')
  })

  it('falls through to the empty state before any search runs', async () => {
    const target = mountWith({ countOnly: true, hasSearched: false, query: '' })
    await tick()
    expect(target.querySelector('.count-only-summary')).toBeFalsy()
  })
})
