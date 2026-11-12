/**
 * A row's action list: which entries a volume, server, or favorite row offers, in what
 * order, and which ones a running transfer greys. The ONE list every door shows (the row's
 * → submenu, its right-click, the servers hub's right-click), so it's pinned here rather
 * than per surface.
 */
import { describe, it, expect, vi, beforeAll, afterAll, beforeEach } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'

const ejectVolume = vi.fn(() => Promise.resolve('diskEjected'))
const ejectAllOnDisk = vi.fn(() => Promise.resolve('diskEjected'))
const disconnectPlace = vi.fn(() => Promise.resolve(true))
const setPlacePinned = vi.fn(() => Promise.resolve(true))
const busy = new Set<string>()

vi.mock('#lib/tauri-commands/index.js', () => ({
  ejectVolume: (...args: unknown[]) => ejectVolume(...(args as [])),
  ejectAllOnDisk: (...args: unknown[]) => ejectAllOnDisk(...(args as [])),
  disconnectPlace: (...args: unknown[]) => disconnectPlace(...(args as [])),
  setPlacePinned: (...args: unknown[]) => setPlacePinned(...(args as [])),
  listSavedServers: () => Promise.resolve([]),
}))
vi.mock('#lib/stores/volume-busy-store.svelte.js', () => ({
  isVolumeBusy: (id: string) => busy.has(id),
  isVolumeEjecting: () => false,
}))
const connectDirectly = vi.fn(() => Promise.resolve('connected'))
vi.mock('../network/direct-connect', () => ({
  connectDirectly: (...args: unknown[]) => connectDirectly(...(args as [])),
}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import {
  EMPTY_ROW_MENU,
  favoriteRowMenu,
  rowMenuItems,
  rowMenuSections,
  runRowFix,
  runVolumeRowAction,
  volumeRowMenu,
  type RowMenu,
  type RowMenuEntry,
} from './row-menu'
import type { VolumeInfo } from '../types'
import { addToast } from '#lib/ui/toast/index.js'
import { withPlatform } from '#lib/platform/test-platform.js'

const idle = { busy: false, ejecting: false, isSaved: false, directConnection: undefined, autoReconnect: undefined }

const usbDrive: VolumeInfo = {
  id: 'usb-stick',
  name: 'Stick',
  path: '/Volumes/Stick',
  category: 'attached_volume',
  isEjectable: true,
}

const phone: VolumeInfo = {
  ...usbDrive,
  id: 'adb-pixel',
  name: 'Pixel',
  category: 'mobile_device',
  deviceReadiness: { kind: 'ready' },
}

const share: VolumeInfo = {
  id: 'smb-testshare-media',
  name: 'media',
  path: '/Volumes/media',
  category: 'network',
  fsType: 'smbfs',
  isEjectable: true,
  connectionState: 'os_mount',
}

const place: VolumeInfo = {
  id: 'sftp-nas-local-22-test_user_1',
  name: 'Testnas',
  path: 'sftp://test_user_1@testnas.local:22/srv/data',
  category: 'network',
  fsType: 'sftp',
  isEjectable: false,
  connectionState: 'direct',
  pinned: false,
}

function nameOf(entry: RowMenuEntry): string {
  if (entry.type === 'action') return entry.action
  if (entry.type === 'fix') return `fix:${entry.fix}`
  return `toggle:${entry.toggle}`
}

/** Each non-empty named group as its entries' names, so an assertion reads like the menu. */
function shape(menu: RowMenu): Partial<Record<keyof RowMenu, string[]>> {
  const out: Partial<Record<keyof RowMenu, string[]>> = {}
  for (const group of ['actions', 'fixes', 'settings'] as const) {
    if (menu[group].length > 0) out[group] = menu[group].map(nameOf)
  }
  return out
}

function allEntries(menu: RowMenu): RowMenuEntry[] {
  return [...menu.actions, ...menu.fixes, ...menu.settings]
}

function entry(menu: RowMenu, name: string) {
  return allEntries(menu).find((e) => nameOf(e) === name)
}

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})
beforeEach(() => {
  vi.clearAllMocks()
  busy.clear()
})

