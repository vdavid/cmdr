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
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
  }
})

vi.mock('$lib/tauri-commands', () => ({}))

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

  // An empty status bar still drew its top border and padding, so a running search ended
  // the results well in a bordered strip with nothing in it. The bar stays in the DOM (the
  // `aria-live` region has to survive the change) and collapses via `.is-empty` instead.
  it('collapses the whole status bar, not just its text, while a search runs', async () => {
    const target = mountWith({ isSearching: true, hasSearched: true, query: '*.jpg' })
    await tick()
    const bar = target.querySelector('.status-bar')
    expect(bar).toBeTruthy()
    expect(bar?.classList.contains('is-empty')).toBe(true)
    // Still announceable: the live region is present, just collapsed. It's the INNER
    // span, because a live run's counters move ten times a second and only a throttled
    // copy of them may reach a screen reader (`query-stream.ts`).
    expect(bar?.querySelector('[aria-live="polite"]')).toBeTruthy()
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
    // With something to report, the bar is back to its normal boxed self.
    expect(target.querySelector('.status-bar')?.classList.contains('is-empty')).toBe(false)
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

/**
 * The header and the rows are two independent grid containers, so the only thing keeping
 * their columns aligned is that they're handed the SAME `grid-template-columns` string.
 * (`ch` tracks resolving against two different font sizes is exactly how they drifted
 * before.) These pin the shared string and the Selection variant.
 */
describe('SearchResults column tracks', () => {
  const oneRow: SearchResultEntry[] = [
    {
      path: '/dir/a.jpg',
      name: 'a.jpg',
      parentPath: '/dir',
      isDirectory: false,
      size: 1,
      modifiedAt: 0,
      iconId: 'ext:jpg',
    },
  ]

  it('hands the header and every row one identical grid template', async () => {
    const target = mountWith({ results: oneRow, hasSearched: true, query: '*.jpg', totalCount: 1 })
    await tick()
    const header = target.querySelector<HTMLElement>('.column-header')
    const row = target.querySelector<HTMLElement>('.result-row')
    expect(header?.style.gridTemplateColumns).toBeTruthy()
    expect(row?.style.gridTemplateColumns).toBe(header?.style.gridTemplateColumns)
  })

  it('falls back to the fixed Name track when text measurement is unavailable', async () => {
    // jsdom has no Canvas 2D, so pretext never adopts: the pre-measurement CSS fallback
    // (identical to the fixed track this replaced) has to render rather than a broken value.
    const target = mountWith({ results: oneRow, hasSearched: true, query: '*.jpg', totalCount: 1 })
    await tick()
    const header = target.querySelector<HTMLElement>('.column-header')
    expect(header?.style.gridTemplateColumns).toContain('minmax(80px, 22ch)')
  })

  it('gives the Name column the flex track when there is no Path column (Selection)', async () => {
    // Nothing to hand freed width to, so Name absorbs it instead of shrink-wrapping.
    const target = mountWith({
      results: oneRow,
      hasSearched: true,
      query: '*.jpg',
      totalCount: 1,
      showPathColumn: false,
    })
    await tick()
    const header = target.querySelector<HTMLElement>('.column-header')
    expect(header?.style.gridTemplateColumns).toBe('24px minmax(80px, 1fr) 10ch 16ch')
    expect(header?.querySelectorAll('.col-label').length).toBe(4)
  })
})

describe('SearchResults count-only mode', () => {
  it('reads as one sentence with only the grouped total in bold', async () => {
    const target = mountWith({
      countOnly: true,
      hasSearched: true,
      query: '*.jpg',
      totalCount: 12345,
    })
    await tick()
    const summary = target.querySelector('.count-only-summary')
    expect(summary).toBeTruthy()
    // The number is bold and thousands-separated; the rest is a normal sentence around it.
    expect(summary?.querySelector('strong.count-only-number')?.textContent).toBe('12,345')
    expect(summary?.querySelector('.count-only-sentence')?.textContent).toBe('This search yields 12,345 results')
    // No rows and no listbox role in count-only mode.
    expect(target.querySelectorAll('.result-row').length).toBe(0)
    expect(target.querySelector('[role="listbox"]')).toBeFalsy()
  })

  it('renders a zero-match count (not the no-results criteria list)', async () => {
    const target = mountWith({ countOnly: true, hasSearched: true, query: 'nomatch', totalCount: 0 })
    await tick()
    expect(target.querySelector('.count-only-summary strong.count-only-number')?.textContent).toBe('0')
    expect(target.querySelector('.no-results')).toBeFalsy()
  })

  it('uses the singular sentence for a count of one', async () => {
    const target = mountWith({ countOnly: true, hasSearched: true, query: 'unique', totalCount: 1 })
    await tick()
    const sentence = target.querySelector('.count-only-sentence')
    expect(sentence?.textContent).toBe('This search yields 1 result')
  })

  it('falls through to the empty state before any search runs', async () => {
    const target = mountWith({ countOnly: true, hasSearched: false, query: '' })
    await tick()
    expect(target.querySelector('.count-only-summary')).toBeFalsy()
  })

  it('offers "Show results" only when the consumer wires onShowResults', async () => {
    const withoutHandler = mountWith({ countOnly: true, hasSearched: true, query: '*.jpg', totalCount: 3 })
    await tick()
    expect(withoutHandler.querySelector('.count-only-summary button')).toBeFalsy()

    const onShowResults = vi.fn()
    const withHandler = mountWith({
      countOnly: true,
      hasSearched: true,
      query: '*.jpg',
      totalCount: 3,
      onShowResults,
    })
    await tick()
    const button = withHandler.querySelector<HTMLButtonElement>('.count-only-summary button')
    expect(button?.textContent.trim()).toBe('Show results')
    button?.click()
    expect(onShowResults).toHaveBeenCalledOnce()
  })
})

// Column labels over a spinner, a criteria list, the empty state, or a bare total describe a
// table that isn't rendered — and they're the loudest thing in an otherwise quiet area. The
// header now tracks the rows exactly.
describe('SearchResults column header visibility', () => {
  const oneRow: SearchResultEntry[] = [
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

  it('renders the header when rows render', async () => {
    const target = mountWith({ results: oneRow, hasSearched: true, query: '*.jpg', totalCount: 1 })
    await tick()
    expect(target.querySelector('.column-header')).toBeTruthy()
  })

  const headerlessStates: [string, Partial<typeof baseProps>][] = [
    ['count-only', { countOnly: true, hasSearched: true, query: '*.jpg', totalCount: 9 }],
    ['searching', { isSearching: true, hasSearched: true, query: '*.jpg', results: oneRow }],
    ['no results', { hasSearched: true, query: '*.nope', results: [], totalCount: 0 }],
    ['empty state', { hasSearched: false, query: '' }],
    ['index unavailable', { isIndexAvailable: false, isIndexReady: false }],
  ]

  it.each(headerlessStates)('hides the header in the %s state', async (_name, props) => {
    const target = mountWith(props)
    await tick()
    expect(target.querySelector('.column-header')).toBeFalsy()
  })
})
