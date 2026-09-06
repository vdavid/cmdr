/**
 * Tests that `FileIcon.svelte` DRAWS a folder's Finder custom icon.
 *
 * The regression this pins: a custom-icon folder keeps the generic `dir` iconId
 * (the backend defers the `kHasCustomIcon` getxattr off the bulk-listing hot
 * path), so its icon is cached under a `path:{dir}` key that no entry points at.
 * Looking up `iconId` alone drew the generic folder over an icon
 * `prefetchCustomFolderIcons` had already fetched and cached — the icon arrived
 * and was silently thrown away.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

const GENERIC_URL = '/icons/generic-folder.svg'
const CUSTOM_URL = '/icons/custom-artwork.svg'
const CUSTOM_PATH = '/Users/test/Projects'

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => GENERIC_URL,
    // Exactly one folder has been assigned a custom icon.
    getCachedCustomFolderIcon: (path: string) => (path === CUSTOM_PATH ? CUSTOM_URL : undefined),
    iconCacheVersion: writable(0),
  }
})

// Gold is ON throughout, so the custom-icon cases also prove the recolor filter
// keeps its hands off the user's own artwork.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
  getIsCmdrGold: () => true,
}))

import FileIcon from './FileIcon.svelte'

const baseEntry = {
  name: 'Projects',
  path: CUSTOM_PATH,
  isDirectory: true,
  isSymlink: false,
  size: 0,
  modifiedAt: 1710000000,
  iconId: 'dir',
  permissions: 420,
  owner: 'test',
  group: 'staff',
  extendedMetadataLoaded: false,
}

let target: HTMLElement | undefined
let app: ReturnType<typeof mount> | undefined

function render(overrides: Partial<typeof baseEntry> = {}): HTMLImageElement {
  target = document.createElement('div')
  document.body.appendChild(target)
  app = mount(FileIcon, { target, props: { file: { ...baseEntry, ...overrides } } })
  flushSync()
  const img = target.querySelector('img.icon')
  if (!img) throw new Error('no icon img rendered')
  return img as HTMLImageElement
}

afterEach(() => {
  if (app) void unmount(app)
  app = undefined
  target?.remove()
  target = undefined
})

describe('FileIcon custom folder icons', () => {
  it('draws the custom icon for a folder that has one, despite its generic dir iconId', () => {
    expect(render().getAttribute('src')).toBe(CUSTOM_URL)
  })

  it('never gold-recolors a custom icon (it would repaint the user artwork)', () => {
    expect(render().classList.contains('gold-folder')).toBe(false)
  })

  it('still draws the generic folder icon for a folder without a custom one', () => {
    const img = render({ path: '/Users/test/Plain' })
    expect(img.getAttribute('src')).toBe(GENERIC_URL)
    // Untouched by the custom-icon path, so gold still applies.
    expect(img.classList.contains('gold-folder')).toBe(true)
  })

  it('leaves a symlinked directory on its generic icon', () => {
    // Mirrors `prefetchCustomFolderIcons`, which never asks about symlinks: the
    // link-badged glyph is the salient signal.
    const img = render({ isSymlink: true, iconId: 'symlink-dir' })
    expect(img.getAttribute('src')).toBe(GENERIC_URL)
  })

  it('leaves files alone even at a path that collides with a custom-icon folder', () => {
    const img = render({ isDirectory: false, iconId: 'ext:md' })
    expect(img.getAttribute('src')).toBe(GENERIC_URL)
  })
})
