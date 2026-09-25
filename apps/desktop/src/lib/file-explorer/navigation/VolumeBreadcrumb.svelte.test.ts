/**
 * Behavioral tests for the chip (`VolumeBreadcrumb.svelte`) and the volume switcher it
 * hosts (`VolumeChooserMenu.svelte`, the house `Menu`). They're the behavior contract the
 * M2 port had to keep, so they mount the chip and drive it the way a person does — real
 * keydowns, real clicks.
 *
 * The chip's OTHER menu is `FavoritesMenu.svelte.test.ts`: the favorites cases moved
 * there whole when the favorites left the switcher, ❌ they weren't dropped.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'
import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'
import type { VolumeChangePayload } from '../pane/types'

const ejectVolume = vi.fn(() => Promise.resolve())
const disconnectPlace = vi.fn(() => Promise.resolve(true))
const hasServerSecret = vi.fn(() => Promise.resolve(true))
const listSavedServers = vi.fn(() =>
  Promise.resolve([{ id: 'sftp-nas-local-22-ada', places: [{ volumeId: 'sftp-nas-local-22-ada' }] }]),
)

/**
 * The volume list the store mock answers with. Swappable so the server-row block
 * can put a place in the switcher without shifting the favorite indices every
 * other block here counts on.
 */
const stubs = vi.hoisted(() => ({
  volumes: null as unknown[] | null,
  ejecting: new Set<string>(),
  /**
   * The per-drive index status the freshness badge renders from. `null` means the
   * fetch answers "nothing to show", so no row grows a badge — which is what every
   * block here except the drive-index one wants.
   */
  indexStatus: null as Record<string, unknown> | null,
  /** What `resolvePathVolume` answers: the volume the checkmark tracks. */
  containingVolumeId: 'root',
  /**
   * What Rust answers for a share's "Use Cmdr's fast direct connection" switch. `null`
   * means no switch, so no row: what every block here except the share submenu's wants.
   */
  directSwitch: null as boolean | null,
}))

const connectDirectly = vi.fn(() => Promise.resolve({ kind: 'connected' }))
const setSmbDirectConnectionEnabled = vi.fn(() => Promise.resolve('saved'))

// What checking a share's direct-connection switch runs. The component is the only importer here, so a factory
// mock costs nothing else.
vi.mock('../network/direct-connect', () => ({
  connectDirectly: (...args: unknown[]) => connectDirectly(...(args as [])),
}))

// The drive-index badge's own IPC module. `drive-index-manager.svelte.ts` imports this
// SUB-PATH, not the `$lib/tauri-commands` barrel mocked below, so it needs its own mock.
vi.mock('$lib/tauri-commands/indexing', () => ({
  getVolumeIndexStatusById: () =>
    Promise.resolve(stubs.indexStatus ? { status: 'ok', data: stubs.indexStatus } : { status: 'timedOut' }),
  onIndexFreshnessChanged: () => Promise.resolve(() => {}),
  onIndexScanStarted: () => Promise.resolve(() => {}),
  onIndexScanComplete: () => Promise.resolve(() => {}),
}))

// The test DOM has no layout, so no scroll: a stub keeps `scrollHighlightedIntoView`
// from throwing into a floating promise, and lets a test watch it.
Element.prototype.scrollIntoView = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: { id: stubs.containingVolumeId, path: '/' } })),
  upgradeToSmbVolume: vi.fn(() => Promise.resolve({ status: 'success' })),
  ejectVolume: (...args: unknown[]) => ejectVolume(...(args as [])),
  getVolumeSpace: vi.fn(() => Promise.resolve(null)),
  systemHasSavedSmbPassword: vi.fn(() => Promise.resolve(false)),
  upgradeToSmbVolumeUsingSavedPassword: vi.fn(() => Promise.resolve({ status: 'success' })),
  removeFavorite: vi.fn(() => Promise.resolve()),
  renameFavorite: vi.fn(() => Promise.resolve()),
  reorderFavorites: vi.fn(() => Promise.resolve()),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
  disconnectPlace: (...args: unknown[]) => disconnectPlace(...(args as [])),
  forgetServer: vi.fn(() => Promise.resolve(true)),
  forgetServerSecret: vi.fn(() => Promise.resolve(true)),
  hasServerSecret: (...args: unknown[]) => hasServerSecret(...(args as [])),
  listSavedServers: () => listSavedServers(),
  getSmbDirectConnectionEnabled: () => Promise.resolve(stubs.directSwitch),
  setSmbDirectConnectionEnabled: (...args: unknown[]) => setSmbDirectConnectionEnabled(...(args as [])),
  addFavorite: vi.fn(() => Promise.resolve()),
  trackEvent: vi.fn(() => Promise.resolve()),
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
  // `volume-capabilities` reads it to classify a `.git`-portal path, which the favorites
  // menu's add row asks about.
  getShowVirtualGitPortal: () => false,
  // The drive-index badge's master switch. On, so a row carrying a status renders
  // the badge with its actions rather than the "indexing is off" note.
  getDriveIndexingEnabled: () => true,
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
  openVolumeChooser: () => void
  toggleVolumeChooser: () => void
  toggleFavoritesMenu: () => void
  closeHeaderMenu: () => void
  isHeaderMenuOpen: () => boolean
}