describe('volumeRowMenu', () => {
  it('offers nothing on a fixed disk', () => {
    expect(volumeRowMenu({ ...usbDrive, id: 'root', category: 'main_volume', isEjectable: false }, idle)).toEqual(
      EMPTY_ROW_MENU,
    )
  })

  it('offers Eject on a removable drive, keeping the menu up so several go in a row', () => {
    const menu = volumeRowMenu(usbDrive, idle)
    expect(shape(menu)).toEqual({ actions: ['eject'] })
    expect(entry(menu, 'eject')).toMatchObject({ label: 'Eject (Stick)', icon: 'eject', keepsMenuOpen: true })
    expect(entry(menu, 'eject')).not.toMatchObject({ disabled: true })
  })

  it('offers Eject all under Eject on a drive with several volumes, and says what each does', () => {
    const menu = volumeRowMenu({ ...usbDrive, sharesDiskWithOthers: true }, idle)
    expect(shape(menu)).toEqual({ actions: ['eject', 'eject-all'] })
    expect(entry(menu, 'eject')).toMatchObject({
      tooltip: 'Ejects only this volume. Its drive stays connected until the other volumes are ejected too.',
    })
    expect(entry(menu, 'eject-all')).toMatchObject({
      label: 'Eject all',
      tooltip: 'Ejects every volume on this drive, so you can unplug it.',
    })
  })

  it('leaves the plain Eject without a tooltip on a drive with one volume', () => {
    expect(entry(volumeRowMenu(usbDrive, idle), 'eject')).not.toHaveProperty('tooltip', expect.any(String))
  })

  it('greys Eject all with Eject', () => {
    const menu = volumeRowMenu({ ...usbDrive, sharesDiskWithOthers: true }, { ...idle, busy: true })
    expect(entry(menu, 'eject-all')).toMatchObject({ label: 'Eject all (busy)', disabled: true })
  })

  it('greys Eject while a transfer touches the drive, and says why', () => {
    const menu = volumeRowMenu(usbDrive, { ...idle, busy: true })
    expect(entry(menu, 'eject')).toMatchObject({ label: 'Eject (Stick) (busy)', disabled: true })
  })

  it('greys Eject while the drive’s eject is still running', () => {
    expect(entry(volumeRowMenu(usbDrive, { ...idle, ejecting: true }), 'eject')).toMatchObject({ disabled: true })
  })

  it('words a phone’s eject as Disconnect, with the unplug glyph', () => {
    expect(entry(volumeRowMenu(phone, idle), 'eject')).toMatchObject({ label: 'Disconnect', icon: 'unplug' })
  })

  it('leaves the switch out until Rust has answered for the share', () => {
    expect(shape(volumeRowMenu(share, idle))).toEqual({ actions: ['eject'] })
  })

  it('offers a saved, connected server every server action, in order, and ❌ never Eject', () => {
    const menu = volumeRowMenu(place, { ...idle, isSaved: true })
    expect(shape(menu)).toEqual({ actions: ['open', 'edit', 'disconnect', 'pin', 'forget-secret', 'forget-server'] })
  })

  it('offers Unpin on a pinned server', () => {
    expect(shape(volumeRowMenu({ ...place, pinned: true }, idle))).toEqual({
      actions: ['open', 'disconnect', 'unpin', 'forget-secret'],
    })
  })

  it('offers no Disconnect on a saved row: there is no session to end', () => {
    const menu = volumeRowMenu({ ...place, connectionState: 'saved', pinned: true }, { ...idle, isSaved: true })
    expect(entry(menu, 'disconnect')).toBeUndefined()
  })

  it('offers no Edit or Forget server on a server nothing saved', () => {
    const menu = volumeRowMenu(place, idle)
    expect(entry(menu, 'edit')).toBeUndefined()
    expect(entry(menu, 'forget-server')).toBeUndefined()
  })

  it('puts a saved place’s “Reconnect automatically” in the settings group, explained, so nobody reads it as “connect at startup”', () => {
    const menu = volumeRowMenu(place, { ...idle, isSaved: true, autoReconnect: false })
    expect(shape(menu)).toEqual({
      actions: ['open', 'edit', 'disconnect', 'pin', 'forget-secret', 'forget-server'],
      settings: ['toggle:auto-reconnect'],
    })
    expect(entry(menu, 'toggle:auto-reconnect')).toMatchObject({
      type: 'toggle',
      label: 'Reconnect automatically',
      checked: false,
      tooltip: expect.stringContaining('doesn’t make Cmdr connect at startup') as unknown,
    })
  })

  it('leaves “Reconnect automatically” out where nothing saved backs the row: there is nothing to persist', () => {
    expect(entry(volumeRowMenu(place, idle), 'toggle:auto-reconnect')).toBeUndefined()
  })

  it('greys the destructive server actions under a running transfer, never Open, Edit, or the pin', () => {
    const menu = volumeRowMenu(place, { ...idle, busy: true, isSaved: true })
    const greyed = allEntries(menu)
      .filter((e) => e.type === 'action' && e.disabled)
      .map((e) => (e.type === 'action' ? e.action : ''))
    expect(greyed).toEqual(['disconnect', 'forget-secret', 'forget-server'])
    expect(entry(menu, 'disconnect')?.label).toBe('Disconnect (busy)')
  })
})

