import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResultsView from './SearchResultsView.svelte'
import {
  _resetForTesting,
  appendSnapshotEntries,
  getOrCreate,
  type SearchSnapshot,
} from '$lib/search/snapshot-store.svelte'
import type { SearchResultEntry } from '$lib/ipc/bindings'
import type { FileEntry, SelectPayload } from '../types'

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
/** The Rust menu builder's positional signature, as one rest tuple: `[path, filename, isDirectory, paths, options]`. */
const showFileContextMenuSpy = vi.fn<(...args: [string, string, boolean, string[], unknown?]) => Promise<void>>()

vi.mock('$lib/tauri-commands', () => ({
  getDirStatsBatch: () => Promise.resolve([]),
  listen: () => Promise.resolve(() => {}),
  showFileContextMenu: (...args: [string, string, boolean, string[], unknown?]) => showFileContextMenuSpy(...args),
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
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
  getWalkedGround: () => [],
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
    showFileContextMenuSpy.mockClear()
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

  it('grows when a still-running walk appends to the snapshot it is rendering', async () => {
    // "Open in pane" hands a pane a snapshot the walk is still filling
    // (`search/walk-handoff.svelte.ts`). The rows land through
    // `appendSnapshotEntries`; this is the only tier that can say whether they reach
    // the screen. Found end to end: the pane kept the two rows it opened with while
    // the toast counted up to 24.
    const id = 'sr-growing'
    getOrCreate(id, makeSnapshot(id, [makeEntry('alpha.txt')]))

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
    const rowsAtOpen = target.querySelectorAll('.file-entry').length
    expect(rowsAtOpen).toBeGreaterThan(0)

    appendSnapshotEntries(id, [makeEntry('beta.txt'), makeEntry('gamma.txt')], 3)
    await tick()
    expect(target.querySelectorAll('.file-entry').length).toBe(rowsAtOpen + 2)
    target.remove()
  })

  describe('column sort', () => {
    /** Row labels in render order. The Name column carries the `~`-shortened full path. */
    function rowLabels(target: HTMLElement): string[] {
      return [...target.querySelectorAll('.file-entry')].map((el) => el.getAttribute('data-filename') ?? '')
    }

    it("keeps the snapshot's own order whatever the pane is sorted by", async () => {
      // The pane's `sortBy` / `sortOrder` reach `FullList` but govern nothing here:
      // `staticEntries` are rendered as given, because the search engine
      // already ranked them, and re-ordering them in the view would break the
      // whole subsystem's invariant, that a selected index and `snapshot.entries[i]`
      // name the same row. Every source-side op resolves through that index.
      const id = 'sr-sorted'
      getOrCreate(id, makeSnapshot(id, [makeEntry('zeta.txt'), makeEntry('alpha.txt'), makeEntry('mid.txt')]))

      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(SearchResultsView, {
        target,
        props: {
          path: `search-results://${id}`,
          cursorIndex: 0,
          isFocused: true,
          sortBy: 'size',
          sortOrder: 'descending',
          onNavigate: () => {},
          onSelect: () => {},
        },
      })
      await tick()

      expect(rowLabels(target)).toEqual(['~/zeta.txt', '~/alpha.txt', '~/mid.txt'])
      target.remove()
    })

    it('offers no sort trigger in the header, because a click could not reorder the rows', async () => {
      // The rows come through `staticEntries` in the engine's order, so a sort
      // control here would promise something the pane can't do. The
      // `search-results` capability row turns `sortsRows` off and the header
      // renders plain labels: no button, no click target, nothing to press.
      const id = 'sr-header'
      getOrCreate(id, makeSnapshot(id, [makeEntry('b.txt'), makeEntry('a.txt')]))

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

      const headers = [...target.querySelectorAll<HTMLElement>('.sortable-header')]
      expect(headers.length).toBeGreaterThan(0)
      expect(headers.filter((el) => el.tagName === 'BUTTON')).toEqual([])
      target.remove()
    })

    it('lights no column and shows no arrow, whatever the pane last sorted by', async () => {
      // The user report behind this: a header reading "sorted by size, descending"
      // over a ranked result set that is in neither order. The labels stay so the
      // columns are still named; the claim goes.
      const id = 'sr-indicator'
      getOrCreate(id, makeSnapshot(id, [makeEntry('b.txt'), makeEntry('a.txt')]))

      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(SearchResultsView, {
        target,
        props: {
          path: `search-results://${id}`,
          cursorIndex: 0,
          isFocused: true,
          sortBy: 'size',
          sortOrder: 'descending',
          onNavigate: () => {},
          onSelect: () => {},
        },
      })
      await tick()

      expect(target.querySelector('.sortable-header.is-active')).toBeNull()
      expect(target.querySelector('.sort-indicator')).toBeNull()
      // The column is still named, so the pane reads as a file list.
      const labels = [...target.querySelectorAll('.sortable-header .label')].map((el) => el.textContent)
      expect(labels).toContain('Size')
      target.remove()
    })
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
    let selectArgs: SelectPayload | null = null
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
        onSelect: (args: SelectPayload) => {
          selectArgs = args
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
    // The `name` field on the adapted entry is the friendly full path
    // (`~/second.txt`). We assert against `path` so the test pins navigation
    // routing rather than the display string.
    let navigatedPath: string | null = null
    const component = mount(SearchResultsView, {
      target,
      props: {
        path: `search-results://${id}`,
        cursorIndex: 1,
        isFocused: true,
        sortBy: 'name',
        sortOrder: 'ascending',
        onNavigate: (entry: FileEntry) => {
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

  describe('right-click acts on the whole selection', () => {
    /** Mounts a three-row snapshot pane with `selected` pre-selected and returns its rows. */
    async function mountRows(id: string, selected: Set<number>) {
      getOrCreate(id, makeSnapshot(id, [makeEntry('a.txt'), makeEntry('b.txt'), makeEntry('c.txt')]))
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
          selectedIndices: selected,
          onNavigate: () => {},
          onSelect: () => {},
        },
      })
      await tick()
      return { target, rows: target.querySelectorAll('.file-entry') }
    }

    function rightClick(row: Element) {
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    }

    it('hands the menu every selected path when the clicked row is part of the selection', async () => {
      // Finder's rule, and the one `pane-pointer::handleContextMenu` follows on a
      // normal pane: Delete / Copy / Move from the menu act on what is selected.
      const { target, rows } = await mountRows('sr-menu-selection', new Set([0, 2]))

      rightClick(rows[0])

      expect(showFileContextMenuSpy).toHaveBeenCalledTimes(1)
      expect(showFileContextMenuSpy.mock.calls[0][3]).toEqual(['/Users/test/a.txt', '/Users/test/c.txt'])
      target.remove()
    })

    it('acts on the clicked row alone when it sits outside the selection', async () => {
      const { target, rows } = await mountRows('sr-menu-outside', new Set([0, 2]))

      rightClick(rows[1])

      expect(showFileContextMenuSpy.mock.calls[0][3]).toEqual(['/Users/test/b.txt'])
      target.remove()
    })

    it('labels the menu with the basename, not the friendly full path shown in the Name column', async () => {
      const { target, rows } = await mountRows('sr-menu-label', new Set())

      rightClick(rows[1])

      expect(showFileContextMenuSpy.mock.calls[0][0]).toBe('/Users/test/b.txt')
      expect(showFileContextMenuSpy.mock.calls[0][1]).toBe('b.txt')
      expect(showFileContextMenuSpy.mock.calls[0][3]).toEqual(['/Users/test/b.txt'])
      target.remove()
    })
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
