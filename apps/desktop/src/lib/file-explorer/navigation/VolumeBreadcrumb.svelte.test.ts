/**
 * Behavioral tests for `VolumeBreadcrumb.svelte` focused on the favorite-rename
 * keyboard guard (Fix E): while a favorite is being renamed inline, the dropdown
 * must NOT consume arrow / Home / End / Enter keys, so the textbox keeps them and
 * the panes behind the dropdown stay inert. The cross-pane suppression itself
 * lives in `DualPaneExplorer.routeToVolumeChooser`; here we pin the leaf guard.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'
import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'

vi.mock('$lib/tauri-commands', () => ({
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: { id: 'root', path: '/' } })),
  upgradeToSmbVolume: vi.fn(() => Promise.resolve({ status: 'success' })),
  ejectVolume: vi.fn(() => Promise.resolve()),
  getIpcErrorMessage: (e: unknown) => String(e),
  getVolumeSpace: vi.fn(() => Promise.resolve(null)),
  systemHasSavedSmbPassword: vi.fn(() => Promise.resolve(false)),
  upgradeToSmbVolumeUsingSavedPassword: vi.fn(() => Promise.resolve({ status: 'success' })),
  removeFavorite: vi.fn(() => Promise.resolve()),
  renameFavorite: vi.fn(() => Promise.resolve()),
  reorderFavorites: vi.fn(() => Promise.resolve()),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'fav-1', name: 'Documents', path: '/Users/test/Documents', category: 'favorite', isEjectable: false },
  ],
  getVolumesTimedOut: () => false,
  isVolumesRefreshing: () => false,
  isVolumeRetryFailed: () => false,
  requestVolumeRefresh: vi.fn(),
}))

vi.mock('$lib/stores/volume-busy-store.svelte', () => ({ isVolumeBusy: () => false }))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn(() => 'toast-id'), dismissToast: vi.fn() }))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
  getFileSizeFormat: () => 'binary',
  getFileSizeUnit: () => 'bytes',
  getNetworkEnabled: () => true,
  getUseAppIconsForDocuments: () => false,
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: vi.fn().mockReturnValue('/icons/dir.png'),
    iconCacheVersion: writable(0),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
  }
})

interface BreadcrumbInstance {
  open: () => void
  getIsOpen: () => boolean
  handleKeyDown: (e: KeyboardEvent) => boolean
}

function mountBreadcrumb(): { instance: BreadcrumbInstance; target: HTMLDivElement } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(VolumeBreadcrumb, {
    target,
    props: { volumeId: 'root', currentPath: '/Users/test' },
  }) as unknown as BreadcrumbInstance
  return { instance, target }
}

describe('VolumeBreadcrumb favorite-rename keyboard guard', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('handleKeyDown returns false when the dropdown is closed', () => {
    const { instance } = mountBreadcrumb()
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown' }))).toBe(false)
  })

  it('consumes ArrowDown when the dropdown is open and not renaming', async () => {
    const { instance } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()
    expect(instance.getIsOpen()).toBe(true)
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown' }))).toBe(true)
  })

  it('does NOT consume ArrowDown / Home / End while a favorite rename is active', async () => {
    const { instance, target } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()

    // Start the inline rename: right-click the favorite row, then click "Rename".
    const favRow = target.querySelector('.favorite-item') as HTMLElement
    expect(favRow).toBeTruthy()
    favRow.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    await tick()
    flushSync()
    const renameItem = [...target.querySelectorAll('.row-menu-item')].find(
      (el) => el.textContent.trim() === 'Rename',
    ) as HTMLElement
    expect(renameItem).toBeTruthy()
    renameItem.click()
    await tick()
    flushSync()

    expect(target.querySelector('.favorite-rename-input')).toBeTruthy()

    // The guard: keys the dropdown would otherwise eat must fall through (false)
    // so the rename textbox keeps them.
    for (const key of ['ArrowDown', 'ArrowUp', 'Home', 'End']) {
      expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key }))).toBe(false)
    }
  })
})
