/**
 * Behavioral tests for `VolumeBreadcrumb.svelte` focused on the favorite-rename
 * keyboard guard (Fix E): while a favorite is being renamed inline, the dropdown
 * must NOT consume arrow / Home / End / Enter keys, so the textbox keeps them and
 * the panes behind the dropdown stay inert. The cross-pane suppression itself
 * lives in `DualPaneExplorer.routeToVolumeChooser`; here we pin the leaf guard.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'
import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'

const reorderFavorites = vi.fn(() => Promise.resolve())
const ejectVolume = vi.fn(() => Promise.resolve())
const disconnectPlace = vi.fn(() => Promise.resolve(true))
const showVolumeRowContextMenu = vi.fn(() => Promise.resolve())
const hasServerSecret = vi.fn(() => Promise.resolve(true))
const listSavedServers = vi.fn(() =>
  Promise.resolve([{ id: 'sftp-nas-local-22-ada', places: [{ volumeId: 'sftp-nas-local-22-ada' }] }]),
)

/**
 * The volume list the store mock answers with. Swappable so the server-row block
 * can put a place in the switcher without shifting the favorite indices every
 * other block here counts on.
 */
const stubs = vi.hoisted(() => ({ volumes: null as unknown[] | null, ejecting: new Set<string>() }))

// Captures the `volume-context-action` listener the component registers in `onMount`, so a
// test can fire a native row-menu pick (Rename / Remove) the same way the backend would.
let volumeContextActionHandler: ((payload: { action: string; volumeId: string }) => void) | undefined

vi.mock('$lib/tauri-commands', () => ({
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: { id: 'root', path: '/' } })),
  upgradeToSmbVolume: vi.fn(() => Promise.resolve({ status: 'success' })),
  ejectVolume: (...args: unknown[]) => ejectVolume(...(args as [])),
  getVolumeSpace: vi.fn(() => Promise.resolve(null)),
  systemHasSavedSmbPassword: vi.fn(() => Promise.resolve(false)),
  upgradeToSmbVolumeUsingSavedPassword: vi.fn(() => Promise.resolve({ status: 'success' })),
  removeFavorite: vi.fn(() => Promise.resolve()),
  renameFavorite: vi.fn(() => Promise.resolve()),
  reorderFavorites: (...args: unknown[]) => reorderFavorites(...(args as [])),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
  showVolumeRowContextMenu: (...args: unknown[]) => showVolumeRowContextMenu(...(args as [])),
  disconnectPlace: (...args: unknown[]) => disconnectPlace(...(args as [])),
  forgetServer: vi.fn(() => Promise.resolve(true)),
  forgetServerSecret: vi.fn(() => Promise.resolve(true)),
  hasServerSecret: (...args: unknown[]) => hasServerSecret(...(args as [])),
  listSavedServers: () => listSavedServers(),
  onVolumeContextAction: (cb: (payload: { action: string; volumeId: string }) => void) => {
    volumeContextActionHandler = cb
    return Promise.resolve(() => {})
  },
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () =>
    stubs.volumes ?? [
      { id: 'fav-1', name: 'Documents', path: '/Users/test/Documents', category: 'favorite', isEjectable: false },
      { id: 'fav-2', name: 'Downloads', path: '/Users/test/Downloads', category: 'favorite', isEjectable: false },
      { id: 'fav-3', name: 'Projects', path: '/Users/test/Projects', category: 'favorite', isEjectable: false },
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    ],
  getVolumesTimedOut: () => false,
  isVolumesRefreshing: () => false,
  isVolumeRetryFailed: () => false,
  requestVolumeRefresh: vi.fn(),
}))

vi.mock('$lib/stores/volume-busy-store.svelte', () => ({
  isVolumeBusy: () => false,
  isVolumeEjecting: (id: string) => stubs.ejecting.has(id),
}))

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
    getCachedCustomFolderIcon: () => undefined,
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