// ============================================================================
// The switcher is the house `Menu`: it PORTALS to `document.body`, carries the documented
// `data-*` hooks (`$lib/ui/DETAILS.md` § Menu), and catches keys on its own document capture
// listener. So these look at the document rather than the mount target, select on hooks
// rather than on classes, and press keys for real.
// ============================================================================

/** The open switcher's surface, or null when none is open. */
function menuSurface(): HTMLElement | null {
  return document.querySelector('[data-menu]')
}

/**
 * The switcher's own surface, by name. A drive-index badge in a row opens a menu of its own,
 * which is a `[data-menu]` too, so ❌ `menuSurface()` can't answer "is the switcher still up"
 * once one is open: it would happily return the badge's menu instead.
 */
function switcherSurface(): HTMLElement | null {
  return document.querySelector('[data-menu][aria-label="Volume switcher"]')
}

/** The main list's rows. The submenu is its own surface, so its row never lands here. */
function menuRows(): NodeListOf<HTMLElement> {
  return document.querySelectorAll('[data-menu] [data-menu-row]')
}

/**
 * The VOLUME rows alone. The switcher now leads with a "See N favorites" row that swaps in
 * the favorites menu, so a pin counting volume rows by index has to skip it — ❌ don't fold
 * this back into `menuRows`, which several pins use to count what the menu really renders.
 */
function volumeRows(): HTMLElement[] {
  return [...menuRows()].filter((row) => row.getAttribute('data-menu-row') !== 'favorites:see')
}

function isHighlighted(row: Element | null | undefined): boolean {
  return row?.hasAttribute('data-highlighted') ?? false
}

/**
 * Press a key the way the app does. Returns true when the menu claimed it, which is the
 * answer the switcher's own `handleKeyDown` gave before the primitive took the keyboard.
 */
function press(key: string, modifiers: KeyboardEventInit = {}): boolean {
  return !document.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...modifiers }))
}

// A switcher left open outlives its target div: it's portaled, and its key listener lives on
// the document. Escape closes whichever ones are open before the next test mounts — twice,
// because the first one only backs out of an open submenu.
afterEach(() => {
  press('Escape')
  press('Escape')
  document.body.innerHTML = ''
})

interface BreadcrumbProps {
  volumeId?: string
  currentPath?: string
  onVolumeChange?: (change: VolumeChangePayload) => void
}

function mountBreadcrumb(props: BreadcrumbProps = {}): { instance: BreadcrumbInstance; target: HTMLDivElement } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(VolumeBreadcrumb, {
    target,
    props: { paneId: 'left' as const, volumeId: 'root', currentPath: '/Users/test', ...props },
  }) as unknown as BreadcrumbInstance
  // Settle the chip's `bind:this`: the menu hangs under that element, so `open()` called
  // before the binding lands would have nothing to anchor to.
  flushSync()
  return { instance, target }
}

