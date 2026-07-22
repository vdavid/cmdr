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

// Mutable so each test can set WHICH volume is scanning / aggregating. The
// predicates are per-volume now (a scan on volume B must not light up folders on
// volume A), so the mock answers true only for the matching volume id.
const idx = vi.hoisted(() => ({ scanningVolume: null as string | null, aggregatingVolume: null as string | null }))
vi.mock('$lib/indexing/index-state.svelte', () => ({
  isVolumeScanning: (volumeId: string) => idx.scanningVolume === volumeId,
  isVolumeAggregating: (volumeId: string) => idx.aggregatingVolume === volumeId,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
  formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
  formattedDate: (t: number | undefined) =>
    t
      ? { text: '2025-03-14 10:30', segments: [{ text: '2025', ageClass: 'age-fresh' as const }] }
      : { text: '', segments: [] },
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

function mountFileInfo(entry: ReturnType<typeof makeDir>, volumeId = 'root'): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SelectionInfo, {
    target,
    props: { viewMode: 'brief', volumeId, entry, stats: STATS, selectedCount: 0 },
  })
  return target
}

beforeEach(() => {
  idx.scanningVolume = null
  idx.aggregatingVolume = null
})

describe('SelectionInfo Brief file-info dir size state', () => {
  it('shows the stale hourglass for an indexed dir while scanning', async () => {
    idx.scanningVolume = 'root'
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows the stale hourglass for an indexed dir while aggregating (not just scanning)', async () => {
    idx.aggregatingVolume = 'root'
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('does NOT show the hourglass when only ANOTHER volume is scanning (per-volume scope)', async () => {
    // The pane is on volume A (smb-nas); only volume B (root) is scanning. The
    // per-folder hourglass must stay off. With the old global `isScanning()` this
    // wrongly lit up for every pane's folders regardless of which drive scanned.
    idx.scanningVolume = 'root'
    const t = mountFileInfo(makeDir(), 'smb-nas')
    await tick()
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })

  it('does NOT show the hourglass when only another volume is aggregating', async () => {
    idx.aggregatingVolume = 'root'
    const t = mountFileInfo(makeDir(), 'smb-nas')
    await tick()
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })

  it("shows the hourglass when the pane's OWN volume is scanning", async () => {
    idx.scanningVolume = 'smb-nas'
    const t = mountFileInfo(makeDir(), 'smb-nas')
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

  it('shows the dir placeholder with the not-ready hourglass for an unindexed dir while indexing', async () => {
    idx.scanningVolume = 'root'
    const t = mountFileInfo(makeDir({ recursiveSize: undefined, recursivePhysicalSize: undefined }))
    await tick()
    expect(t.textContent).toMatch(/DIR/)
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows the dir placeholder for an unindexed dir when idle', async () => {
    const t = mountFileInfo(makeDir({ recursiveSize: undefined, recursivePhysicalSize: undefined }))
    await tick()
    expect(t.textContent).toMatch(/DIR|<dir>/i)
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })
})
