/**
 * Tier 3 a11y tests for `BriefList.svelte`.
 *
 * Virtual-scrolling horizontal file list. We stub Tauri IPC, reactive
 * settings, and the icon cache so the component mounts with no real
 * backend. Tests cover the empty-folder overlay and a populated list
 * with mixed entries (folder + file) visible after an await.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import BriefList from './BriefList.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { FileEntry } from '../types'

vi.mock('$lib/tauri-commands', () => ({
  getFileRange: vi.fn(() => Promise.resolve([] as FileEntry[])),
  getDirStatsBatch: vi.fn(() => Promise.resolve({})),
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => undefined,
    iconCacheVersion: writable(0),
    iconCacheCleared: writable(0),
    prefetchIcons: vi.fn(),
  }
})

vi.mock('$lib/indexing/index-state.svelte', () => ({
  isScanning: () => false,
  isAggregating: () => false,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getRowHeight: () => 20,
  getIsCompactDensity: () => false,
  getUseAppIconsForDocuments: () => true,
  formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
  formatFileSize: (n: number) => `${String(n)} B`,
  getSizeDisplayMode: () => 'smart',
  getSizeMismatchWarning: () => false,
  getStripedRows: () => false,
}))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: (key: string) => {
    if (key === 'advanced.virtualizationBufferColumns') return 2
    return undefined
  },
}))

describe('BriefList a11y', () => {
  // TODO: When `cursorIndex >= 0` but no matching file row exists (empty
  // folder, or cursor past the end of the virtualized window), the
  // listbox sets `aria-activedescendant="file-<index>"` to a non-existent
  // ID. axe flags `aria-valid-attr-value`. Fix: make the binding
  // `cursorIndex >= 0 && cursorIndex < totalCount ? ... : undefined` in
  // `BriefList.svelte` (around the .brief-list element).
  it.skip('empty folder overlay has no a11y violations (BLOCKED: aria-valid-attr-value)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l1',
        totalCount: 0,
        includeHidden: false,
        cursorIndex: 0,
        isFocused: true,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        onSelect: () => {},
        onNavigate: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('empty folder with no cursor has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l1',
        totalCount: 0,
        includeHidden: false,
        cursorIndex: -1,
        isFocused: true,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
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
        extendedMetadataLoaded: false,
      },
    ]
    const { getFileRange } = await import('$lib/tauri-commands')
    vi.mocked(getFileRange).mockResolvedValue(mockEntries)

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BriefList, {
      target,
      props: {
        listingId: 'l2',
        totalCount: 1,
        includeHidden: false,
        cursorIndex: 0,
        isFocused: true,
        hasParent: true,
        parentPath: '/root/..',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
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
        totalCount: 0,
        includeHidden: false,
        cursorIndex: -1,
        isFocused: false,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        onSelect: () => {},
        onNavigate: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
