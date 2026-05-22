import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResultsView from './SearchResultsView.svelte'
import { _resetForTesting, getOrCreate, type SearchSnapshot } from '$lib/search/snapshot-store.svelte'
import type { SearchResultEntry } from '$lib/ipc/bindings'

// FullList depends on a lot of platform-y machinery (canvas measurer, tauri commands).
// We're not exercising its internals here — we just need the wrapper to render and
// expose the snapshot's `entries` to its children. Stub the heaviest internals so the
// test environment stays happy.
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
  formattedDate: () => ({ text: '', parts: { left: [], right: null } }),
  formatFileSize: () => '',
  getSizeDisplayMode: () => 'smart',
  getSizeMismatchWarning: () => false,
  getStripedRows: () => false,
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
  getUseAppIconsForDocuments: () => false,
}))

function makeEntry(name: string, parentPath = '/Users/test'): SearchResultEntry {
  return {
    name,
    path: `${parentPath}/${name}`,
    parentPath,
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

describe('SearchResultsView', () => {
  beforeEach(() => {
    _resetForTesting()
  })

  it('renders rows from a stored snapshot', async () => {
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
        onNavigateToAncestor: () => {},
        onSelect: () => {},
      },
    })
    await tick()
    // The snapshot-missing pane shouldn't appear when the id resolves.
    expect(target.querySelector('.snapshot-missing')).toBeNull()
    target.remove()
  })

  it('renders the friendly missing-snapshot pane when the id does not resolve', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResultsView, {
      target,
      props: {
        path: 'search-results://nonexistent-id',
        cursorIndex: 0,
        isFocused: false,
        sortBy: 'name',
        sortOrder: 'ascending',
        onNavigate: () => {},
        onNavigateToAncestor: () => {},
        onSelect: () => {},
      },
    })
    await tick()
    const missing = target.querySelector('.snapshot-missing')
    expect(missing).not.toBeNull()
    expect(missing?.textContent).toContain('no longer available')
    target.remove()
  })

  it('renders nothing usable when the path is malformed (no prefix)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchResultsView, {
      target,
      props: {
        // Wrong prefix: SearchResultsView extracts null and treats it as missing.
        path: '/not/a/snapshot/url',
        cursorIndex: 0,
        isFocused: false,
        sortBy: 'name',
        sortOrder: 'ascending',
        onNavigate: () => {},
        onNavigateToAncestor: () => {},
        onSelect: () => {},
      },
    })
    await tick()
    expect(target.querySelector('.snapshot-missing')).not.toBeNull()
    target.remove()
  })
})
