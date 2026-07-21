/**
 * Tests for the symlink hint indicator in `SelectionInfo.svelte`.
 *
 * The hint appears next to a directory's size when its subtree contains
 * symlinks. Symlinked content is intentionally omitted from the recursive
 * size (matching `du`/Finder), so we surface a small `(i)` icon to explain
 * why a folder of symlinks may show 0 bytes.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SelectionInfo from './SelectionInfo.svelte'

vi.mock('$lib/indexing/index-state.svelte', () => ({
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
  formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
  formattedDate: (t: number | undefined) =>
    t
      ? {
          text: '2025-03-14 10:30',
          segments: [
            { text: '2025', ageClass: 'age-fresh' as const },
            { text: '-', ageClass: null },
            { text: '03', ageClass: null },
            { text: '-', ageClass: null },
            { text: '14', ageClass: null },
            { text: ' ', ageClass: null },
            { text: '10', ageClass: null },
            { text: ':', ageClass: null },
            { text: '30', ageClass: null },
          ],
        }
      : { text: '', segments: [] },
  getSizeDisplayMode: () => 'smart',
  getFileSizeUnit: () => 'bytes',
  getFileSizeFormat: () => 'binary',
}))

function makeDir(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    name: 'links',
    path: '/Users/test/links',
    isDirectory: true,
    isSymlink: false,
    size: undefined,
    modifiedAt: 1710000000,
    iconId: 'folder',
    permissions: 0o755,
    owner: 'test',
    group: 'staff',
    extendedMetadataLoaded: false,
    recursiveSize: 0,
    recursivePhysicalSize: 0,
    recursiveFileCount: 1,
    recursiveDirCount: 0,
    ...overrides,
  }
}

describe('SelectionInfo symlink hint', () => {
  it('renders the (i) icon when the directory has recursiveHasSymlinks=true', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionInfo, {
      target,
      props: {
        volumeId: 'root',
        viewMode: 'brief',
        entry: makeDir({ recursiveHasSymlinks: true }),
        stats: {
          totalFiles: 42,
          totalDirs: 5,
          totalSize: 1_000_000,
          totalPhysicalSize: 1_000_000,
          selectedFiles: null,
          selectedDirs: null,
          selectedSize: null,
          selectedPhysicalSize: null,
        },
        selectedCount: 0,
        currentPath: '',
      },
    })
    await tick()
    const hint = target.querySelector('.symlink-hint')
    expect(hint).not.toBeNull()
    expect(hint?.getAttribute('aria-label')).toMatch(/symlinks/i)
  })

  it('does not render the (i) icon when recursiveHasSymlinks is false', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionInfo, {
      target,
      props: {
        volumeId: 'root',
        viewMode: 'brief',
        entry: makeDir({ recursiveHasSymlinks: false }),
        stats: {
          totalFiles: 42,
          totalDirs: 5,
          totalSize: 1_000_000,
          totalPhysicalSize: 1_000_000,
          selectedFiles: null,
          selectedDirs: null,
          selectedSize: null,
          selectedPhysicalSize: null,
        },
        selectedCount: 0,
        currentPath: '',
      },
    })
    await tick()
    expect(target.querySelector('.symlink-hint')).toBeNull()
  })

  it('does not render the (i) icon for plain files with the flag', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionInfo, {
      target,
      props: {
        volumeId: 'root',
        viewMode: 'brief',
        // Files don't get the flag, but guard against future regressions
        entry: makeDir({ isDirectory: false, size: 1024, recursiveHasSymlinks: true }),
        stats: {
          totalFiles: 42,
          totalDirs: 5,
          totalSize: 1_000_000,
          totalPhysicalSize: 1_000_000,
          selectedFiles: null,
          selectedDirs: null,
          selectedSize: null,
          selectedPhysicalSize: null,
        },
        selectedCount: 0,
        currentPath: '',
      },
    })
    await tick()
    expect(target.querySelector('.symlink-hint')).toBeNull()
  })
})
