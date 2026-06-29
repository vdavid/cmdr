/**
 * Fix A: when `listing.showExtensionInName` is ON, the Ext DATA column is gone
 * (the full filename lives in the Name column), but sort-by-extension must stay
 * reachable from the header. The single Name-column header splits into two
 * `SortableHeader` sort triggers inside a `.header-name-ext` row: "Name" (fills,
 * left) and "Ext" (right), each clickable and showing its caret when active.
 *
 * This pins that the header renders both triggers AND that no Ext data cell is
 * emitted, so the renderer stays in lockstep with `measure-column-widths.ts`
 * (which returns `ext: 0` in this mode).
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FullList from './FullList.svelte'
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
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getRowHeight: () => 20,
  getIconSize: () => 16,
  getIsCompactDensity: () => false,
  getUseAppIconsForDocuments: () => true,
  formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
  formattedDate: (t: number | undefined) =>
    t
      ? { text: '2025-03-14 10:30', segments: [{ text: '2025-03-14 10:30', ageClass: null }] }
      : { text: '', segments: [] },
  formatFileSize: (n: number) => `${String(n)} B`,
  getSizeDisplayMode: () => 'smart',
  getSizeMismatchWarning: () => false,
  getStripedRows: () => false,
  // The flag under test:
  getShowExtensionInName: () => true,
  getShowTags: () => false,
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
}))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: (key: string) => (key === 'advanced.virtualizationBufferRows' ? 20 : undefined),
}))

async function mountPopulated() {
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
  mount(FullList, {
    target,
    props: {
      listingId: 'l1',
      volumeId: 'root',
      totalCount: 1,
      includeHidden: false,
      cursorIndex: 0,
      isFocused: true,
      hasParent: false,
      parentPath: '',
      currentPath: '/root',
      sortBy: 'extension',
      sortOrder: 'ascending',
      onSelect: () => {},
      onNavigate: () => {},
    },
  })
  await tick()
  await new Promise((r) => setTimeout(r, 0))
  await tick()
  return target
}

describe('FullList combined Name+Ext header (showExtensionInName)', () => {
  it('renders the combined header with both Name and Ext sort triggers', async () => {
    const target = await mountPopulated()
    const combined = target.querySelector('.header-name-ext')
    expect(combined).toBeTruthy()
    const labels = [...(combined?.querySelectorAll('.sortable-header .label') ?? [])].map((l) => l.textContent)
    expect(labels).toEqual(['Name', 'Ext'])
    // Both triggers are real buttons (keyboard- and mouse-operable).
    expect(combined?.querySelectorAll('button.sortable-header').length).toBe(2)
  })

  it('shows the active caret on the Ext trigger when sorting by extension', async () => {
    const target = await mountPopulated()
    const combined = target.querySelector('.header-name-ext')
    const extBtn = combined?.querySelectorAll('.sortable-header')[1]
    expect(extBtn?.classList.contains('is-active')).toBe(true)
    // The caret span is only shown (not `.invisible`) on the active column.
    expect(extBtn?.querySelector('.sort-indicator:not(.invisible)')).toBeTruthy()
  })

  it('emits no Ext data cell (full filename rides in the Name column)', async () => {
    const target = await mountPopulated()
    expect(target.querySelectorAll('.col-ext').length).toBe(0)
  })
})
