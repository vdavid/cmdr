/**
 * Tests for the Cmdr-gold folder recolor predicate in `FileIcon.svelte`.
 *
 * When the app color is "Cmdr gold", generic folder icons get a grayscale-first
 * CSS filter (`.gold-folder`) that re-tints them gold regardless of the macOS
 * system accent. The subtle contract this pins:
 *   - special system folders (`special:*`) ARE recolored, so Downloads/Desktop/
 *     etc. don't leak the system accent through,
 *   - packages (`pkg:*`, full-color `.app` icons) and custom-icon folders
 *     (`path:*`, a user-assigned icon) are NEVER recolored, since the filter
 *     would ruin the former and override the latter.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

// Every id resolves to a data URL so the live `<img>` (not the fallback) renders.
vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => '/icons/stub.svg',
    iconCacheVersion: writable(0),
  }
})

// Force the "Cmdr gold" app color on, so the recolor gate depends solely on iconId.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getIsCmdrGold: () => true,
}))

import FileIcon from './FileIcon.svelte'

const baseEntry = {
  name: 'entry',
  path: '/Users/test/entry',
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

function renderWithIconId(iconId: string): HTMLImageElement {
  target = document.createElement('div')
  document.body.appendChild(target)
  app = mount(FileIcon, { target, props: { file: { ...baseEntry, iconId } } })
  flushSync()
  const img = target.querySelector('img.icon')
  if (!img) throw new Error(`no icon img rendered for iconId=${iconId}`)
  return img as HTMLImageElement
}

afterEach(() => {
  if (app) void unmount(app)
  app = undefined
  target?.remove()
  target = undefined
})

describe('FileIcon gold recolor gate', () => {
  it.each(['dir', 'symlink-dir', 'special:downloads', 'special:home'])(
    'recolors folder icon id %s to gold',
    (iconId) => {
      expect(renderWithIconId(iconId).classList.contains('gold-folder')).toBe(true)
    },
  )

  it.each(['pkg:/Applications/Safari.app', 'path:/Users/test/CustomFolder', 'ext:md', 'file'])(
    'leaves icon id %s untouched (no gold recolor)',
    (iconId) => {
      expect(renderWithIconId(iconId).classList.contains('gold-folder')).toBe(false)
    },
  )
})
