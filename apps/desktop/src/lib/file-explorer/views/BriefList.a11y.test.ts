/**
 * Tier 3 a11y tests for `BriefList.svelte`.
 *
 * Virtual-scrolling horizontal file list. We stub Tauri IPC, reactive
 * settings, and the icon cache so the component mounts with no real
 * backend. Tests cover the empty-folder overlay and a populated list
 * with mixed entries (folder + file) visible after an await.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import BriefList from './BriefList.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { FileEntry, SortColumn } from '../types'

vi.mock('#lib/tauri-commands/index.js', () => ({
  getFileRange: vi.fn(() => Promise.resolve([] as FileEntry[])),
  getDirStatsBatch: vi.fn(() => Promise.resolve({})),
}))

vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => undefined,
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
    iconCacheCleared: writable(0),
    prefetchIcons: vi.fn(),
  }
})

vi.mock('#lib/indexing/index-state.svelte.js', () => ({
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
  getWalkedGround: () => [],
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getRowHeight: () => 20,
  getIsCompactDensity: () => false,
  getIsCmdrGold: () => false,
  getUseAppIconsForDocuments: () => true,
  formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
  formatFileSize: (n: number) => `${String(n)} B`,
  getSizeDisplayMode: () => 'smart',
  getSizeMismatchWarning: () => false,
  getStripedRows: () => false,
  getShowTags: () => false,
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
  getBriefColumnWidthMode: () => 'paneWidth',
  getBriefColumnWidthMaxPx: () => 400,
}))

vi.mock('#lib/settings/settings-store.js', () => ({
  getSetting: (key: string) => {
    if (key === 'advanced.virtualizationBufferColumns') return 2
    return undefined
  },
}))

describe('BriefList a11y', () => {
  // Pins the `aria-activedescendant` gate: the cursor exists but no row is
  // rendered, so the attribute must be absent rather than name a missing id.
  it('empty folder with a cursor has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l1',
        volumeId: 'root',
        totalCount: 0,
        includeHidden: false,
        cursorIndex: 0,
        isFocused: true,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        effectiveSortBy: 'name',
        onSelect: () => {},
        onNavigate: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a folder holding only hidden items says so, beside the listbox, with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l1',
        volumeId: 'root',
        totalCount: 0,
        hiddenCount: 1,
        includeHidden: false,
        cursorIndex: -1,
        isFocused: true,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        effectiveSortBy: 'name',
        onSelect: () => {},
        onNavigate: () => {},
      },
    })
    await tick()
    const state = target.querySelector('.empty-folder-state')
    expect(state?.textContent).toContain('There’s one hidden item in this folder.')
    expect(state?.closest('[role="listbox"]')).toBeNull()
    expect(state?.parentElement?.classList.contains('row-area')).toBe(true)
    await expectNoA11yViolations(target)
  })

  it('empty folder with no cursor has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l1',
        volumeId: 'root',
        totalCount: 0,
        includeHidden: false,
        cursorIndex: -1,
        isFocused: true,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        effectiveSortBy: 'name',
        onSelect: () => {},
        onNavigate: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('populated (with parent entry + cached row) has no a11y violations', async () => {
    const mockEntries: FileEntry[] = [
      {
        name: 'report.md',
        path: '/root/report.md',
        isDirectory: false,
        isSymlink: false,
        size: 2048,
        modifiedAt: 1710000000,
        iconId: 'ext:md',
        permissions: 420,
        owner: 'test',
        group: 'staff',
      },
    ]
    const { getFileRange } = await import('#lib/tauri-commands/index.js')
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l2',
        volumeId: 'root',
        totalCount: 1,
        includeHidden: false,
        cursorIndex: 0,
        isFocused: true,
        hasParent: true,
        parentPath: '/root/..',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        effectiveSortBy: 'name',
        onSelect: () => {},
        onNavigate: () => {},
      },
    })
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('unfocused pane has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l3',
        volumeId: 'root',
        totalCount: 0,
        includeHidden: false,
        cursorIndex: -1,
        isFocused: false,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        effectiveSortBy: 'name',
        onSelect: () => {},
        onNavigate: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

// Brief shows a button per date, so the fallback note rides on the one the user asked for.
describe('BriefList header names', () => {
  async function headerNames(sort: { sortBy: SortColumn; effectiveSortBy: SortColumn }): Promise<(string | null)[]> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l1',
        volumeId: 'root',
        totalCount: 0,
        includeHidden: false,
        cursorIndex: -1,
        isFocused: true,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortOrder: 'descending',
        onSelect: () => {},
        onNavigate: () => {},
        ...sort,
      },
    })
    await tick()
    const names = [...target.querySelectorAll('.sortable-header')].map((button) => button.getAttribute('aria-label'))
    target.remove()
    return names
  }

  it('names each date button by its label and sort state', async () => {
    expect(await headerNames({ sortBy: 'added', effectiveSortBy: 'added' })).toEqual([
      'Name, not sorted',
      'Ext, not sorted',
      'Size, not sorted',
      'Modified, not sorted',
      'Created, not sorted',
      'Added, sorted descending',
    ])
  })

  it('says why on the requested button in a fallback, and only there', async () => {
    const names = await headerNames({ sortBy: 'added', effectiveSortBy: 'modified' })
    expect(names.at(-1)).toBe(
      'Added, sorted descending (This location doesn’t record when items were added, so Cmdr sorts by date modified here.)',
    )
    expect(names.slice(0, -1).every((name) => name?.endsWith('not sorted'))).toBe(true)
  })
})