/** Opens the switcher over `rows` and hands back the mounted pieces. */
async function openWithRows(
  rows: unknown[],
  props: BreadcrumbProps = {},
): Promise<{ instance: BreadcrumbInstance; target: HTMLDivElement }> {
  stubs.volumes = rows
  const mounted = mountBreadcrumb(props)
  mounted.instance.openVolumeChooser()
  await tick()
  flushSync()
  return mounted
}

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
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
    disconnectPlace.mockClear()
  })

  it('gives a live place a Disconnect control, and clicking it drops the session', async () => {
    await openWith([serverRow({ connectionState: 'direct' })])
    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
    expect(button).toBeTruthy()
    expect(button.getAttribute('aria-label')).toBe('Disconnect Naspolya')

    button.click()
    await tick()
    expect(disconnectPlace).toHaveBeenCalledWith('sftp-nas-local-22-ada')
  })

  it('gives a dropped-but-registered place one too: there is still a session to close', async () => {
    await openWith([serverRow({ connectionState: 'disconnected' })])
    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
    expect(button.getAttribute('aria-label')).toBe('Disconnect Naspolya')
  })

  it('gives a saved place NO control: nothing is open to close', async () => {
    await openWith([serverRow({ connectionState: 'saved' })])
    expect(document.querySelector('[data-menu-row] .eject-button')).toBeNull()
    // ❗ And it reads as saved rather than as a failure: greyed, hollow dot.
    expect(document.querySelector('[data-menu-row] .is-saved-place')).toBeTruthy()
  })

  // The dot's WORDS are pinned in `connection-tooltips.test.ts` (a pure call, no
  // hover timer); what the row owes is a class per state, since the stylesheet
  // paints each one differently and a missing rule renders an unpainted circle.
  it('paints one dot class per connection state', async () => {
    for (const state of ['direct', 'disconnected', 'needs_sign_in', 'needs_host_key_approval', 'saved'] as const) {
      document.body.innerHTML = ''
      await openWith([serverRow({ connectionState: state })])
      expect(document.querySelector(`[data-menu-row] .smb-indicator-${state}`), `no dot for ${state}`).toBeTruthy()
    }
  })

  it('shows the protocol in the filesystem slot, so a row says what it speaks', async () => {
    await openWith([serverRow({ connectionState: 'direct' })])
    expect(document.querySelector('[data-menu-row] .volume-fs')?.textContent).toBe('SFTP')
  })

  /** Right-clicks the place (row 0 is the hub, "Servers") and waits for its submenu to be placed. */
  async function openPlaceSubmenu(): Promise<HTMLElement[]> {
    // The saved-server store is read on open: let it land first.
    await new Promise((resolve) => setTimeout(resolve, 0))
    volumeRows()[1].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    await tick()
    flushSync()
    await tick()
    await tick()
    return [...document.querySelectorAll<HTMLElement>('[data-menu-submenu] [data-menu-row]')]
  }

  it('opens the place’s submenu on right-click: the server actions, read off the row and the saved store', async () => {
    await openWith([serverRow({ connectionState: 'direct' })])
    const rows = await openPlaceSubmenu()
    expect(rows.map((row) => row.textContent.trim())).toEqual([
      'Open',
      'Edit server…',
      'Disconnect',
      'Pin to switcher',
      'Forget saved password',
      'Forget server',
    ])
    // ❗ Deciding whether a secret is stored would cost a Keychain read, and every read of
    // one can raise a system prompt in front of the menu appearing.
    expect(hasServerSecret).not.toHaveBeenCalled()
  })

  it('disconnects from the submenu and leaves the switcher up, like the row’s own control', async () => {
    await openWith([serverRow({ connectionState: 'direct' })])
    await openPlaceSubmenu()
    document
      .querySelector('[data-menu-submenu] [data-menu-row="row:sftp-nas-local-22-ada:disconnect"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => {
      expect(disconnectPlace).toHaveBeenCalledWith('sftp-nas-local-22-ada')
    })
    expect(switcherSurface()).toBeTruthy()
  })

  it('opens the place in THIS pane from the submenu’s Open, the way a click on the row does', async () => {
    const onVolumeChange = vi.fn()
    stubs.volumes = [serverRow({ connectionState: 'direct' })]
    const { instance } = mountBreadcrumb({ onVolumeChange })
    instance.openVolumeChooser()
    await tick()
    flushSync()
    await openPlaceSubmenu()
    document
      .querySelector('[data-menu-submenu] [data-menu-row="row:sftp-nas-local-22-ada:open"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => {
      expect(onVolumeChange).toHaveBeenCalledWith(expect.objectContaining({ volumeId: 'sftp-nas-local-22-ada' }))
    })
  })
})

/**
 * The CHIP's own control, which is the same decision as a switcher row's and has to
 * stay that way: a user reported pressing the chip's button on an open SFTP place and
 * being told the drive isn't removable, while the row for the same volume, one click
 * deeper in the switcher, disconnected it (`ERR-P7F5Q`).
 */