describe('VolumeBreadcrumb favorite keyboard reorder (Alt+Up / Alt+Down)', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    reorderFavorites.mockClear()
  })

  it('Alt+ArrowDown on the highlighted favorite persists the moved order via reorderFavorites', async () => {
    const { instance } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()

    // Highlight the first favorite (fav-1). Home jumps the virtual highlight to index 0.
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'Home' }))).toBe(true)
    await tick()
    flushSync()

    // Alt+ArrowDown moves fav-1 down one slot: ['2', '1', '3'] (bare ids).
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', altKey: true }))).toBe(true)
    await tick()
    flushSync()

    expect(reorderFavorites).toHaveBeenCalledTimes(1)
    expect(reorderFavorites).toHaveBeenCalledWith(['2', '1', '3'])
  })

  it('two quick Alt+ArrowDown presses keep moving the SAME favorite (optimistic local order, no stale-state race)', async () => {
    const { instance } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()

    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'Home' }))).toBe(true)
    await tick()
    flushSync()

    // First press: fav-1 (index 0) → index 1, order ['2', '1', '3'].
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', altKey: true }))).toBe(true)
    await tick()
    flushSync()

    // Second press immediately, BEFORE any `volumes-changed` refresh (the mock store never updates).
    // It must compute against the optimistic order, moving fav-1 from index 1 → 2: ['2', '3', '1'].
    // Without the local-first override it would re-read the stale store and wrongly emit ['2','1','3'].
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', altKey: true }))).toBe(true)
    await tick()
    flushSync()

    expect(reorderFavorites).toHaveBeenCalledTimes(2)
    expect(reorderFavorites).toHaveBeenLastCalledWith(['2', '3', '1'])
  })

  it('Alt+ArrowUp at the top favorite is a no-op (no persist)', async () => {
    const { instance } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()

    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'Home' }))).toBe(true)
    await tick()
    flushSync()

    // Already at the top: Alt+ArrowUp is consumed but persists nothing.
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowUp', altKey: true }))).toBe(true)
    await tick()
    flushSync()
    expect(reorderFavorites).not.toHaveBeenCalled()
  })

  it('Alt+ArrowDown on a non-favorite (real volume) does not reorder', async () => {
    const { instance } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()

    // End jumps to the last item: the real volume (Macintosh HD), not a favorite.
    expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'End' }))).toBe(true)
    await tick()
    flushSync()

    instance.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', altKey: true }))
    await tick()
    flushSync()
    expect(reorderFavorites).not.toHaveBeenCalled()
  })
})

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

    // Start the inline rename the way the native row menu does: the backend emits
    // `volume-context-action` with `rename-favorite` for the right-clicked favorite.
    const favRow = target.querySelector('.favorite-item') as HTMLElement
    expect(favRow).toBeTruthy()
    volumeContextActionHandler?.({ action: 'rename-favorite', volumeId: 'fav-1' })
    await tick()
    flushSync()

    expect(target.querySelector('.favorite-rename-input')).toBeTruthy()

    // The guard: keys the dropdown would otherwise eat must fall through (false)
    // so the rename textbox keeps them.
    for (const key of ['ArrowDown', 'ArrowUp', 'Home', 'End']) {
      expect(instance.handleKeyDown(new KeyboardEvent('keydown', { key }))).toBe(false)
    }
  })

  it('stops EVERY key (Space included) from bubbling out of the rename input to the pane', async () => {
    const { instance, target } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()

    const favRow = target.querySelector('.favorite-item') as HTMLElement
    expect(favRow).toBeTruthy()
    volumeContextActionHandler?.({ action: 'rename-favorite', volumeId: 'fav-1' })
    await tick()
    flushSync()

    const input = target.querySelector('.favorite-rename-input') as HTMLInputElement
    expect(input).toBeTruthy()

    // A document-level listener stands in for the pane's Space-selection / type-to-jump
    // DOM listeners. The rename input must stop ALL keys from reaching it.
    const leaked: string[] = []
    const docListener = (e: KeyboardEvent) => leaked.push(e.key)
    document.addEventListener('keydown', docListener)
    try {
      for (const key of [' ', 'a', 'ArrowDown', 'Backspace']) {
        input.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
      }
      await tick()
      flushSync()
      expect(leaked).toEqual([])
    } finally {
      document.removeEventListener('keydown', docListener)
    }
  })
})

/**
 * The switcher's server rows: the dot that says how live the place is, and the
 * Disconnect control that replaces Eject on one.
 *
 * ❗ The control's WORD is the point. "Eject" promises safe-to-unplug, which a
 * server can't deliver, and `saved` is the row where a control would have no
 * subject at all: nothing is open to close.
 */