describe('volumeRowMenu: an SMB share’s direct connection', () => {
  const direct: VolumeInfo = { ...share, connectionState: 'direct' }

  it('switch ON but still on the macOS mount: offers “Connect directly now” as a fix, between Eject and the switch', () => {
    const menu = volumeRowMenu(share, { ...idle, directConnection: true })
    expect(shape(menu)).toEqual({
      actions: ['eject'],
      fixes: ['fix:connect-directly'],
      settings: ['toggle:direct-connection'],
    })
    expect(entry(menu, 'fix:connect-directly')).toMatchObject({
      type: 'fix',
      label: 'Connect directly now',
      tooltip: expect.stringContaining('faster access') as unknown,
    })
    expect(entry(menu, 'toggle:direct-connection')).toMatchObject({ type: 'toggle', checked: true })
  })

  it('switch ON and already direct: only the switch, nothing to fix', () => {
    expect(shape(volumeRowMenu(direct, { ...idle, directConnection: true }))).toEqual({
      actions: ['eject'],
      settings: ['toggle:direct-connection'],
    })
  })

  it('switch OFF: only the switch, since checking it already connects', () => {
    expect(shape(volumeRowMenu(share, { ...idle, directConnection: false }))).toEqual({
      actions: ['eject'],
      settings: ['toggle:direct-connection'],
    })
  })

  it('offers no fix before Rust has answered for the switch', () => {
    expect(entry(volumeRowMenu(share, idle), 'fix:connect-directly')).toBeUndefined()
  })
})

/** A mounted share's detach item and refusal toast say its OS's verb; a drive's say Eject everywhere (#1117). */
describe.each([
  ['macos', 'Eject (media)', 'Eject (media) (busy)', 'Couldn’t eject media: '],
  ['windows', 'Disconnect (media)', 'Disconnect (media) (busy)', 'Couldn’t disconnect media: '],
  ['linux', 'Unmount (media)', 'Unmount (media) (busy)', 'Couldn’t unmount media: '],
] as const)('volumeRowMenu: a mounted share on %s', (platform, label, busyLabel, toastStart) => {
  beforeEach(() => withPlatform(platform))

  it('words the detach item with this OS’s verb, idle and busy', () => {
    expect(entry(volumeRowMenu(share, idle), 'eject')).toMatchObject({ label, icon: 'eject' })
    busy.add(share.id)
    expect(entry(volumeRowMenu(share, { ...idle, busy: true }), 'eject')).toMatchObject({
      label: busyLabel,
      disabled: true,
    })
  })

  it('keeps Eject for a drive', () => {
    expect(entry(volumeRowMenu(usbDrive, idle), 'eject')).toMatchObject({ label: 'Eject (Stick)' })
  })

  it('says the same verb when the OS turns the unmount down', async () => {
    ejectVolume.mockRejectedValueOnce({ type: 'unknown' })
    await runVolumeRowAction({ volume: share, action: 'eject' })
    const [text] = vi.mocked(addToast).mock.calls[0] as [string]
    expect(text.startsWith(toastStart)).toBe(true)
  })
})

/**
 * A SAVED share (`docs/specs/saved-smb-shares.md`) is pinnable like an SFTP
 * place: Unpin sits in its switcher row's menu, and a share nobody saved (a
 * Finder mount) offers no pin at all.
 */
describe('volumeRowMenu: a saved SMB share’s pin', () => {
  it('offers Pin after Eject on a mounted saved share', () => {
    expect(shape(volumeRowMenu(share, { ...idle, isSaved: true }))).toEqual({ actions: ['eject', 'pin'] })
  })

  it('offers Unpin on a pinned one, greyed out of the switcher or not', () => {
    const saved: VolumeInfo = { ...share, connectionState: 'saved', isEjectable: false, pinned: true }
    expect(shape(volumeRowMenu(saved, { ...idle, isSaved: true }))).toEqual({ actions: ['unpin'] })
  })

  it('offers no pin on a share nothing saved', () => {
    expect(shape(volumeRowMenu(share, idle))).toEqual({ actions: ['eject'] })
  })
})

describe('favoriteRowMenu', () => {
  it('offers Rename, Set shortcut key, and Remove, all keeping the menu up', () => {
    const menu = favoriteRowMenu()
    expect(shape(menu)).toEqual({
      actions: ['rename-favorite', 'edit-favorite-shortcut', 'remove-favorite'],
    })
    expect(allEntries(menu).every((e) => e.type === 'action' && e.keepsMenuOpen)).toBe(true)
  })
})

