import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResultsView from './SearchResultsView.svelte'
import {
  _resetForTesting,
  appendSnapshotEntries,
  getOrCreate,
  getSnapshot,
  type SearchSnapshot,
} from '$lib/search/snapshot-store.svelte'
import type { SearchResultEntry } from '$lib/ipc/bindings'
import type { FileEntry, SelectPayload } from '../types'

// FullList depends on a lot of platform-y machinery (canvas measurer, tauri commands).
// We're not exercising its internals here — we just need the wrapper to render and
// expose the snapshot's `entries` to its children. Stub the heaviest internals so the
// test environment stays happy.
/**
 * The live tooltip text per node, so the header's "what does the next click do"
 * wording is assertable. The real action swaps the text on every update, which is
 * exactly the behaviour under test on the third state.
 */

const tooltipTextByNode = new Map<Element, string>()

/** Every tooltip text currently attached to a node in the document. */
function tooltipTexts(): string[] {
  return [...tooltipTextByNode.entries()].filter(([node]) => node.isConnected).map(([, text]) => text)
}

vi.mock('$lib/tooltip/tooltip', () => ({
  tooltip: (node: Element, params: unknown) => {
    const read = (p: unknown): string => (typeof p === 'string' ? p : ((p as { text?: string } | null)?.text ?? ''))
    tooltipTextByNode.set(node, read(params))
    return {
      update(next: unknown) {
        tooltipTextByNode.set(node, read(next))
      },
      destroy() {
        tooltipTextByNode.delete(node)
      },
    }
  },
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

/** The backend comparator behind a header click; each test says what it answers. */
const sortSearchResultsSpy = vi.fn<() => Promise<number[]>>()

vi.mock('$lib/tauri-commands', () => ({
  getDirStatsBatch: () => Promise.resolve([]),
  listen: () => Promise.resolve(() => {}),
  showFileContextMenu: (...args: [string, string, boolean, string[], unknown?]) => showFileContextMenuSpy(...args),
  sortSearchResults: () => sortSearchResultsSpy(),
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
  getDirectorySortMode: () => 'likeFiles',
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
    volumeId: 'root',
    caseSensitive: false,
    excludeSystemDirs: true,
    entries,
    totalCount: entries.length,
    createdAt: Date.now(),
    label: 'foo',
    sort: null,
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

    /** Mounts the view on `id` and returns the target, ready to click headers in. */
    function mountOn(id: string): HTMLElement {
      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(SearchResultsView, {
        target,
        props: {
          path: `search-results://${id}`,
          cursorIndex: 0,
          isFocused: true,
          onNavigate: () => {},
          onSelect: () => {},
        },
      })
      return target
    }

    /** The header button carrying `label`. */
    function header(target: HTMLElement, label: string): HTMLButtonElement {
      const found = [...target.querySelectorAll<HTMLButtonElement>('button.sortable-header')].find(
        (el) => el.querySelector('.label')?.textContent === label,
      )
      if (!found) throw new Error(`no ${label} header`)
      return found
    }

    it('opens in the engine ranked order, with no column claimed and no arrow drawn', async () => {
      const id = 'sr-ranked'
      getOrCreate(id, makeSnapshot(id, [makeEntry('zeta.txt'), makeEntry('alpha.txt'), makeEntry('mid.txt')]))

      const target = mountOn(id)
      await tick()

      expect(rowLabels(target)).toEqual(['~/zeta.txt', '~/alpha.txt', '~/mid.txt'])
      expect(target.querySelector('.sortable-header.is-active')).toBeNull()
      expect(target.querySelector('.sort-indicator:not(.invisible)')).toBeNull()
      // The columns are still named, so the pane reads as a file list.
      const labels = [...target.querySelectorAll('.sortable-header .label')].map((el) => el.textContent)
      expect(labels).toContain('Size')
      target.remove()
    })

    it('re-orders the SNAPSHOT on a header click, so every index the user sees still names its row', async () => {
      const id = 'sr-click'
      getOrCreate(id, makeSnapshot(id, [makeEntry('zeta.txt'), makeEntry('alpha.txt'), makeEntry('mid.txt')]))
      // The backend comparator's answer for name-ascending over that ranked order.
      sortSearchResultsSpy.mockResolvedValue([1, 2, 0])

      const target = mountOn(id)
      await tick()
      header(target, 'Name').click()
      await vi.waitFor(() => {
        expect(getSnapshot(id)?.sort).toEqual({ column: 'name', order: 'ascending' })
      })
      await tick()

      expect(rowLabels(target)).toEqual(['~/alpha.txt', '~/mid.txt', '~/zeta.txt'])
      // The store's array moved with the view: that is the whole invariant.
      expect((getSnapshot(id)?.entries ?? []).map((e) => e.name)).toEqual(['alpha.txt', 'mid.txt', 'zeta.txt'])
      target.remove()
    })

    it('lights the sorted column and draws its direction', async () => {
      const id = 'sr-caret'
      getOrCreate(id, makeSnapshot(id, [makeEntry('b.txt'), makeEntry('a.txt')]))
      sortSearchResultsSpy.mockResolvedValue([1, 0])

      const target = mountOn(id)
      await tick()
      header(target, 'Name').click()
      await vi.waitFor(() => {
        expect(getSnapshot(id)?.sort).not.toBeNull()
      })
      await tick()

      const active = target.querySelector('.sortable-header.is-active')
      expect(active?.querySelector('.label')?.textContent).toBe('Name')
      expect(active?.querySelector('.sort-indicator')?.textContent.trim()).toBe('▲')
      target.remove()
    })

    it('walks the tri-state, ending back in the ranked order the snapshot opened in', async () => {
      const id = 'sr-cycle'
      getOrCreate(id, makeSnapshot(id, [makeEntry('b.txt'), makeEntry('a.txt')]))
      sortSearchResultsSpy.mockResolvedValue([1, 0])

      const target = mountOn(id)
      await tick()

      header(target, 'Name').click()
      await vi.waitFor(() => {
        expect(getSnapshot(id)?.sort).toEqual({ column: 'name', order: 'ascending' })
      })
      await tick()

      sortSearchResultsSpy.mockResolvedValue([0, 1])
      header(target, 'Name').click()
      await vi.waitFor(() => {
        expect(getSnapshot(id)?.sort).toEqual({ column: 'name', order: 'descending' })
      })
      await tick()

      header(target, 'Name').click()
      await vi.waitFor(() => {
        expect(getSnapshot(id)?.sort).toBeNull()
      })
      await tick()

      // Exactly the order the snapshot was created in, not an approximation.
      expect(rowLabels(target)).toEqual(['~/b.txt', '~/a.txt'])
      target.remove()
    })

    it('names the third click "Sort by relevance", because it restores the ranked order', async () => {
      const id = 'sr-tooltip'
      getOrCreate(id, makeSnapshot(id, [makeEntry('b.txt'), makeEntry('a.txt')]))
      sortSearchResultsSpy.mockResolvedValue([1, 0])

      const target = mountOn(id)
      await tick()
      header(target, 'Name').click()
      await vi.waitFor(() => {
        expect(getSnapshot(id)?.sort?.order).toBe('ascending')
      })
      await tick()
      // Still in the column's default order, so the next click flips direction.
      expect(tooltipTexts()).not.toContain('Sort by relevance')

      sortSearchResultsSpy.mockResolvedValue([0, 1])
      header(target, 'Name').click()
      await vi.waitFor(() => {
        expect(getSnapshot(id)?.sort?.order).toBe('descending')
      })
      await tick()

      expect(tooltipTexts()).toContain('Sort by relevance')
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

  it('forwards `selectedIndices` to FullList without crashing', async () => {
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
        // Pre-select the middle row; the view wires this through to FullList.
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
        onNavigate: () => {},
        onSelect: () => {},
      },
    })
    await tick()
    expect(target.querySelector('.snapshot-missing')).not.toBeNull()
    target.remove()
  })
})
