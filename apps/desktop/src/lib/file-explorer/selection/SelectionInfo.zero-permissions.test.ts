/**
 * Regression anchor for entries whose backend reports no POSIX permissions.
 *
 * SMB, archives, MTP, and the git virtual listings all leave `FileEntry`'s
 * `permissions` at the `0` default, and directories carry no `size` anywhere.
 * Brief mode's `SelectionInfo` must still show the folder's size (or the
 * `<dir>` placeholder) and its date, exactly as Full mode does — never a
 * "no access" placeholder inferred from the missing metadata.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SelectionInfo from './SelectionInfo.svelte'

vi.mock('$lib/indexing/index-state.svelte', () => ({
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
  getWalkedGround: () => ({ wholeVolume: false, roots: [] }),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
  formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
  formattedDate: (t: number | undefined) =>
    t
      ? { text: '2025-03-14 10:30', segments: [{ text: '2025-03-14 10:30', ageClass: 'age-fresh' as const }] }
      : { text: '', segments: [] },
  getSizeDisplayMode: () => 'smart',
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
}))

/** A folder as an SMB / archive / MTP backend hands it over: `permissions: 0`, no `size`. */
function makeRemoteDir(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    name: 'photos',
    path: '/Volumes/nas/photos',
    isDirectory: true,
    isSymlink: false,
    size: undefined,
    modifiedAt: 1710000000,
    iconId: 'folder',
    permissions: 0,
    owner: '',
    group: '',
    extendedMetadataLoaded: false,
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

function mountFileInfo(entry: ReturnType<typeof makeRemoteDir>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SelectionInfo, {
    target,
    props: { viewMode: 'brief', volumeId: 'smb-nas', entry, stats: STATS, selectedCount: 0 },
  })
  return target
}

describe('SelectionInfo Brief file-info for entries with no reported permissions', () => {
  it('shows the <dir> placeholder and the date for an unindexed remote folder', async () => {
    const t = mountFileInfo(makeRemoteDir())
    await tick()
    expect(t.textContent).toMatch(/DIR|<dir>/i)
    expect(t.textContent).not.toMatch(/permission|access/i)
    expect(t.querySelector('.date-label')?.textContent).toContain('2025-03-14')
  })

  it('shows the real size for an indexed remote folder', async () => {
    const t = mountFileInfo(
      makeRemoteDir({
        recursiveSize: 1024,
        recursivePhysicalSize: 1024,
        recursiveFileCount: 3,
        recursiveDirCount: 1,
        recursiveSizeComplete: true,
      }),
    )
    await tick()
    expect(t.querySelector('.size')?.textContent).toContain('1')
    expect(t.textContent).not.toMatch(/permission|access/i)
    expect(t.querySelector('.date-label')?.textContent).toContain('2025-03-14')
  })
})
