/**
 * Tier 3 a11y tests for `SelectionInfo.svelte`.
 *
 * Status bar below each pane. Tests cover each of the four display
 * modes: empty, no-selection (full), file-info (brief), and
 * selection-summary.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SelectionInfo from './SelectionInfo.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

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

const entry = {
  name: 'report.md',
  path: '/Users/test/report.md',
  isDirectory: false,
  isSymlink: false,
  size: 2048,
  modifiedAt: 1710000000,
  iconId: 'ext:md',
  permissions: 420,
  owner: 'test',
  group: 'staff',
  extendedMetadataLoaded: false,
}

describe('SelectionInfo a11y', () => {
  it('empty directory has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionInfo, {
      target,
      props: {
        volumeId: 'root',
        viewMode: 'full',
        entry: null,
        stats: {
          totalFiles: 0,
          totalDirs: 0,
          totalSize: 0,
          totalPhysicalSize: 0,
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
    await expectNoA11yViolations(target)
  })

  it('full mode, no selection has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionInfo, {
      target,
      props: {
        volumeId: 'root',
        viewMode: 'full',
        entry: null,
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
    await expectNoA11yViolations(target)
  })

  it('brief mode file-info has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionInfo, {
      target,
      props: {
        volumeId: 'root',
        viewMode: 'brief',
        entry,
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
    await expectNoA11yViolations(target)
  })

  it('selection summary has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SelectionInfo, {
      target,
      props: {
        volumeId: 'root',
        viewMode: 'full',
        entry: null,
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
        selectedCount: 3,
        currentPath: '',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
