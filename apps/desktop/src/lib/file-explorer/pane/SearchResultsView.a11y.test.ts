/**
 * Tier 3 a11y tests for `SearchResultsView.svelte`.
 *
 * Two interesting states for axe to audit:
 *   1. The defensive "snapshot no longer available" message that renders when
 *      the URL points at an id the store doesn't know about. This is the only
 *      bit of UI the component owns directly — everything else is forwarded
 *      to `FullList`, which has its own tier-3 coverage.
 *   2. A populated render against a stub snapshot. `FullList` mounts under
 *      the hood; we mock the platform-heavy modules the same way the unit
 *      test in `SearchResultsView.svelte.test.ts` does so jsdom stays happy.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResultsView from './SearchResultsView.svelte'
import { _resetForTesting, getOrCreate, type SearchSnapshot } from '$lib/search/snapshot-store.svelte'
import type { SearchResultEntry } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Mirror the platform stubs from `SearchResultsView.svelte.test.ts` so the
// FullList subtree mounts cleanly under jsdom.
vi.mock('$lib/tooltip/tooltip', () => ({
  tooltip: () => ({ destroy() {} }),
}))
vi.mock('$lib/utils/shorten-middle-action', () => ({
  useShortenMiddle: () => ({ destroy() {} }),
}))
vi.mock('$lib/text-size.svelte', () => ({
  getEffectiveScale: () => 1,
  onDebouncedScaleChange: () => () => {},
}))
vi.mock('$lib/tauri-commands', () => ({
  getDirStatsBatch: () => Promise.resolve([]),
  listen: () => Promise.resolve(() => {}),
  showFileContextMenu: () => Promise.resolve(),
}))
vi.mock('$lib/icon-cache', () => ({
  iconCacheCleared: {
    subscribe: (fn: (v: number) => void) => {
      fn(0)
      return () => {}
    },
  },
  iconCacheVersion: {
    subscribe: (fn: (v: number) => void) => {
      fn(0)
      return () => {}
    },
  },
  getCachedIcon: () => null,
  prefetchIcons: () => Promise.resolve(),
}))
vi.mock('$lib/stores/restricted-paths-store.svelte', () => ({
  isRestricted: () => false,
}))
vi.mock('$lib/system-strings.svelte', () => ({
  restrictedFolderTooltip: () => 'restricted',
}))
vi.mock('$lib/indexing/index-state.svelte', () => ({
  isScanning: () => false,
  isAggregating: () => false,
}))
vi.mock('../git/status-column', () => ({
  fetchStatusMap: () => Promise.resolve(null),
  glyphFor: () => '',
  labelFor: () => '',
}))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getRowHeight: () => 24,
  getIconSize: () => 16,
  getIsCompactDensity: () => false,
  formattedDate: () => ({ text: '', segments: [] }),
  formatFileSize: () => '',
  getSizeDisplayMode: () => 'smart',
  getSizeMismatchWarning: () => false,
  getStripedRows: () => false,
  getShowExtensionInName: () => false,
  getShowTags: () => false,
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
  getUseAppIconsForDocuments: () => false,
}))

function makeEntry(name: string): SearchResultEntry {
  return {
    name,
    path: `/Users/test/${name}`,
    parentPath: '/Users/test',
    isDirectory: false,
    size: 100,
    modifiedAt: 1_700_000_000,
    iconId: 'ext:txt',
  }
}

function makeSnapshot(id: string, entries: SearchResultEntry[]): SearchSnapshot {
  return {
    id,
    query: 'foo',
    mode: 'filename',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    entries,
    totalCount: entries.length,
    createdAt: Date.now(),
    label: 'foo',
  }
}

describe('SearchResultsView a11y', () => {
  it('snapshot-missing pane has no a11y violations', async () => {
    _resetForTesting()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResultsView, {
      target,
      props: {
        path: 'search-results://nonexistent',
        cursorIndex: 0,
        isFocused: false,
        sortBy: 'name',
        sortOrder: 'ascending',
        onNavigate: () => {},
        onSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('populated snapshot view has no a11y violations', async () => {
    _resetForTesting()
    const id = 'sr-1'
    getOrCreate(id, makeSnapshot(id, [makeEntry('alpha.txt'), makeEntry('beta.txt')]))
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResultsView, {
      target,
      props: {
        path: `search-results://${id}`,
        cursorIndex: 0,
        isFocused: true,
        sortBy: 'name',
        sortOrder: 'ascending',
        onNavigate: () => {},
        onSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