describe('rowMenuItems', () => {
  it('turns groups into submenu rows: unique values, a rule before the settings, and the entry carried back', () => {
    const menu = volumeRowMenu(share, { ...idle, directConnection: false })
    const items = rowMenuItems(share.id, menu, (e) => e)
    expect(items?.map((item) => item.value)).toEqual([
      'row:smb-testshare-media:eject',
      'row:smb-testshare-media:toggle:direct-connection',
    ])
    expect(items?.[0]).toMatchObject({ icon: { lucide: 'eject' }, keepsMenuOpen: true, separatorBefore: false })
    expect(items?.[1]).toMatchObject({ check: { kind: 'toggle', checked: false }, separatorBefore: true })
    expect(items?.[1].data).toBe(menu.settings[0])
  })

  it('runs a fix straight on from the actions, with no rule between, and its explanation as the tooltip', () => {
    const menu = volumeRowMenu(share, { ...idle, directConnection: true })
    const items = rowMenuItems(share.id, menu, (e) => e)
    expect(items?.map((item) => [item.value, item.separatorBefore])).toEqual([
      ['row:smb-testshare-media:eject', false],
      ['row:smb-testshare-media:fix:connect-directly', false],
      ['row:smb-testshare-media:toggle:direct-connection', true],
    ])
    expect(items?.[1].tooltip).toContain('faster access')
    expect(items?.[1].check).toBeUndefined()
    expect(items?.[1].keepsMenuOpen).toBeFalsy()
  })

  it('draws no rule above the settings when nothing sits above them', () => {
    const { settings } = volumeRowMenu(share, { ...idle, directConnection: false })
    const items = rowMenuItems('x', { ...EMPTY_ROW_MENU, settings }, (e) => e)
    expect(items?.[0].separatorBefore).toBe(false)
  })

  it('carries a switch’s explanation onto its row as the tooltip', () => {
    const menu = volumeRowMenu(place, { ...idle, isSaved: true, autoReconnect: true })
    const row = rowMenuItems(place.id, menu, (e) => e)?.find((item) => item.value.endsWith('toggle:auto-reconnect'))
    expect(row).toMatchObject({ check: { kind: 'toggle', checked: true }, separatorBefore: true })
    expect(row?.tooltip).toContain('reconnects to this server on its own')
  })

  it('gives a row with no actions no submenu at all, so it draws no arrow', () => {
    expect(rowMenuItems('root', EMPTY_ROW_MENU, (e) => e)).toBeUndefined()
  })
})

describe('rowMenuSections', () => {
  it('puts the actions and fixes in one section and the settings in the next, so the primitive draws the rule', () => {
    const menu = volumeRowMenu(share, { ...idle, directConnection: true })
    const sections = rowMenuSections(share.id, menu, (e) => e)
    expect(sections.map((section) => section.items.map((item) => item.value))).toEqual([
      ['row:smb-testshare-media:eject', 'row:smb-testshare-media:fix:connect-directly'],
      ['row:smb-testshare-media:toggle:direct-connection'],
    ])
    // A top-level list splits into sections, never rules inside one.
    expect(sections.flatMap((section) => section.items).some((item) => item.separatorBefore)).toBe(false)
  })
})

describe('runRowFix', () => {
  it('“Connect directly now” runs the one Connect directly flow, named for the share', async () => {
    await runRowFix({ volume: share, fix: 'connect-directly' })
    expect(connectDirectly).toHaveBeenCalledWith({ volumeId: 'smb-testshare-media', shareName: 'media' })
  })
})

describe('runVolumeRowAction', () => {
  it('ejects a drive through the same guarded path as the row’s eject button', async () => {
    await runVolumeRowAction({ volume: usbDrive, action: 'eject' })
    expect(ejectVolume).toHaveBeenCalledWith('usb-stick')
  })

  it('asks for the whole disk on Eject all, and refuses under a running transfer', async () => {
    await runVolumeRowAction({ volume: usbDrive, action: 'eject-all' })
    expect(ejectAllOnDisk).toHaveBeenCalledWith('usb-stick')
    ejectAllOnDisk.mockClear()
    busy.add(usbDrive.id)
    await runVolumeRowAction({ volume: usbDrive, action: 'eject-all' })
    expect(ejectAllOnDisk).not.toHaveBeenCalled()
    busy.delete(usbDrive.id)
  })

  it('refuses to disconnect a server under a running transfer, whatever the menu showed', async () => {
    busy.add(place.id)
    await runVolumeRowAction({ volume: place, action: 'disconnect' })
    expect(disconnectPlace).not.toHaveBeenCalled()
  })

  it('hands every other server action to the servers family', async () => {
    await runVolumeRowAction({ volume: place, action: 'pin' })
    expect(setPlacePinned).toHaveBeenCalledWith('sftp-nas-local-22-test_user_1', true)
  })

  it('opens through the surface’s own navigation', async () => {
    const onOpen = vi.fn()
    await runVolumeRowAction({ volume: place, action: 'open', onOpen })
    expect(onOpen).toHaveBeenCalledWith('sftp-nas-local-22-test_user_1')
  })
})
