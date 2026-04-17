/**
 * Tier 3 a11y tests for `FileIcon.svelte`.
 *
 * 16x16 icon with emoji fallback and symlink/sync overlay badges. The
 * component relies on `$lib/icon-cache` (cache writable) and
 * `$lib/settings/reactive-settings.svelte` (gold folder toggle), which
 * both need to be stubbed so the icon resolves deterministically.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FileIcon from './FileIcon.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: (iconId: string) => (iconId === 'dir' ? '/icons/dir.svg' : undefined),
    iconCacheVersion: writable(0),
  }
})

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getIsCmdrGold: () => false,
}))

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

describe('FileIcon a11y', () => {
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
