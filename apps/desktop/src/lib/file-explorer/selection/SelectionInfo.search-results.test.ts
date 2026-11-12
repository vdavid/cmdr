/**
 * Tests for the two props a search-results pane needs from `SelectionInfo.svelte`:
 * `totalMatches` (the search found more than the pane holds) and `showVolumeSpace`
 * (a snapshot isn't a place on any one disk, so the free-space readout is off).
 *
 * The counting and selection-summary paths themselves are shared with a normal
 * pane and covered by the other `SelectionInfo.*` files.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount } from 'svelte'
import type { ListingStats } from '../types'
import SelectionInfo from './SelectionInfo.svelte'
import { getEffectiveShortcuts, removeShortcut, resetShortcut, setShortcut } from '#lib/shortcuts/shortcuts-store.js'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

vi.mock('#lib/indexing/index-state.svelte.js', () => ({
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
  getWalkedGround: () => [],
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
  formatDateTime: () => '2026-03-14 10:30',
  formattedDate: () => ({ text: '', segments: [] }),
  getSizeDisplayMode: () => 'smart',
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
}))

function stats(overrides: Partial<ListingStats> = {}): ListingStats {
  return {
    totalFiles: 112,
    totalDirs: 0,
    totalSize: 1_000,
    totalPhysicalSize: 1_000,
    selectedFiles: null,
    selectedDirs: null,
    selectedSize: null,
    selectedPhysicalSize: null,
    ...overrides,
  }
}

const space = { kind: 'bounded' as const, totalBytes: 1_000, availableBytes: 400, usedBytes: 600 }

function render(props: Record<string, unknown>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SelectionInfo, {
    target,
    props: {
      viewMode: 'full',
      volumeId: 'root',
      entry: null,
      stats: stats(),
      selectedCount: 0,
      effectiveSortBy: 'name',
      ...props,
    },
  })
  return target
}

describe('SelectionInfo on a search-results pane', () => {
  it('shows the live shortcut as a compact reveal hint', () => {
    setShortcut('nav.parent', 0, '⌃K')
    const target = render({ showInFolderHint: true })
    expect(target.querySelector('[data-search-result-hint]')?.textContent).toContain('⌃K')
    expect(target.querySelector('[data-search-result-hint]')?.textContent).toContain('Show in folder')
    resetShortcut('nav.parent')
  })

  it('drops the reveal hint when nav.parent is unbound', () => {
    while (getEffectiveShortcuts('nav.parent').length > 0) removeShortcut('nav.parent', 0)
    const target = render({ showInFolderHint: true })
    expect(target.querySelector('[data-search-result-hint]')).toBeNull()
    resetShortcut('nav.parent')
  })

  it('says how many of the matches the pane holds when the list is a lower bound', () => {
    const target = render({ totalMatches: 345 })
    expect(target.textContent).toContain('112 of 345 matches')
  })

  it('counts plainly when every match made it into the pane', () => {
    const target = render({ totalMatches: 112 })
    expect(target.textContent).toContain('112 files')
    expect(target.textContent).not.toContain('matches')
  })

  it('counts folders among the rows it reports as shown', () => {
    const target = render({ stats: stats({ totalFiles: 100, totalDirs: 12 }), totalMatches: 345 })
    expect(target.textContent).toContain('112 of 345 matches')
  })

  it('hides the free-space readout when the pane has no one place on disk', () => {
    const target = render({ showVolumeSpace: false, volumeSpace: space })
    expect(target.querySelector('.disk-space-text')).toBeNull()
  })

  it('still shows free space for a normal pane', () => {
    const target = render({ volumeSpace: space })
    expect(target.querySelector('.disk-space-text')).not.toBeNull()
  })
})
