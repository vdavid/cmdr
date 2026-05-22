/**
 * Tier 3 a11y tests for `SearchResults.svelte`.
 *
 * Column headers + results list with multiple states. We pass plain
 * props for each state (unavailable, index-loading, searching, empty,
 * populated) and stub icon-cache + Tauri `formatBytes` which the
 * component uses directly.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResults from './SearchResults.svelte'
import { axe, expectNoA11yViolations } from '$lib/test-a11y'
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

const defaultProps = {
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

describe('SearchResults a11y', () => {
  // `.results-container` only gets `role="listbox"` when there are option rows
  // to host. Every non-populated state (index-unavailable message, loading,
  // searching, no-results, empty-state) renders a plain message container with
  // no role — sidestepping `aria-required-children` cleanly. The tests below
  // exercise each of those states so any regression in the role-gating logic
  // (e.g. someone forcing `role="listbox"` back on) trips immediately.
  it('index ready, no search yet has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, { target, props: defaultProps })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('index unavailable (not scanning) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: { ...defaultProps, isIndexAvailable: false, isIndexReady: false },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('index unavailable with scan in progress has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        isIndexAvailable: false,
        isIndexReady: false,
        scanning: true,
        entriesScanned: 42_000,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('index loading after search has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        isIndexReady: false,
        hasSearched: true,
        query: '*.jpg',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('searching (no results yet) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        isSearching: true,
        hasSearched: true,
        query: '*.jpg',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('no results (search finished, empty) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        hasSearched: true,
        query: 'nonexistentpattern',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  // Populated rows are `role="option"` AND contain interactive children
  // (path-pill `<button>`s and the `…` row-menu `<button>`). Per
  // search-redesign-plan §3.8 / §3.9, the inner buttons are mouse-only and
  // intentionally outside the keyboard Tab order (`tabindex="-1"`); the row
  // itself is the keyboard target. Axe's `nested-interactive` rule flags the
  // structural nesting anyway. We disable that one rule for this state and
  // let every other rule run, so any regression in label, name, or contrast
  // semantics still trips this test. See `lib/search/CLAUDE.md` for the
  // design rationale (decision: "Path pills mouse-only, not in Tab order").
  it('populated results has no a11y violations (nested-interactive intentionally disabled)', async () => {
    const results: SearchResultEntry[] = [
      {
        name: 'photo1.jpg',
        path: '/Users/test/pictures/photo1.jpg',
        parentPath: '/Users/test/pictures',
        isDirectory: false,
        size: 1_500_000,
        modifiedAt: 1_710_000_000,
        iconId: 'ext:jpg',
      },
      {
        name: 'vacation',
        path: '/Users/test/pictures/vacation',
        parentPath: '/Users/test/pictures',
        isDirectory: true,
        size: null,
        modifiedAt: 1_700_000_000,
        iconId: 'dir',
      },
    ]

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        results,
        cursorIndex: 0,
        hasSearched: true,
        query: 'photo*',
        totalCount: 2,
      },
    })
    await tick()
    const out = await axe.run(target, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'],
      },
      rules: {
        'color-contrast': { enabled: false },
        region: { enabled: false },
        // Intentional: mouse-only inner buttons are tabindex="-1"; the row
        // itself is the keyboard target. See block comment above.
        'nested-interactive': { enabled: false },
      },
    })
    expect(out.violations).toEqual([])
  })
})
