/**
 * Tier 3 a11y tests for the query dialog and its results list.
 *
 * `svelte-tests` charges per test FILE, not per test (`docs/testing.md` § "What a
 * test actually costs"), so the two share a file. Their stubs reconcile without a
 * mutable value: `$lib/icon-cache` was already the same shape in both, and
 * `QueryDialog` renders `QueryResults` under the very `$lib/tauri-commands` and
 * `$lib/settings` stubs below, so the results block sees what it always did.
 *
 * The mock-free query-ui components live in `presentational.a11y.test.ts`: these
 * three stubs would apply file-wide, and those components use all three for real.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import { writable } from 'svelte/store'
import QueryDialog from './QueryDialog.svelte'
import SearchResults from './QueryResults.svelte'
import { createQueryFilterState, type QueryFilterState } from './query-filter-state.svelte'
import { createRecentItemsState } from './recent-items/recent-items-state.svelte'
import type { QueryDialogConfig } from './query-dialog-config'
import type { HistoryEntry, SearchResultEntry } from '$lib/tauri-commands'
import { axe, expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'search.autoApply') return true
    return undefined
  }),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
  getCachedIcon: vi.fn(() => undefined),
}))

// Both components share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `QueryDialog.svelte`, the shared orchestrator.
 *
 * Mirrors `lib/search/SearchDialog.a11y.test.ts` but mounts QueryDialog directly with
 * a minimal Search-shaped config. Covers the three macro-states that matter
 * structurally:
 *   - loading (inputs disabled, index not ready)
 *   - default (AI off, index ready)
 *   - AI enabled (cloud provider, index ready)
 *
 * Search's full a11y coverage still lives in the Search wrapper test; this one pins
 * the orchestrator's contract so a regression there doesn't depend on Search's mocks.
 */
