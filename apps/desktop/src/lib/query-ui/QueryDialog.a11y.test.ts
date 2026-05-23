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

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { writable } from 'svelte/store'
import QueryDialog from './QueryDialog.svelte'
import { createQueryFilterState, type QueryFilterState } from './query-filter-state.svelte'
import { createRecentItemsState } from './recent-items/recent-items-state.svelte'
import type { QueryDialogConfig } from './query-dialog-config'
import type { HistoryEntry } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'

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
    maxWidth: 'min(1080px, 80vw)',
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
      searchableFolder: { path: '/Users/test', disabled: false, disabledReason: '' },
      systemDirExcludeTooltip: '',
      aiPattern: null,
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

describe('QueryDialog a11y', () => {
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