describe('VolumeBreadcrumb chip control', () => {
  const place = {
    id: 'sftp-nas-local-22-ada',
    name: 'Naspolya',
    path: 'sftp://ada@nas.local:22/srv/data',
    category: 'network',
    fsType: 'sftp',
    isEjectable: false,
    connectionState: 'direct',
  }

  /** Mounts the chip ON a volume, which is what makes it `currentVolume`. */
  async function mountOn(volume: Record<string, unknown>) {
    stubs.volumes = [volume]
    stubs.containingVolumeId = volume.id as string
    mountBreadcrumb({ volumeId: volume.id as string, currentPath: '/srv/data' })
    // `resolvePathVolume` is a round trip, and `containingVolumeId` lands after it.
    await tick()
    await tick()
    flushSync()
  }

  /** The chip's own detach control, or null when it offers none. */
  function chipControl(): HTMLButtonElement | null {
    return document.querySelector<HTMLButtonElement>('.breadcrumb-eject-button')
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    ejectVolume.mockClear()
    disconnectPlace.mockClear()
  })

  afterEach(() => {
    stubs.volumes = null
    stubs.containingVolumeId = 'root'
  })

  it('says Disconnect on a server, and closes the session rather than asking for an eject', async () => {
    await mountOn(place)

    const button = chipControl()
    expect(button).toBeTruthy()
    expect(button?.getAttribute('aria-label')).toBe('Disconnect Naspolya')

    button?.click()
    await tick()
    expect(disconnectPlace).toHaveBeenCalledWith(place.id)
    // Pre-fix this asked the backend to eject, which can only answer `NotEjectable`.
    expect(ejectVolume).not.toHaveBeenCalled()
  })

  it('still ejects a removable drive from the chip', async () => {
    await mountOn({ id: 'disk4', name: 'Backup', path: '/Volumes/Backup', category: 'volume', isEjectable: true })

    const button = chipControl()
    expect(button?.getAttribute('aria-label')).toBe('Eject Backup')

    button?.click()
    await tick()
    expect(ejectVolume).toHaveBeenCalled()
    expect(disconnectPlace).not.toHaveBeenCalled()
  })

  it('offers nothing on a saved place, which has no session to close', async () => {
    await mountOn({ ...place, connectionState: 'saved' })
    expect(chipControl()).toBeNull()
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
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()
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
    await openWith([drive])
    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
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
    await openWith([phone])
    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
    expect(button.disabled).toBe(true)
    expect(button.getAttribute('aria-label')).toBe('Disconnecting Pixel 7…')
    expect(button.querySelector('.spinner')).toBeTruthy()
  })

  it('keeps an idle drive pressable, with its eject glyph', async () => {
    await openWith([drive])
    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
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
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()
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
    await openWith([phoneRow({ deviceReadiness: { kind: 'ready' } })])
    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
    expect(button.getAttribute('aria-label')).toBe('Disconnect Pixel 7')

    button.click()
    await tick()
    expect(ejectVolume).toHaveBeenCalledWith('adb-pixel-7-a1b2c3d')
  })

  // A regression anchor: it passes with the readiness gate absent too, and that
  // is the point — it is what fails the day someone "tidies up" by disabling
  // every non-ready row.
  it('keeps a phone waiting for its Allow tap openable, and says what it waits for', async () => {
    await openWith([phoneRow({ deviceReadiness: { kind: 'waiting_for_authorization' } })])
    const row = volumeRows()[0]
    expect(row.hasAttribute('data-disabled')).toBe(false)
    expect(row.getAttribute('aria-disabled')).toBeNull()
  })

  it('greys a phone the daemon lists but cannot use, and refuses to open it', async () => {
    await openWith([phoneRow({ deviceReadiness: { kind: 'unavailable', reason: 'offline' } })])
    const row = volumeRows()[0]
    // Greyed and unopenable is what `disabled` means to the menu: it paints the row down,
    // says so to assistive tech, and never activates it.
    expect(row.hasAttribute('data-disabled')).toBe(true)
    expect(row.getAttribute('aria-disabled')).toBe('true')

    // Clicking it leaves the dropdown where it was: there is nothing to open.
    row.click()
    await tick()
    expect(menuSurface()).toBeTruthy()
  })
})

/**
 * Where the cursor sits the moment the switcher opens. It lands on the row wearing
 * the checkmark — the volume the pane's path actually sits on — so Enter re-opens
 * where you already are instead of jumping somewhere else. With nothing checked,
 * it falls back to the first row.
 */
