/**
 * What a live run looks like: the three phases, rows that stay visible while more
 * arrive, the status bar as a progress strip, and count-only telling the truth about a
 * number that's still rising.
 *
 * The regression this file guards is the one the old spinner logic causes: `isSearching`
 * is TRUE for a live run's whole life, and the old rule replaced the whole list with
 * a spinner whenever it was. Under a walk that runs for minutes, that would hide every
 * row the walk found until it finished — which is the streaming UI not existing.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResults from './QueryResults.svelte'
import type { SearchResultEntry } from '$lib/tauri-commands'
import type { LiveRunView } from './query-stream'

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => undefined,
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
  }
})

vi.mock('$lib/tauri-commands', () => ({}))

function rows(n: number): SearchResultEntry[] {
  return Array.from({ length: n }, (_, i) => ({
    name: `file-${String(i)}.txt`,
    path: `/Users/test/file-${String(i)}.txt`,
    parentPath: '/Users/test',
    isDirectory: false,
    size: 10,
    modifiedAt: 1_700_000_000,
    iconId: 'ext:txt',
  }))
}

function liveView(overrides: Partial<LiveRunView> = {}): LiveRunView {
  return {
    phase: 'walking',
    matchCount: 0,
    dirsFound: 0,
    currentPath: null,
    capped: false,
    phaseSince: 0,
    running: true,
    incomplete: false,
    ...overrides,
  }
}

const baseProps = {
  results: [] as SearchResultEntry[],
  cursorIndex: 0,
  isIndexAvailable: true,
  isIndexReady: true,
  isSearching: true,
  hasSearched: true,
  query: 'report',
  sizeFilter: 'any',
  dateFilter: 'any',
  scanning: false,
  entriesScanned: 0,
  totalCount: 0,
  indexEntryCount: 1000,
  countOnly: false,
  showPathColumn: true,
  onShowResults: undefined as (() => void) | undefined,
  live: null as LiveRunView | null,
  onStopLive: undefined as (() => void) | undefined,
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

const statusText = (t: HTMLElement): string =>
  (t.querySelector('.status-bar')?.textContent ?? '').replace(/\s+/g, ' ').trim()

describe('the four phases, while there is nothing to show yet', () => {
  it('names each wait rather than showing one anonymous spinner', async () => {
    const labels = ['resolvingCoverage', 'waitingForAnotherWalk', 'readingIndex', 'walking'] as const
    const seen = new Set<string>()
    for (const phase of labels) {
      const target = mountWith({ live: liveView({ phase }) })
      await tick()
      expect(target.querySelector('.spinner')).toBeTruthy()
      const label = target.querySelector('.loading-label')?.textContent ?? ''
      expect(label).not.toBe('')
      seen.add(label)
    }
    expect(seen.size).toBe(4)
  })

  it('says the walking phase is about folders that are not indexed', async () => {
    const target = mountWith({ live: liveView({ phase: 'walking' }) })
    await tick()
    expect(target.querySelector('.loading-label')?.textContent).toContain("aren't indexed yet")
  })

  it('does not promise a walk for a run that is queued behind another one', async () => {
    // The run holds no ground: no folder of its own is being read. Saying otherwise is
    // what put "0 folders scanned" under a sentence about walking, for as long as the
    // other walk took.
    const target = mountWith({ live: liveView({ phase: 'waitingForAnotherWalk' }) })
    await tick()
    const label = target.querySelector('.loading-label')?.textContent ?? ''
    expect(label).not.toBe('')
    expect(label).not.toContain("aren't indexed yet")
    expect(statusText(target)).not.toContain('folders scanned')
  })
})

describe('rows stay on screen while the run keeps finding more', () => {
  it('renders the rows found so far instead of replacing them with a spinner', async () => {
    // `isSearching` is true for the whole live run. The old spinner rule emptied the list.
    const target = mountWith({ results: rows(3), totalCount: 3, live: liveView({ matchCount: 3 }) })
    await tick()
    expect(target.querySelectorAll('.result-row')).toHaveLength(3)
    expect(target.querySelector('.loading-label')).toBeFalsy()
    // The listbox role is back on, which needs its option children to exist.
    expect(target.querySelector('[role="listbox"]')).toBeTruthy()
  })

  it('counts up in the status bar without ever claiming a total', async () => {
    const target = mountWith({
      results: rows(3),
      totalCount: 1234,
      live: liveView({ matchCount: 1234, dirsFound: 4312, currentPath: '/Volumes/naspi/photos' }),
    })
    await tick()
    const status = statusText(target)
    expect(status).toContain('1,234 matches so far')
    expect(status).toContain('4,312 folders scanned')
    expect(status).not.toContain('of 1,234 results')
  })

  it('shows where the walk has got to, named for a screen reader', async () => {
    // The path is rendered by the mid-truncating action, so the element is empty in
    // the markup and filled at runtime. Pinning that it ends up with text is what
    // catches a wiring change that leaves a blank strip where progress should be.
    const target = mountWith({
      results: rows(1),
      live: liveView({ currentPath: '/Volumes/naspi/photos/2019' }),
    })
    await tick()
    const path = target.querySelector('.status-path')
    expect(path?.textContent ?? '').toContain('naspi')
    expect(path?.getAttribute('aria-label')).toContain('/Volumes/naspi/photos/2019')
  })

  it('offers a way out, with the key that does the same thing', async () => {
    const stop = vi.fn()
    const target = mountWith({ results: rows(1), live: liveView(), onStopLive: stop })
    await tick()
    const button = target.querySelector<HTMLButtonElement>('.status-stop button')
    expect(button).toBeTruthy()
    expect(button?.textContent).toContain('Stop')
    expect(button?.textContent).toContain('Esc')
    button?.click()
    expect(stop).toHaveBeenCalledTimes(1)
  })

  it('takes the Stop button away once there is nothing to stop', async () => {
    const target = mountWith({
      results: rows(1),
      isSearching: false,
      live: liveView({ running: false }),
      onStopLive: () => {},
    })
    await tick()
    expect(target.querySelector('.status-stop')).toBeFalsy()
  })
})

describe('how a live run ends', () => {
  it('keeps the rows and says the list is short when the run stopped early', async () => {
    const target = mountWith({
      results: rows(12),
      isSearching: false,
      totalCount: 40,
      live: liveView({ running: false, incomplete: true, matchCount: 40 }),
    })
    await tick()
    expect(target.querySelectorAll('.result-row')).toHaveLength(12)
    expect(statusText(target)).toContain("Cmdr didn't finish looking")
  })

  it('falls back to the ordinary result line when the run covered its ground', async () => {
    const target = mountWith({
      results: rows(3),
      isSearching: false,
      totalCount: 3,
      live: liveView({ running: false, matchCount: 3 }),
    })
    await tick()
    expect(statusText(target)).toContain('3 of 3 results')
  })

  it('says the rows stopped at the cap while the count carried on past it', async () => {
    const target = mountWith({
      results: rows(3),
      isSearching: false,
      totalCount: 5000,
      live: liveView({ running: false, capped: true, matchCount: 5000 }),
    })
    await tick()
    expect(statusText(target)).toContain('Showing the first 3 of 5,000 matches')
  })
})

describe('count-only stops claiming a total it does not have', () => {
  it('says "so far" while the run is still counting', async () => {
    const target = mountWith({
      countOnly: true,
      totalCount: 812,
      live: liveView({ phase: 'walking', matchCount: 812 }),
    })
    await tick()
    const summary = target.querySelector('.count-only-summary')?.textContent ?? ''
    expect(summary).toContain('so far')
    expect(summary).toContain('812')
  })

  it('keeps saying "so far" after a run that ended short, because the count is a lower bound', async () => {
    const target = mountWith({
      countOnly: true,
      isSearching: false,
      totalCount: 812,
      live: liveView({ running: false, incomplete: true, matchCount: 812 }),
    })
    await tick()
    expect(target.querySelector('.count-only-summary')?.textContent).toContain('so far')
  })

  it('states the exact total once the run covered its ground', async () => {
    const target = mountWith({
      countOnly: true,
      isSearching: false,
      totalCount: 812,
      live: liveView({ running: false, matchCount: 812 }),
    })
    await tick()
    const summary = target.querySelector('.count-only-summary')?.textContent ?? ''
    expect(summary).toContain('This search yields')
    expect(summary).not.toContain('so far')
  })

  it('waits for the run to have ground of its own before showing a count at all', async () => {
    // "0 results so far" while Cmdr is still working out what it covers is noise, not
    // information — and so is the same zero while the run is queued behind somebody
    // else's walk, having counted nothing yet either.
    for (const phase of ['resolvingCoverage', 'waitingForAnotherWalk'] as const) {
      const target = mountWith({ countOnly: true, live: liveView({ phase }) })
      await tick()
      expect(target.querySelector('.count-only-summary')).toBeFalsy()
      expect(target.querySelector('.spinner')).toBeTruthy()
    }
  })
})

describe('the announcement a screen reader hears', () => {
  it('is a throttled copy, not the counter itself', async () => {
    // The visible number moves ten times a second; the live region must not. The rule
    // itself is pinned in `query-stream.test.ts`; what's pinned here is that the region
    // is a separate node, so the two can differ at all.
    const target = mountWith({ results: rows(2), live: liveView({ matchCount: 2 }) })
    await tick()
    const region = target.querySelector('.status-bar [aria-live="polite"]')
    expect(region).toBeTruthy()
    expect(target.querySelector('.status-bar')?.getAttribute('aria-live')).toBeNull()
  })
})
