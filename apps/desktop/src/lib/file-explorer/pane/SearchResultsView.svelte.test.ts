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
        onSelect: () => {},
      },
    })
    await tick()
    const missing = target.querySelector('.snapshot-missing')
    expect(missing).not.toBeNull()
    expect(missing?.textContent).toContain('no longer available')
    target.remove()
  })

  it('forwards `selectedIndices` to FullList without crashing (M8d)', async () => {
    const id = 'sr-sel'
    getOrCreate(id, makeSnapshot(id, [makeEntry('a.txt'), makeEntry('b.txt'), makeEntry('c.txt')]))

    const target = document.createElement('div')
    document.body.appendChild(target)
    let selectArgs: [number, boolean | undefined, boolean | undefined] | null = null
    mount(SearchResultsView, {
      target,
      props: {
        path: `search-results://${id}`,
        cursorIndex: 1,
        isFocused: true,
        sortBy: 'name',
        sortOrder: 'ascending',
        // Pre-select the middle row; M8d wires this through to FullList.
        selectedIndices: new Set([1]),
        onNavigate: () => {},
        onSelect: (idx, shiftKey, metaKey) => {
          selectArgs = [idx, shiftKey, metaKey]
        },
      },
    })
    await tick()
    expect(target.querySelector('.snapshot-missing')).toBeNull()
    // Callback wiring sanity: the prop is the same shape FullList already accepts.
    expect(selectArgs).toBeNull()
    target.remove()
  })

  it('exposes findItemIndex, openCursorItem, and isMissing on the public API', async () => {
    const id = 'sr-api'
    getOrCreate(id, makeSnapshot(id, [makeEntry('first.txt'), makeEntry('second.txt')]))

    const target = document.createElement('div')
    document.body.appendChild(target)
    // The `name` field on the adapted entry is now the friendly full path
    // (`~/second.txt`), per search-fixup-brief item 15. We assert against
    // `path` so the test pins navigation routing rather than the display string.
    let navigatedPath: string | null = null
    const component = mount(SearchResultsView, {
      target,
      props: {
        path: `search-results://${id}`,
        cursorIndex: 1,
        isFocused: true,
        sortBy: 'name',
        sortOrder: 'ascending',
        onNavigate: (entry) => {
          navigatedPath = entry.path
        },
        onSelect: () => {},
      },
    })
    await tick()

    // The component's exported API is what FilePane reads via `bind:this`. We
    // mirror that here. `findItemIndex` matches by basename (post-fixup); the
    // adapted `name` field is the friendly full path.
    const api = component as unknown as {
      findItemIndex: (name: string) => number
      openCursorItem: () => void
      isMissing: () => boolean
    }
    expect(api.findItemIndex('second.txt')).toBe(1)
    expect(api.findItemIndex('missing.txt')).toBe(-1)
    expect(api.isMissing()).toBe(false)

    api.openCursorItem()
    expect(navigatedPath).toBe('/Users/test/second.txt')

    target.remove()
  })

  it('reports isMissing() === true when the snapshot lookup fails', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(SearchResultsView, {
      target,
      props: {
        path: 'search-results://not-there',
        cursorIndex: 0,
        isFocused: false,
        sortBy: 'name',
        sortOrder: 'ascending',
        onNavigate: () => {},
        onSelect: () => {},
      },
    })
    await tick()
    const api = component as unknown as { isMissing: () => boolean }
    expect(api.isMissing()).toBe(true)
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
        onSelect: () => {},
      },
    })
    await tick()
    expect(target.querySelector('.snapshot-missing')).not.toBeNull()
    target.remove()
  })
})