describe('VolumeBreadcrumb highlight on open', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
    stubs.containingVolumeId = 'root'
  })

  it('starts on the row carrying the checkmark, not on row 0', async () => {
    // The pane sits on the LAST drive, so the checked row is neither the first volume
    // row nor the first row of the menu.
    stubs.containingVolumeId = 'volumes-b'
    stubs.volumes = [
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      { id: 'volumes-a', name: 'Alpha', path: '/Volumes/Alpha', category: 'attached_volume', isEjectable: true },
      { id: 'volumes-b', name: 'Beta', path: '/Volumes/Beta', category: 'attached_volume', isEjectable: true },
    ]
    const { instance, target } = mountBreadcrumb()
    // The checkmark tracks `containingVolumeId`, which `resolvePathVolume` fills in
    // asynchronously; the chip naming the volume is that answer having landed.
    await vi.waitFor(() => {
      expect(target.querySelector('.volume-name')?.textContent).toContain('Beta')
    })
    instance.openVolumeChooser()
    await tick()
    flushSync()

    // Two drives lead it, so the containing volume is volume row 2 — the case that
    // tells "the checked row" apart from "the first row".
    const rows = volumeRows()
    expect(rows[2].hasAttribute('data-checked')).toBe(true)
    expect(isHighlighted(rows[2])).toBe(true)
    expect(isHighlighted(rows[0])).toBe(false)
  })

  it('falls back to the menu’s first row when no row is the containing volume', async () => {
    // A phone: it isn't the pane's volume, and neither is the synthetic Servers row,
    // so nothing is checked. ❗ The fallback is the FIRST row of the menu, which since
    // M3 is the "See N favorites" row rather than the first volume.
    await openWithRows([
      { id: 'mtp-1', name: 'Pixel', path: 'mtp://pixel', category: 'mobile_device', isEjectable: true },
    ])

    expect(document.querySelector('[data-menu-row][data-checked]')).toBeNull()
    expect(isHighlighted(menuRows()[0])).toBe(true)
    expect(menuRows()[0].getAttribute('data-menu-row')).toBe('favorites:see')
  })
})

/**
 * One cursor, one input device. Touching the keyboard suppresses hover highlighting
 * so the mouse's resting position can't show a second cursor; moving the pointer
 * more than 5 px hands control back and takes the highlight with it.
 */
describe('VolumeBreadcrumb keyboard vs pointer mode', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    // Three volume rows, so "another row" is a row the cursor is genuinely not on.
    stubs.volumes = [
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      { id: 'volumes-a', name: 'Alpha', path: '/Volumes/Alpha', category: 'attached_volume', isEjectable: true },
      { id: 'volumes-b', name: 'Beta', path: '/Volumes/Beta', category: 'attached_volume', isEjectable: true },
    ]
  })

  it('suppresses hover highlighting once the keyboard has the cursor', async () => {
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()

    press('Home')
    await tick()
    flushSync()

    const dropdown = menuSurface()
    expect(dropdown?.classList.contains('keyboard-mode')).toBe(true)

    // Hovering another row must not move the cursor off row 0 while keyboard mode holds.
    // ❗ `menuRows`, not `volumeRows`: Home lands on the menu's first row, which is the
    // "See N favorites" row the switcher now leads with.
    const rows = [...menuRows()]
    rows[2].dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    await tick()
    flushSync()
    expect(isHighlighted(rows[0])).toBe(true)
    expect(isHighlighted(rows[2])).toBe(false)
  })

  it('a pointer move over 5 px leaves keyboard mode and takes the highlight to the row under the cursor', async () => {
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()

    press('Home')
    await tick()
    flushSync()

    const dropdown = menuSurface()
    const rows = [...menuRows()]

    // The first move only records where the pointer was: a mouse sitting still under
    // a moving list must not steal the cursor.
    rows[2].dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: 100, clientY: 100 }))
    await tick()
    flushSync()
    expect(dropdown?.classList.contains('keyboard-mode')).toBe(true)
    expect(isHighlighted(rows[0])).toBe(true)

    // A move past the 5 px threshold is a real gesture: the pointer takes over.
    rows[2].dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: 100, clientY: 120 }))
    await tick()
    flushSync()
    expect(dropdown?.classList.contains('keyboard-mode')).toBe(false)
    expect(isHighlighted(rows[2])).toBe(true)
    expect(isHighlighted(rows[0])).toBe(false)
  })
})

/**
 * A share's submenu, which holds its "Use Cmdr's fast direct connection" switch. Pointer
 * and keyboard both reach it, and while it's up it owns the only visible cursor.
 */
