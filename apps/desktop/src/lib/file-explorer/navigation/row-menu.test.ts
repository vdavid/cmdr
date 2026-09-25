/**
 * A row's action list: which entries a volume, server, or favorite row offers, in what
 * order, and which ones a running transfer greys. The ONE list every door shows (the row's
 * → submenu, its right-click, the servers hub's right-click), so it's pinned here rather
 * than per surface.
 */
import { describe, it, expect, vi, beforeAll, afterAll, beforeEach } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'

const ejectVolume = vi.fn(() => Promise.resolve())
const disconnectPlace = vi.fn(() => Promise.resolve(true))
const setPlacePinned = vi.fn(() => Promise.resolve(true))
const busy = new Set<string>()

vi.mock('$lib/tauri-commands', () => ({
  ejectVolume: (...args: unknown[]) => ejectVolume(...(args as [])),
  disconnectPlace: (...args: unknown[]) => disconnectPlace(...(args as [])),
  setPlacePinned: (...args: unknown[]) => setPlacePinned(...(args as [])),
  listSavedServers: () => Promise.resolve([]),
}))
vi.mock('$lib/stores/volume-busy-store.svelte', () => ({
  isVolumeBusy: (id: string) => busy.has(id),
  isVolumeEjecting: () => false,
}))
const connectDirectly = vi.fn(() => Promise.resolve('connected'))
vi.mock('../network/direct-connect', () => ({
  connectDirectly: (...args: unknown[]) => connectDirectly(...(args as [])),
}))
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
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
  id: 'smb-naspi-media',
  name: 'media',
  path: '/Volumes/media',
  category: 'network',
  fsType: 'smbfs',
  isEjectable: true,
  connectionState: 'os_mount',
}

const place: VolumeInfo = {
  id: 'sftp-nas-local-22-ada',
  name: 'Naspolya',
  path: 'sftp://ada@nas.local:22/srv/data',
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

describe('favoriteRowMenu', () => {
  it('offers Rename, Set shortcut, and Remove, all keeping the menu up', () => {
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
      'row:smb-naspi-media:eject',
      'row:smb-naspi-media:toggle:direct-connection',
    ])
    expect(items?.[0]).toMatchObject({ icon: { lucide: 'eject' }, keepsMenuOpen: true, separatorBefore: false })
    expect(items?.[1]).toMatchObject({ check: { kind: 'toggle', checked: false }, separatorBefore: true })
    expect(items?.[1].data).toBe(menu.settings[0])
  })

  it('runs a fix straight on from the actions, with no rule between, and its explanation as the tooltip', () => {
    const menu = volumeRowMenu(share, { ...idle, directConnection: true })
    const items = rowMenuItems(share.id, menu, (e) => e)
    expect(items?.map((item) => [item.value, item.separatorBefore])).toEqual([
      ['row:smb-naspi-media:eject', false],
      ['row:smb-naspi-media:fix:connect-directly', false],
      ['row:smb-naspi-media:toggle:direct-connection', true],
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
      ['row:smb-naspi-media:eject', 'row:smb-naspi-media:fix:connect-directly'],
      ['row:smb-naspi-media:toggle:direct-connection'],
    ])
    // A top-level list splits into sections, never rules inside one.
    expect(sections.flatMap((section) => section.items).some((item) => item.separatorBefore)).toBe(false)
  })
})

describe('runRowFix', () => {
  it('“Connect directly now” runs the one Connect directly flow, named for the share', async () => {
    await runRowFix({ volume: share, fix: 'connect-directly' })
    expect(connectDirectly).toHaveBeenCalledWith({ volumeId: 'smb-naspi-media', shareName: 'media' })
  })
})

describe('runVolumeRowAction', () => {
  it('ejects a drive through the same guarded path as the row’s eject button', async () => {
    await runVolumeRowAction({ volume: usbDrive, action: 'eject' })
    expect(ejectVolume).toHaveBeenCalledWith('usb-stick')
  })

  it('refuses to disconnect a server under a running transfer, whatever the menu showed', async () => {
    busy.add(place.id)
    await runVolumeRowAction({ volume: place, action: 'disconnect' })
    expect(disconnectPlace).not.toHaveBeenCalled()
  })

  it('hands every other server action to the servers family', async () => {
    await runVolumeRowAction({ volume: place, action: 'pin' })
    expect(setPlacePinned).toHaveBeenCalledWith('sftp-nas-local-22-ada', true)
  })

  it('opens through the surface’s own navigation', async () => {
    const onOpen = vi.fn()
    await runVolumeRowAction({ volume: place, action: 'open', onOpen })
    expect(onOpen).toHaveBeenCalledWith('sftp-nas-local-22-ada')
  })
})