describe('VolumeBreadcrumb server rows', () => {
  function serverRow(overrides: Record<string, unknown>) {
    return {
      id: 'sftp-nas-local-22-ada',
      name: 'Naspolya',
      path: 'sftp://ada@nas.local:22/srv/data',
      category: 'network',
      fsType: 'sftp',
      isEjectable: false,
      ...overrides,
    }
  }

  async function openWith(rows: unknown[]) {
    stubs.volumes = rows
    const { instance, target } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()
    return target
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
    disconnectPlace.mockClear()
    showVolumeRowContextMenu.mockClear()
  })

  it('gives a live place a Disconnect control, and clicking it drops the session', async () => {
    const target = await openWith([serverRow({ connectionState: 'direct' })])
    const button = target.querySelector('.volume-item .eject-button') as HTMLButtonElement
    expect(button).toBeTruthy()
    expect(button.getAttribute('aria-label')).toBe('Disconnect Naspolya')

    button.click()
    await tick()
    expect(disconnectPlace).toHaveBeenCalledWith('sftp-nas-local-22-ada')
  })

  it('gives a dropped-but-registered place one too: there is still a session to close', async () => {
    const target = await openWith([serverRow({ connectionState: 'disconnected' })])
    const button = target.querySelector('.volume-item .eject-button') as HTMLButtonElement
    expect(button.getAttribute('aria-label')).toBe('Disconnect Naspolya')
  })

  it('gives a saved place NO control: nothing is open to close', async () => {
    const target = await openWith([serverRow({ connectionState: 'saved' })])
    expect(target.querySelector('.volume-item .eject-button')).toBeNull()
    // ❗ And it reads as saved rather than as a failure: greyed, hollow dot.
    expect(target.querySelector('.volume-item.is-saved-place')).toBeTruthy()
  })

  // The dot's WORDS are pinned in `connection-tooltips.test.ts` (a pure call, no
  // hover timer); what the row owes is a class per state, since the stylesheet
  // paints each one differently and a missing rule renders an unpainted circle.
  it('paints one dot class per connection state', async () => {
    for (const state of ['direct', 'disconnected', 'needs_sign_in', 'needs_host_key_approval', 'saved'] as const) {
      document.body.innerHTML = ''
      const target = await openWith([serverRow({ connectionState: state })])
      expect(target.querySelector(`.volume-item .smb-indicator-${state}`), `no dot for ${state}`).toBeTruthy()
    }
  })

  it('shows the protocol in the filesystem slot, so a row says what it speaks', async () => {
    const target = await openWith([serverRow({ connectionState: 'direct' })])
    expect(target.querySelector('.volume-item .volume-fs')?.textContent).toBe('SFTP')
  })

  it('opens a server menu on right-click, with the row read as the caller sees it', async () => {
    const target = await openWith([serverRow({ connectionState: 'direct' })])
    // Row 0 is the hub ("Servers"); the place is the one after it.
    const row = target.querySelectorAll('.volume-item')[1] as HTMLElement
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    // One store read runs before the popup: let it settle. ❗ One, ❌ not two —
    // deciding whether a secret is stored would cost a Keychain read, and every
    // read of one can raise a system prompt in front of the menu appearing.
    await vi.waitFor(() => {
      expect(showVolumeRowContextMenu).toHaveBeenCalled()
    })
    expect(showVolumeRowContextMenu).toHaveBeenCalledWith('sftp-nas-local-22-ada', 'Naspolya', false, false, {
      showsDisconnect: true,
      isSaved: true,
      pinned: false,
    })
  })
})

/**
 * An eject that's still running. One took 10.5 s with no sign of life in a user's
 * log, which invites another click; the backend joins that click to the running
 * eject anyway, and the control shows the eject is underway so nobody has to try.
 */