describe('VolumeBreadcrumb share submenu', () => {
  const share = {
    id: 'volumes-share',
    name: 'Share',
    path: '/Volumes/Share',
    category: 'network',
    fsType: 'smbfs',
    connectionState: 'os_mount',
    isEjectable: true,
  }

  /** Row 0 is the synthetic Servers hub; the share is the row after it. */
  const SHARE_ROW = 1

  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
    stubs.directSwitch = true
    connectDirectly.mockClear()
    setSmbDirectConnectionEnabled.mockClear()
  })

  afterEach(() => {
    stubs.directSwitch = null
  })

  /** Opens the switcher on `rows`, then lets the switch values Rust answers land. */
  async function openWithSwitches(rows: unknown[]): Promise<{ instance: BreadcrumbInstance }> {
    const opened = await openWithRows(rows)
    await settle()
    return opened
  }

  async function settle(): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, 0))
    await tick()
    flushSync()
  }

  /** Highlights the share row, then opens its submenu. */
  async function openShareSubmenu(): Promise<void> {
    // Two steps down: the menu leads with the "See N favorites" row, then the hub.
    press('ArrowDown')
    press('ArrowDown')
    await tick()
    flushSync()
    expect(press('ArrowRight')).toBe(true)
    await tick()
    flushSync()
  }

  function submenuRows(): HTMLElement[] {
    return [...document.querySelectorAll<HTMLElement>('[data-menu-submenu] [data-menu-row]')]
  }

  it('opens the submenu when the pointer rests on the row', async () => {
    await openWithSwitches([share])
    expect(document.querySelector('[data-menu-submenu]')).toBeNull()

    volumeRows()[SHARE_ROW].dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    await tick()
    flushSync()
    expect(document.querySelector('[data-menu-submenu]')).toBeTruthy()
  })

  // Switch ON, yet the share is still on the macOS mount (the auto upgrade couldn't dial): the
  // checked box alone would leave no one-click way to connect, so a one-shot fix sits between
  // Eject and the switch, and only the switch gets the rule.
  it('an OS-mounted share with its switch on offers Eject, then “Connect directly now”, then the switch below a rule', async () => {
    await openWithSwitches([share])
    await openShareSubmenu()

    const rows = submenuRows()
    expect(rows.map((row) => row.textContent.trim())).toEqual([
      'Eject (Share)',
      'Connect directly now',
      'Use Cmdr’s fast direct connection',
    ])
    expect(rows[1].previousElementSibling?.getAttribute('role')).not.toBe('separator')
    expect(rows[2].hasAttribute('data-checked')).toBe(true)
    expect(rows[2].previousElementSibling?.getAttribute('role')).toBe('separator')
  })

  it('“Connect directly now” runs "Connect directly" for the share, leaving the switch alone', async () => {
    await openWithSwitches([share])
    await openShareSubmenu()

    press('ArrowDown')
    expect(press('Enter')).toBe(true)
    await settle()
    expect(connectDirectly).toHaveBeenCalledWith({ volumeId: 'volumes-share', shareName: 'Share' })
    expect(setSmbDirectConnectionEnabled).not.toHaveBeenCalled()
  })

  // Checking the box already connects, so an OFF switch needs no second door.
  it('an OS-mounted share with its switch off holds Eject, then the switch below a rule', async () => {
    stubs.directSwitch = false
    await openWithSwitches([share])
    await openShareSubmenu()

    const rows = submenuRows()
    expect(rows.map((row) => row.textContent.trim())).toEqual(['Eject (Share)', 'Use Cmdr’s fast direct connection'])
    expect(rows[1].hasAttribute('data-checked')).toBe(false)
    expect(rows[1].previousElementSibling?.getAttribute('role')).toBe('separator')
  })

  it('a direct share with its switch on has nothing to fix', async () => {
    await openWithSwitches([{ ...share, connectionState: 'direct' }])
    await openShareSubmenu()
    expect(submenuRows().map((row) => row.textContent.trim())).not.toContain('Connect directly now')
  })

  it('a share Rust has no switch for gets its Eject alone', async () => {
    stubs.directSwitch = null
    await openWithSwitches([share])
    await openShareSubmenu()
    expect(submenuRows().map((row) => row.textContent.trim())).toEqual(['Eject (Share)'])
  })

  it('ejects from the submenu and leaves the switcher up, so several can go in a row', async () => {
    await openWithSwitches([share])
    await openShareSubmenu()
    expect(press('Enter')).toBe(true)
    await settle()
    expect(ejectVolume).toHaveBeenCalledWith('volumes-share')
    expect(switcherSurface()).toBeTruthy()
    expect(document.querySelector('[data-menu-submenu]')).toBeNull()
  })

  it('checking the switch on an OS-mounted share runs "Connect directly" for it', async () => {
    stubs.directSwitch = false
    await openWithSwitches([share])
    await openShareSubmenu()

    // Past Eject to the switch.
    press('ArrowDown')
    expect(press('Enter')).toBe(true)
    await settle()
    expect(setSmbDirectConnectionEnabled).toHaveBeenCalledWith('volumes-share', true)
    // The share's NAME rides along, read while the row is still listed: it's what
    // words the answer if the share goes away before the backend gets there.
    expect(connectDirectly).toHaveBeenCalledWith({ volumeId: 'volumes-share', shareName: 'Share' })
  })

  it('ArrowRight opens it at the highlight, and ArrowLeft closes it again', async () => {
    await openWithSwitches([share])
    await openShareSubmenu()
    expect(document.querySelector('[data-menu-submenu]')).toBeTruthy()

    expect(press('ArrowLeft')).toBe(true)
    await tick()
    flushSync()
    expect(document.querySelector('[data-menu-submenu]')).toBeNull()
  })

  // Escape belongs to the submenu while the submenu is up: it backs out one level
  // rather than dropping the whole switcher.
  it('Escape closes the submenu and leaves the switcher open', async () => {
    const { instance } = await openWithSwitches([share])
    await openShareSubmenu()

    expect(press('Escape')).toBe(true)
    await tick()
    flushSync()
    expect(document.querySelector('[data-menu-submenu]')).toBeNull()
    expect(menuSurface()).toBeTruthy()
    expect(instance.isHeaderMenuOpen()).toBe(true)
  })

  it('suppresses the parent row highlight while the submenu is up (one cursor at a time)', async () => {
    await openWithSwitches([share])
    press('ArrowDown')
    press('ArrowDown')
    await tick()
    flushSync()
    const rows = volumeRows()
    expect(isHighlighted(rows[SHARE_ROW])).toBe(true)

    press('ArrowRight')
    await tick()
    flushSync()
    expect(document.querySelector('[data-menu-submenu] [data-menu-row][data-highlighted]')).toBeTruthy()
    expect(isHighlighted(rows[SHARE_ROW])).toBe(false)
  })
})