describe('QueryDialog a11y', () => {
  interface BuildOpts {
    aiEnabled: boolean
    isIndexReady: boolean
    isIndexAvailable: boolean
    inputsDisabled: boolean
  }

  function buildConfig(opts: BuildOpts, state: QueryFilterState): QueryDialogConfig {
    // historyStore types as `RecentItemsStore<HistoryEntry>`; we widen to the generic's
    // default so the assembled config matches QueryDialog's `<unknown>` parameter
    // (Svelte's `mount()` pins the generic to its default at the call site).
    const historyStore = createRecentItemsState<HistoryEntry>({
      getRecent: () => Promise.resolve([]),
    }) as unknown as QueryDialogConfig['historyStore']
    return {
      title: 'Search',
      dialogType: 'search',
      width: 'min(1080px, 80vw)',
      state,
      aiEnabled: opts.aiEnabled,
      inputsDisabled: opts.inputsDisabled,
      visibleChips: { size: true, date: true, scope: true, pattern: true },
      showPathColumn: true,
      runHintCopy: 'Press Enter to search',
      historyStore,
      recentItems: {
        adapter: (e: unknown) => {
          const entry = e as HistoryEntry
          return {
            label: entry.query,
            tooltip: entry.query,
            mode: entry.mode,
            ageLabel: 'now',
            metaLabel: '12 results',
            ariaLabel: entry.query,
          }
        },
        keyFn: (e: unknown) => (e as HistoryEntry).id,
      },
      emptyState: { examples: [], indexEntryCount: 1234 },
      filterChipsExtras: {
        caseSensitive: false,
        scope: '',
        excludeSystemDirs: true,
        scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
        defaultScope: { path: '/Users/test', label: 'Current folder' },
        systemDirExcludeTooltip: '',
        aiPattern: null,
        aiPatternKind: null,
        onToggleCaseSensitive: () => {},
        onToggleExcludeSystemDirs: () => {},
        onSetScope: () => {},
        onClearAiPattern: () => {},
      },
      scanning: false,
      entriesScanned: 0,
      indexEntryCount: 1234,
      isIndexAvailable: opts.isIndexAvailable,
      isIndexReady: opts.isIndexReady,
      runQuery: () => Promise.resolve({ entries: [], totalCount: 0 }),
      primaryAction: {
        label: 'Show all in main window',
        shortcutHint: '⌥⏎',
        ariaLabel: 'Show all in main window',
        handler: () => {},
      },
      secondaryAction: {
        label: 'Go to file',
        shortcutHint: '⏎',
        ariaLabel: 'Go to file',
        handler: () => {},
      },
      onPickPath: () => {},
      onPickExample: () => {},
      onRowMenu: () => {},
      onActivateRecent: () => {},
      onRemoveRecent: () => {},
      onClose: () => {},
    }
  }

  beforeEach(() => {
    // jsdom doesn't reset between tests; clear body so the previous mount doesn't leak.
    document.body.innerHTML = ''
  })

  it('loading state (inputs disabled, index not ready) has no violations', async () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    const target = document.createElement('div')
    document.body.appendChild(target)
    // Cast widens our HistoryEntry-typed config to the generic's default so the
    // call-site type check passes (see QueryDialog.svelte.test.ts for the same trick).
    mount(QueryDialog, {
      target,
      props: {
        config: buildConfig(
          { aiEnabled: false, isIndexReady: false, isIndexAvailable: false, inputsDisabled: true },
          state,
        ),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('default state (AI off, index ready) has no violations', async () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    const target = document.createElement('div')
    document.body.appendChild(target)
    // Cast widens our HistoryEntry-typed config to the generic's default so the
    // call-site type check passes (see QueryDialog.svelte.test.ts for the same trick).
    mount(QueryDialog, {
      target,
      props: {
        config: buildConfig(
          { aiEnabled: false, isIndexReady: true, isIndexAvailable: true, inputsDisabled: false },
          state,
        ),
      },
    })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('AI enabled state (cloud provider, index ready) has no violations', async () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    const target = document.createElement('div')
    document.body.appendChild(target)
    // Cast widens our HistoryEntry-typed config to the generic's default so the
    // call-site type check passes (see QueryDialog.svelte.test.ts for the same trick).
    mount(QueryDialog, {
      target,
      props: {
        config: buildConfig(
          { aiEnabled: true, isIndexReady: true, isIndexAvailable: true, inputsDisabled: false },
          state,
        ),
      },
    })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SearchResults.svelte`.
 *
 * Column headers + results list with multiple states. We pass plain
 * props for each state (unavailable, index-loading, searching, empty,
 * populated) and stub icon-cache + Tauri command wrappers which the
 * component uses directly.
 */
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

  // Regression: a reopened dialog re-runs (spinner showing) while `results` still holds the
  // persisted prior set. The spinner replaces the rows, so `role="listbox"` must NOT be set
  // (no `option` children = axe `aria-required-children` critical). Pre-fix the role gated on
  // `results.length > 0` alone and tripped here; now it gates on actually-rendered rows.
  it('searching with stale results still present has no a11y violations (no orphan listbox)', async () => {
    const stale: SearchResultEntry[] = [
      {
        name: 'photo1.jpg',
        path: '/Users/test/pictures/photo1.jpg',
        parentPath: '/Users/test/pictures',
        isDirectory: false,
        size: 1_500_000,
        modifiedAt: 1_710_000_000,
        iconId: 'ext:jpg',
      },
    ]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        isSearching: true,
        hasSearched: true,
        query: '*.jpg',
        results: stale,
        totalCount: 1,
      },
    })
    await tick()
    // The container must not claim to be a listbox while it shows the spinner.
    expect(target.querySelector('[role="listbox"]')).toBeNull()
    await expectNoA11yViolations(target)
  })

  // Populated rows are `role="option"` AND contain interactive children (the
  // path-pill `<button>`s). Those are mouse-only and intentionally outside the
  // keyboard Tab order (`tabindex="-1"`); the row itself is the keyboard target.
  // Axe's `nested-interactive` rule flags the structural nesting anyway. We
  // disable that one rule for this state and let every other rule run, so any
  // regression in label, name, or contrast semantics still trips this test.
  // See `lib/query-ui/CLAUDE.md` § "Path pills with overflow collapse" for
  // the design rationale.
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

/**
 * A live run adds three things an audit should see: the phase spinner, the status bar
 * as a progress strip (counters, the walked path, a Stop button), and the throttled
 * live region that had to move off `.status-bar` onto an inner span.
 */
describe('SearchResults a11y: a live run', () => {
  /**
   * The populated-results audit's rule set: `nested-interactive` is off because the
   * path pills inside a row are mouse-only `tabindex="-1"` children of the row, which
   * is itself the keyboard target (see the block comment above).
   */
  async function auditWithRows(target: HTMLElement): Promise<void> {
    const out = await axe.run(target, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'],
      },
      rules: {
        'color-contrast': { enabled: false },
        region: { enabled: false },
        'nested-interactive': { enabled: false },
      },
    })
    expect(out.violations).toEqual([])
  }

  const sampleRows: SearchResultEntry[] = [
    {
      name: 'photo1.jpg',
      path: '/Users/test/pictures/photo1.jpg',
      parentPath: '/Users/test/pictures',
      isDirectory: false,
      size: 1_500_000,
      modifiedAt: 1_710_000_000,
      iconId: 'ext:jpg',
    },
  ]

  const walking = {
    phase: 'walking' as const,
    matchCount: 1234,
    dirsFound: 4312,
    currentPath: '/Volumes/naspi/photos/2019',
    capped: false,
    running: true,
    incomplete: false,
  }

  it('the phase state, with nothing found yet, has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: { ...defaultProps, isSearching: true, hasSearched: true, query: 'report', live: walking },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('rows plus the running progress strip has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        isSearching: true,
        hasSearched: true,
        query: 'report',
        results: sampleRows,
        totalCount: 1234,
        live: walking,
        onStopLive: () => {},
      },
    })
    await tick()
    await auditWithRows(target)
  })

  it('a run that stopped short has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResults, {
      target,
      props: {
        ...defaultProps,
        hasSearched: true,
        query: 'report',
        results: sampleRows,
        totalCount: 40,
        live: { ...walking, running: false, incomplete: true, currentPath: null, matchCount: 40 },
      },
    })
    await tick()
    await auditWithRows(target)
  })
})