describe('VolumeBreadcrumb eject in progress', () => {
  const drive = {
    id: 'volumes-backup',
    name: 'Backup',
    path: '/Volumes/Backup',
    category: 'attached_volume',
    isEjectable: true,
  }

  async function openWith(rows: unknown[]) {
    stubs.volumes = rows
    const { instance, target } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()
    return target
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
    stubs.ejecting = new Set()
    ejectVolume.mockClear()
  })

  afterEach(() => {
    stubs.volumes = null
    stubs.ejecting = new Set()
  })

  it('shows a drive whose eject is running as in progress, and a click starts nothing', async () => {
    stubs.ejecting = new Set([drive.id])
    const target = await openWith([drive])
    const button = target.querySelector('.volume-item .eject-button') as HTMLButtonElement
    expect(button.disabled).toBe(true)
    expect(button.getAttribute('aria-label')).toBe('Ejecting Backup…')
    expect(button.querySelector('.spinner')).toBeTruthy()

    button.click()
    await tick()
    expect(ejectVolume).not.toHaveBeenCalled()
  })

  it('says Disconnecting on a phone whose disconnect is running', async () => {
    const phone = {
      id: 'adb-pixel-7-a1b2c3d',
      name: 'Pixel 7',
      path: 'adb://R58M12345',
      category: 'mobile_device',
      fsType: 'adb',
      isEjectable: true,
      deviceReadiness: { kind: 'ready' },
    }
    stubs.ejecting = new Set([phone.id])
    const target = await openWith([phone])
    const button = target.querySelector('.volume-item .eject-button') as HTMLButtonElement
    expect(button.disabled).toBe(true)
    expect(button.getAttribute('aria-label')).toBe('Disconnecting Pixel 7…')
    expect(button.querySelector('.spinner')).toBeTruthy()
  })

  it('keeps an idle drive pressable, with its eject glyph', async () => {
    const target = await openWith([drive])
    const button = target.querySelector('.volume-item .eject-button') as HTMLButtonElement
    expect(button.disabled).toBe(false)
    expect(button.getAttribute('aria-label')).toBe('Eject Backup')
    expect(button.querySelector('.spinner')).toBeNull()

    button.click()
    await tick()
    expect(ejectVolume).toHaveBeenCalledWith(drive.id)
  })
})

describe('VolumeBreadcrumb phone rows', () => {
  function phoneRow(overrides: Record<string, unknown>) {
    return {
      id: 'adb-pixel-7-a1b2c3d',
      name: 'Pixel 7',
      path: 'adb://R58M12345',
      category: 'mobile_device',
      fsType: 'adb',
      isEjectable: true,
      ...overrides,
    }
  }

  async function openWith(rows: unknown[]) {
    stubs.volumes = rows
    const { instance, target } = mountBreadcrumb()
    instance.open()
    await tick()
    flushSync()
    return target
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
    ejectVolume.mockClear()
  })

  // ❗ `adb` has no per-client detach, so nothing here is made safe to unplug.
  // The ACTION is still the ordinary eject path (which for ADB answers
  // `DeviceDisconnect`); only the word changes.
  it('says Disconnect on a phone, and still runs the eject path', async () => {
    const target = await openWith([phoneRow({ deviceReadiness: { kind: 'ready' } })])
    const button = target.querySelector('.volume-item .eject-button') as HTMLButtonElement
    expect(button.getAttribute('aria-label')).toBe('Disconnect Pixel 7')

    button.click()
    await tick()
    expect(ejectVolume).toHaveBeenCalledWith('adb-pixel-7-a1b2c3d')
  })

  // A regression anchor: it passes with the readiness gate absent too, and that
  // is the point — it is what fails the day someone "tidies up" by disabling
  // every non-ready row.
  it('keeps a phone waiting for its Allow tap openable, and says what it waits for', async () => {
    const target = await openWith([phoneRow({ deviceReadiness: { kind: 'waiting_for_authorization' } })])
    const row = target.querySelector('.volume-item') as HTMLElement
    expect(row.classList.contains('is-unavailable')).toBe(false)
    expect(row.getAttribute('aria-disabled')).toBeNull()
  })

  it('greys a phone the daemon lists but cannot use, and refuses to open it', async () => {
    const target = await openWith([phoneRow({ deviceReadiness: { kind: 'unavailable', reason: 'offline' } })])
    const row = target.querySelector('.volume-item') as HTMLElement
    expect(row.classList.contains('is-unavailable')).toBe(true)
    expect(row.getAttribute('aria-disabled')).toBe('true')

    // Clicking it leaves the dropdown where it was: there is nothing to open.
    row.click()
    await tick()
    expect(target.querySelector('.volume-dropdown')).toBeTruthy()
  })
})