/**
 * Where the dropdown lands. It's `position: fixed`, so it gets its coordinates from
 * the anchor's rect and a height budget from the space left below it; the highlighted
 * row is then scrolled into that budget.
 */
describe('VolumeBreadcrumb dropdown placement', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
  })

  it('sets top, left, and a max height from the anchor rect and the space below it', async () => {
    const { instance, target } = mountBreadcrumb()
    // The test DOM has no layout engine, so the anchor reports its own geometry.
    const anchor = target.querySelector('.volume-name') as HTMLElement
    anchor.getBoundingClientRect = () =>
      ({ top: 30, bottom: 50, left: 12, right: 200, width: 188, height: 20, x: 12, y: 30 }) as DOMRect

    instance.openVolumeChooser()
    // `querySelector<HTMLElement>` rather than an `as` cast: eslint's
    // `no-unnecessary-type-assertion` fixer strips the cast, and `Element.style`
    // then can't resolve (`docs/testing.md` § "Merging test files").
    await vi.waitFor(() => {
      expect(menuSurface()?.style.top).toBeTruthy()
    })
    const dropdown = menuSurface()

    expect(dropdown?.style.top).toBe('54px') // the anchor's bottom, plus 4px of air
    expect(dropdown?.style.left).toBe('12px') // flush with the anchor's left edge
    expect(dropdown?.style.maxHeight).toBe(`${String(window.innerHeight - 54 - 8)}px`)
  })

  it('scrolls the row the keyboard just landed on into view', async () => {
    stubs.volumes = [
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      { id: 'volumes-a', name: 'Alpha', path: '/Volumes/Alpha', category: 'attached_volume', isEjectable: true },
    ]
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()

    // `menuRows`: the cursor opens on the menu's first row (the favorites one), so one
    // ArrowDown lands on the row after it.
    const rows = [...menuRows()]
    const scrollIntoView = vi.fn()
    rows[1].scrollIntoView = scrollIntoView

    press('ArrowDown')
    await vi.waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled()
    })
    // `nearest`: bring it in if it's out, ❌ never re-centre a row already on screen.
    expect(scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' })
  })
})

