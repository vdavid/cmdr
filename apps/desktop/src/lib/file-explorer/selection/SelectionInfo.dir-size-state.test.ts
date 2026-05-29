/**
 * Tests for the directory size-column state in `SelectionInfo.svelte`'s
 * Brief-mode `file-info` readout (the status bar under the pane).
 *
 * Mirrors FullList's size cell: a directory's recursive size shows the
 * "size updating" hourglass while the index is unsettled — either globally
 * (a full scan / aggregation) or per-folder (a live delete/copy in flight,
 * via `recursiveSizePending`). Drives off the shared `getDirSizeDisplayState`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import SelectionInfo from './SelectionInfo.svelte'

// Mutable so each test can flip the global indexing state.
const idx = vi.hoisted(() => ({ scanning: false, aggregating: false }))
vi.mock('$lib/indexing/index-state.svelte', () => ({
  isScanning: () => idx.scanning,
  isAggregating: () => idx.aggregating,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
  formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
  formattedDate: (t: number | undefined) =>
    t
      ? { text: '2025-03-14 10:30', parts: { left: [{ text: '2025', ageClass: 'age-fresh' as const }], right: null } }
      : { text: '', parts: { left: [], right: null } },
  getSizeDisplayMode: () => 'smart',
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
}))

function makeDir(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    name: 'projects',
    path: '/Users/test/projects',
    isDirectory: true,
    isSymlink: false,
    size: undefined,
    modifiedAt: 1710000000,
    iconId: 'folder',
    permissions: 0o755,
    owner: 'test',
    group: 'staff',
    extendedMetadataLoaded: false,
    recursiveSize: 1024,
    recursivePhysicalSize: 1024,
    recursiveFileCount: 3,
    recursiveDirCount: 1,
    ...overrides,
  }
}

const STATS = {
  totalFiles: 42,
  totalDirs: 5,
  totalSize: 1_000_000,
  totalPhysicalSize: 1_000_000,
  selectedFiles: null,
  selectedDirs: null,
  selectedSize: null,
  selectedPhysicalSize: null,
}

function mountFileInfo(entry: ReturnType<typeof makeDir>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SelectionInfo, {
    target,
    props: { viewMode: 'brief', entry, stats: STATS, selectedCount: 0 },
  })
  return target
}

beforeEach(() => {
  idx.scanning = false
  idx.aggregating = false
})

describe('SelectionInfo Brief file-info dir size state', () => {
  it('shows the stale hourglass for an indexed dir while scanning', async () => {
    idx.scanning = true
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows the stale hourglass for an indexed dir while aggregating (not just scanning)', async () => {
    idx.aggregating = true
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows the stale hourglass when the dir is recursiveSizePending with no global scan', async () => {
    const t = mountFileInfo(makeDir({ recursiveSizePending: true }))
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows no hourglass for a settled indexed dir', async () => {
    const t = mountFileInfo(makeDir({ recursiveSizePending: false }))
    await tick()
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })

  it('shows "Scanning" for an unindexed dir while indexing', async () => {
    idx.scanning = true
    const t = mountFileInfo(makeDir({ recursiveSize: undefined, recursivePhysicalSize: undefined }))
    await tick()
    expect(t.textContent ?? '').toMatch(/Scanning/i)
  })

  it('shows the dir placeholder for an unindexed dir when idle', async () => {
    const t = mountFileInfo(makeDir({ recursiveSize: undefined, recursivePhysicalSize: undefined }))
    await tick()
    expect(t.textContent ?? '').toMatch(/DIR|<dir>/i)
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })
})
