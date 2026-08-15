/**
 * Tests for the directory size-column state in `SelectionInfo.svelte`'s
 * Brief-mode `file-info` readout (the status bar under the pane).
 *
 * Mirrors FullList's size cell: a directory's recursive size shows the
 * "size updating" hourglass while THIS folder's size can move — its own ground
 * under a walker (tested both ways, since the roll-up repairs the ancestor
 * chain), the volume aggregating, or a live delete/copy in flight for this
 * folder (`recursiveSizePending`). Drives off the shared
 * `getDirSizeDisplayState`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import SelectionInfo from './SelectionInfo.svelte'

// Mutable so each test can say which volume is having which ground walked. Both
// are per-volume (work on volume B must not light up folders on volume A), so the
// mock answers only for the matching volume id.
const idx = vi.hoisted(() => ({
  scanningVolume: null as string | null,
  aggregatingVolume: null as string | null,
  walkingVolume: null as string | null,
  walkedRoots: [] as string[],
  wholeVolume: false,
}))
vi.mock('$lib/indexing/index-state.svelte', () => ({
  isVolumeScanning: (volumeId: string) => idx.scanningVolume === volumeId,
  isVolumeAggregating: (volumeId: string) => idx.aggregatingVolume === volumeId,
  getWalkedGround: (volumeId: string) =>
    idx.walkingVolume === volumeId
      ? { wholeVolume: idx.wholeVolume, roots: idx.walkedRoots }
      : { wholeVolume: false, roots: [] },
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
  idx.walkingVolume = null
  idx.walkedRoots = []
  idx.wholeVolume = false
})

/** Put `roots` under the walker on `volumeId`. */
function walking(volumeId: string, roots: string[]): void {
  idx.walkingVolume = volumeId
  idx.walkedRoots = roots
}

describe('SelectionInfo Brief file-info dir size state', () => {
  it('shows the stale hourglass for an indexed dir while its own ground is walked', async () => {
    walking('root', ['/Users/test/projects'])
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows the hourglass for a folder ABOVE the branch being walked', async () => {
    // The roll-up repairs the ancestor chain, so walking `projects/deep` moves
    // the size shown for `projects` too. A downward-only test would leave this
    // folder looking settled while its number is about to change.
    walking('root', ['/Users/test/projects/deep/nested'])
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('leaves a folder outside the branch alone', async () => {
    // The mixed state a phased first index spends most of its time in: home is
    // covered and exact while some other corner of the drive is still being read.
    walking('root', ['/opt/homebrew'])
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })

  it('does NOT light every folder up merely because the drive is scanning', async () => {
    // A phased first index has the drive "scanning" for minutes while only one
    // branch at a time can move a size. Reading the volume-wide flag here is what
    // put an hourglass on every row for the whole first index.
    idx.scanningVolume = 'root'
    const t = mountFileInfo(makeDir())
    await tick()
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })

  it('lights every folder up while a run takes the volume whole', async () => {
    // A full rebuild and every SMB/MTP scan announce no branches and blank the
    // sizes for their whole length, so there the whole-volume answer is honest.
    idx.walkingVolume = 'root'
    idx.wholeVolume = true
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

  it('does NOT show the hourglass when only ANOTHER volume is being walked (per-volume scope)', async () => {
    // The pane is on volume A (smb-nas); the walk is on volume B (root), over a
    // path that would match if the volume were ignored.
    walking('root', ['/Users/test/projects'])
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

  it("shows the hourglass when the walk is on the pane's OWN volume", async () => {
    walking('smb-nas', ['/Users/test/projects'])
    const t = mountFileInfo(makeDir(), 'smb-nas')
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows the stale hourglass when the dir is recursiveSizePending with no walk at all', async () => {
    const t = mountFileInfo(makeDir({ recursiveSizePending: true }))
    await tick()
    expect(t.querySelector('.stale-indicator')).not.toBeNull()
  })

  it('shows no hourglass for a settled indexed dir', async () => {
    const t = mountFileInfo(makeDir({ recursiveSizePending: false }))
    await tick()
    expect(t.querySelector('.stale-indicator')).toBeNull()
  })

  it('shows the dir placeholder with the not-ready hourglass for an unindexed dir being walked', async () => {
    walking('root', ['/Users/test/projects'])
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