/**
 * A control sitting inside a row does its own job and nothing else. Each of these
 * would otherwise activate the row it sits on, switching the pane's volume behind
 * the user's back.
 */
describe('VolumeBreadcrumb row controls do not activate their row', () => {
  const drive = {
    id: 'volumes-backup',
    name: 'Backup',
    path: '/Volumes/Backup',
    category: 'attached_volume',
    isEjectable: true,
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = null
    stubs.indexStatus = null
    ejectVolume.mockClear()
    disconnectPlace.mockClear()
  })

  afterEach(() => {
    stubs.indexStatus = null
  })

  it('the eject button ejects, without navigating the pane or closing the switcher', async () => {
    const onVolumeChange = vi.fn()
    await openWithRows([drive], { onVolumeChange })

    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
    button.click()
    await tick()
    flushSync()

    expect(ejectVolume).toHaveBeenCalledWith('volumes-backup')
    expect(onVolumeChange).not.toHaveBeenCalled()
    // Still open, so several drives can be ejected in a row.
    expect(menuSurface()).toBeTruthy()
  })

  // ❗ The chip's own eject button sits BESIDE the anchor, so the menu's outside-pointer-down
  // close treated it as outside and shut the list before the button could act. Ejecting
  // deliberately leaves the list open, so several drives can go in a row.
  it('the CHIP eject button ejects without closing the open list', async () => {
    const onVolumeChange = vi.fn()
    // The pane's own volume, so the chip itself carries a detach control.
    const { target } = await openWithRows(
      [{ id: 'root', name: 'Backup', path: '/', category: 'attached_volume', isEjectable: true }],
      { onVolumeChange },
    )

    const button = target.querySelector('.eject-button') as HTMLButtonElement
    expect(button).toBeTruthy()
    button.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    button.click()
    await tick()
    flushSync()

    expect(ejectVolume).toHaveBeenCalledWith('root')
    expect(onVolumeChange).not.toHaveBeenCalled()
    expect(menuSurface()).toBeTruthy()
  })

  it('a server row’s Disconnect drops the session, without navigating the pane', async () => {
    const onVolumeChange = vi.fn()
    await openWithRows(
      [
        {
          id: 'sftp-nas-local-22-ada',
          name: 'Naspolya',
          path: 'sftp://ada@nas.local:22/srv/data',
          category: 'network',
          fsType: 'sftp',
          isEjectable: false,
          connectionState: 'direct',
        },
      ],
      { onVolumeChange },
    )

    const button = document.querySelector('[data-menu-row] .eject-button') as HTMLButtonElement
    button.click()
    await tick()
    flushSync()

    expect(disconnectPlace).toHaveBeenCalledWith('sftp-nas-local-22-ada')
    expect(onVolumeChange).not.toHaveBeenCalled()
    expect(menuSurface()).toBeTruthy()
  })

  it('the drive-index badge opens its own menu, without navigating the pane', async () => {
    stubs.indexStatus = {
      volumeId: drive.id,
      enabled: true,
      freshness: 'fresh',
      failure: null,
      scanCompletedAt: 1_750_000_000,
      scanDurationMs: 134_000,
      coalescedSignalsSinceSweep: 0,
      unreadableLocations: 0,
      unreadableRetried: false,
      nextSweepDueAt: null,
      liveWatch: true,
    }
    const onVolumeChange = vi.fn()
    await openWithRows([drive], { onVolumeChange })

    // The status fetch is async, so the badge arrives a beat after the row.
    await vi.waitFor(() => {
      expect(document.querySelector('[data-menu-row] .drive-index-badge')).not.toBeNull()
    })
    document.querySelector<HTMLButtonElement>('[data-menu-row] .drive-index-badge')?.click()
    await tick()
    await tick()
    flushSync()

    const badgeMenu = document.querySelector('[data-menu][aria-label="Drive index status"]')
    expect(badgeMenu).toBeTruthy()
    expect(onVolumeChange).not.toHaveBeenCalled()
    expect(switcherSurface()).toBeTruthy()

    // ❗ And it STAYS up while the badge's menu is used. The badge's menu portals to the body,
    // so without the house `Menu`'s nested-menu rule a pointer-down in it reads as "outside"
    // and closes the list the badge was sitting in ($lib/ui/DETAILS.md § Menu).
    badgeMenu?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    await tick()
    flushSync()
    expect(switcherSurface()).toBeTruthy()
  })
})
