/**
 * Tier 3 a11y tests for the selection-area components: the file icon, the status
 * bar below each pane, the sortable column header, and the tag dots.
 *
 * One file per component would cost about four times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, fixtures, props,
 * and assertions.
 *
 * No stub here disagrees between blocks: the two `reactive-settings` sets overlap
 * only on `getFileSizeFormat`, and agree on it. Every stub spreads the real module
 * first, so the two blocks that never stubbed anything still see every un-stubbed
 * export.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import FileIcon from './FileIcon.svelte'
import SelectionInfo from './SelectionInfo.svelte'
import SortableHeader from './SortableHeader.svelte'
import TagDots from './TagDots.svelte'
import type { TagRef } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/icon-cache', async (importOriginal) => {
  const { writable } = await import('svelte/store')
  return {
    ...(await importOriginal<Record<string, unknown>>()),
    getCachedIcon: (iconId: string) => (iconId === 'dir' ? '/icons/dir.svg' : undefined),
    iconCacheVersion: writable(0),
  }
})

vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getIsCmdrGold: () => false,
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

vi.mock('$lib/indexing/index-state.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isVolumeScanning: () => false,
  isVolumeAggregating: () => false,
  getWalkedGround: () => [],
}))

// These components share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

const fileEntry = {
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

/**
 * Tier 3 a11y tests for `FileIcon.svelte`.
 *
 * 16x16 icon with emoji fallback and symlink/sync overlay badges. The
 * component relies on `$lib/icon-cache` (cache writable) and
 * `$lib/settings/reactive-settings.svelte` (gold folder toggle), which
 * both need to be stubbed so the icon resolves deterministically.
 */
describe('FileIcon a11y', () => {
  const folderEntry = {
    ...fileEntry,
    name: 'projects',
    path: '/Users/test/projects',
    isDirectory: true,
    iconId: 'dir',
  }

  const symlinkEntry = {
    ...fileEntry,
    name: 'link-to-stuff',
    isSymlink: true,
    iconId: 'symlink-dir',
  }

  it('regular file (emoji fallback) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FileIcon, { target, props: { file: fileEntry } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('folder with cached icon has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FileIcon, { target, props: { file: folderEntry } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('symlink with badge has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FileIcon, { target, props: { file: symlinkEntry } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('file with sync icon overlay has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FileIcon, { target, props: { file: fileEntry, syncIcon: '/icons/sync-synced.svg' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SelectionInfo.svelte`.
 *
 * Status bar below each pane. Tests cover each of the four display
 * modes: empty, no-selection (full), file-info (brief), and
 * selection-summary.
 */
describe('SelectionInfo a11y', () => {
  const entry = fileEntry

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
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SortableHeader.svelte`.
 */
describe('SortableHeader a11y', () => {
  it('active ascending has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SortableHeader, {
      target,
      props: {
        column: 'name',
        label: 'Name',
        currentSortColumn: 'name',
        currentSortOrder: 'ascending',
        onClick: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('inactive right-aligned has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SortableHeader, {
      target,
      props: {
        column: 'size',
        label: 'Size',
        currentSortColumn: 'name',
        currentSortOrder: 'ascending',
        align: 'right',
        onClick: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `TagDots.svelte`.
 *
 * The cluster is decorative pixels (colored dots), so it must expose the tag
 * names through an accessible label; the individual dots stay `aria-hidden`.
 * (The same names also ride a `use:tooltip` hover hint, which renders into the
 * shared tooltip element only on hover and so isn't asserted here.) Pure, no
 * store/icon-cache stubs needed.
 */
describe('TagDots a11y', () => {
  const tag = (name: string, color: number): TagRef => ({ name, color })

  function render(tags: TagRef[] | undefined): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TagDots, { target, props: { tags } })
    return target
  }

  it('exposes the tag names as an accessible label on the cluster', async () => {
    const target = render([tag('Urgent', 6), tag('Review', 2)])
    await tick()
    const cluster = target.querySelector('[role="img"]')
    expect(cluster).not.toBeNull()
    expect(cluster?.getAttribute('aria-label')).toBe('Urgent, Review')
    await expectNoA11yViolations(target)
  })

  it('includes colourless tag names in the label even with no dot', async () => {
    const target = render([tag('Filed', 0), tag('Hot', 6)])
    await tick()
    const cluster = target.querySelector('[role="img"]')
    expect(cluster?.getAttribute('aria-label')).toBe('Filed, Hot')
    await expectNoA11yViolations(target)
  })

  it('renders nothing when there are no colored tags', async () => {
    const target = render([tag('OnlyColourless', 0)])
    await tick()
    expect(target.querySelector('[role="img"]')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('overflow chip is hidden from assistive tech (label carries the names)', async () => {
    const target = render([tag('a', 1), tag('b', 2), tag('c', 3), tag('d', 4), tag('e', 5)])
    await tick()
    const chip = target.querySelector('.tag-chip')
    expect(chip?.getAttribute('aria-hidden')).toBe('true')
    expect(chip?.textContent).toBe('+3')
    await expectNoA11yViolations(target)
  })
})
