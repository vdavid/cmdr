/**
 * Behavior tests for `ServersHub`: the double-handler hazard that ⌘R and Enter
 * share, and what Enter and F8 do to each kind of row.
 *
 * Both keys have two handlers on their path: the pane's own element-level one,
 * which reaches `ServersHub.handleKeyDown`, and the document-level dispatcher in
 * `+page.svelte`, registered bubble-phase with no `defaultPrevented` guard. It
 * routes `pane.refresh` back into this component through `refreshNetworkHosts()` →
 * `ServersHub.refresh()`, and `nav.open` (which bare Enter resolves to) back in
 * through `sendKeyToFocusedPane('Enter')` → the pane router's network arm.
 *
 * The local handler therefore has to stop propagation, or one keypress re-reads
 * every host's shares twice, or opens the row under the cursor twice. These tests
 * wire both handlers the way the app does and count the actual work, not the
 * handler's return value.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import ServersHub from './ServersHub.svelte'
import { resolveGlobalKeyAction } from '../../../routes/(main)/global-keydown'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { initShortcutDispatch, destroyShortcutDispatch } from '$lib/shortcuts/shortcut-dispatch'
import type { HubRow } from './servers-hub-rows'
import type { NetworkHost } from '../types'
import type { SavedServer } from '$lib/tauri-commands'

const h = vi.hoisted(() => ({
  clearShareState: vi.fn(),
  fetchShares: vi.fn(() => Promise.resolve()),
  refreshAllStaleShares: vi.fn(),
  listSavedServers: vi.fn(),
  forgetSavedServer: vi.fn(() => Promise.resolve()),
  addToast: vi.fn(() => 'id'),
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
  /** Every command the document dispatcher resolved a key to, in order. */
  dispatched: [] as string[],
  setSetting: vi.fn(),
  /** The stored settings, reactive like the app's, so a toggle re-renders the hub. */
  settings: new Map<string, string>(),
}))

/** The saved SMB server behind a discovered host, which is what makes it the person's own. */
function savedSmb(host: NetworkHost): SavedServer {
  return {
    id: `manual-${host.id}`,
    protocol: 'smb',
    displayName: host.name,
    nameSource: 'fallback',
    address: host.hostname ?? host.name,
    username: null,
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [],
  }
}

const mockHosts: NetworkHost[] = [
  { id: 'h1', name: 'Naspolya', hostname: 'Naspolya.local', ipAddress: '192.168.1.111', port: 445 },
  { id: 'h2', name: 'Attic', hostname: 'attic.local', ipAddress: '192.168.1.112', port: 445 },
]

const savedSftp: SavedServer = {
  id: 'sftp-jump.local-22-ada',
  protocol: 'sftp',
  displayName: 'Jump box',
  nameSource: 'user',
  address: 'jump.local:22',
  username: 'ada',
  pinned: true,
  lastConnectedAt: '2026-09-01T10:00:00Z',
  autoReconnect: true,
  places: [
    {
      volumeId: 'sftp-jump.local-22-ada',
      name: 'Jump box',
      pinned: true,
      connected: false,
      appRoot: 'sftp://ada@jump.local:22',
      username: 'ada',
      autoReconnect: true,
    },
  ],
}

vi.mock('./network-store.svelte', () => ({
  getNetworkHosts: () => mockHosts,
  getDiscoveryState: () => 'idle',
  getShareState: () => undefined,
  isHostResolving: () => false,
  isListingShares: () => false,
  isShareDataStale: () => false,
  getShareCount: () => undefined,
  refreshAllStaleShares: h.refreshAllStaleShares,
  clearShareState: h.clearShareState,
  fetchShares: h.fetchShares,
  getCredentialStatus: () => 'unknown',
  checkCredentialsForHost: vi.fn(() => Promise.resolve()),
  forgetCredentials: vi.fn(() => Promise.resolve()),
  setCredentialStatus: vi.fn(),
  getListedAccount: () => undefined,
}))

vi.mock('./lazy-trigger', () => ({ triggerNetworkDiscovery: vi.fn() }))
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))
vi.mock('$lib/settings/reactive-settings.svelte', async () => {
  const { SvelteMap } = await import('svelte/reactivity')
  h.settings = new SvelteMap<string, string>()
  return {
    getNetworkEnabled: () => true,
    formattedDate: () => ({ text: '', segments: [] }),
    getNearbyServersGroupChoice: () => h.settings.get('network.nearbyServersGroup') ?? 'auto',
  }
})
vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/settings')>()),
  setSetting: (id: string, value: string) => {
    h.setSetting(id, value)
    h.settings.set(id, value)
  },
}))
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  settingAnchorId: (id: string) => `setting-${id}`,
}))
vi.mock('../navigation/server-row-actions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../navigation/server-row-actions')>()),
  forgetSavedServer: h.forgetSavedServer,
}))
vi.mock('$lib/stores/volume-busy-store.svelte', () => ({ isVolumeBusy: () => false, isVolumeEjecting: () => false }))

vi.mock('$lib/tauri-commands', () => ({
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
  showNetworkHostContextMenu: h.showNetworkHostContextMenu,
  onNetworkHostContextAction: vi.fn(() => Promise.resolve(() => {})),
  disconnectNetworkHost: vi.fn(() => Promise.resolve()),
  listSavedServers: h.listSavedServers,
}))

vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: vi.fn(() => Promise.resolve(false)) }))
vi.mock('$lib/ui/toast', () => ({ addToast: h.addToast }))

/**
 * A `pane.refresh` keypress for the platform the test runs on: the default binding
 * is ⌘R on macOS and Ctrl+R elsewhere, and the test env reports non-macOS.
 */
function refreshKeyEvent(): KeyboardEvent {
  const modifier = isMacOS() ? { metaKey: true } : { ctrlKey: true }
  return new KeyboardEvent('keydown', { key: 'r', bubbles: true, ...modifier })
}

/** The exported `ServersHub` API surface these tests drive. */
interface ServersHubApi {
  handleKeyDown: (e: KeyboardEvent) => void
  refresh: () => void
  setCursorIndex: (index: number) => void
  getItemCount: () => number
  findItemIndex: (name: string) => number
  openCursorItem: () => void
  getRowUnderCursor: () => HubRow | null
  selectServer: (id: string) => void
  openContextMenuAtCursor: () => Promise<void>
}

interface MountedHub {
  target: HTMLElement
  api: ServersHubApi
  onHostSelect: ReturnType<typeof vi.fn>
  onServerSelect: ReturnType<typeof vi.fn>
  onConnectToServer: ReturnType<typeof vi.fn>
  cleanup: () => Promise<void>
}

/**
 * Mounts the hub behind the two handlers ⌘R and Enter really pass through: the
 * pane's element-level one (a descendant of `document`, so it runs first) and the
 * document-level dispatcher, which turns a `pane.refresh` dispatch back into
 * `refresh()` the way `refreshPane` in `pane-commands.ts` does, and a `nav.open`
 * dispatch back into this handler the way `sendKeyToFocusedPane('Enter')` does
 * (`nav-handlers.ts` → `pane-key-router` → `handleNetworkKeyDown`).
 */
function mountBehindBothHandlers(): MountedHub {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onHostSelect = vi.fn()
  const onServerSelect = vi.fn()
  const onConnectToServer = vi.fn()
  const component = mount(ServersHub, {
    target,
    props: { paneId: 'left', isFocused: true, onHostSelect, onServerSelect, onConnectToServer },
  })
  const api = component as unknown as ServersHubApi

  const paneHandler = (e: KeyboardEvent) => {
    api.handleKeyDown(e)
  }
  const documentDispatcher = (e: KeyboardEvent) => {
    const action = resolveGlobalKeyAction(e, { dialogOpen: false, paletteOpen: false })
    if (action.kind !== 'dispatch') return
    h.dispatched.push(action.commandId)
    if (action.commandId === 'pane.refresh') api.refresh()
    // `nav.open`'s handler re-sends Enter to the focused pane, which hands the
    // network view every key: the hub's own handler runs a second time.
    if (action.commandId === 'nav.open') api.handleKeyDown(new KeyboardEvent('keydown', { key: 'Enter' }))
    // `file.contextMenu` (⌃⏎) reaches the hub through FilePane → NetworkMountView.
    if (action.commandId === 'file.contextMenu') void api.openContextMenuAtCursor()
  }
  target.addEventListener('keydown', paneHandler)
  document.addEventListener('keydown', documentDispatcher)

  const cleanup = async () => {
    target.removeEventListener('keydown', paneHandler)
    document.removeEventListener('keydown', documentDispatcher)
    await unmount(component)
  }
  return { target, api, onHostSelect, onServerSelect, onConnectToServer, cleanup }
}

/**
 * Lets the hub's first read of the saved list land. It lists nothing before that:
 * which servers are saved decides where each row goes and whether the nearby
 * group starts open.
 */
async function settle(): Promise<void> {
  for (let round = 0; round < 3; round++) await tick()
}

/** An unmodified keypress the hub's own handler claims. */
function plainKey(key: string): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, bubbles: true })
}

beforeEach(() => {
  vi.clearAllMocks()
  h.dispatched.length = 0
  h.listSavedServers.mockResolvedValue([])
  h.settings.clear()
  document.body.innerHTML = ''
  // The document dispatcher's reverse lookup is built here in the app's startup.
  initShortcutDispatch()
})

afterEach(() => {
  destroyShortcutDispatch()
})

describe('ServersHub refresh key', () => {
  it('re-reads each host once per ⌘R, not once per handler on the path', async () => {
    h.listSavedServers.mockResolvedValue(mockHosts.map(savedSmb))
    const { target, cleanup } = mountBehindBothHandlers()
    await settle()
    h.clearShareState.mockClear()
    h.fetchShares.mockClear()

    const listContainer = target.querySelector('.row-list')
    expect(listContainer).not.toBeNull()
    listContainer?.dispatchEvent(refreshKeyEvent())

    // One round per host. Two rounds means the document dispatcher ran the same
    // refresh again after the local handler already did.
    expect(h.clearShareState).toHaveBeenCalledTimes(mockHosts.length)
    expect(h.fetchShares).toHaveBeenCalledTimes(mockHosts.length)

    await cleanup()
  })

  it('still refreshes when only the document dispatcher sees the key', async () => {
    h.listSavedServers.mockResolvedValue(mockHosts.map(savedSmb))
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    h.clearShareState.mockClear()
    h.fetchShares.mockClear()

    // The pane can be unfocused (no local handler on the path) while the window
    // shortcut still fires; `refresh()` stays the entry point for that.
    api.refresh()

    expect(h.clearShareState).toHaveBeenCalledTimes(mockHosts.length)
    expect(h.fetchShares).toHaveBeenCalledTimes(mockHosts.length)

    await cleanup()
  })

  /**
   * #324: a refresh re-reads the list, and listing a host's shares means signing in
   * to it (as a guest, where it lets one in). For a host the person never saved,
   * that waits until they open it.
   */
  it('lists no shares on a host the person never saved', async () => {
    h.listSavedServers.mockResolvedValue([savedSmb(mockHosts[0])])
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    h.clearShareState.mockClear()
    h.fetchShares.mockClear()

    api.refresh()

    expect(h.fetchShares).toHaveBeenCalledOnce()
    expect(h.fetchShares).toHaveBeenCalledWith(mockHosts[0])
    // Its stale list still goes, so opening it lists afresh.
    expect(h.clearShareState).toHaveBeenCalledTimes(mockHosts.length)

    await cleanup()
  })
})

describe('ServersHub rows', () => {
  it('always offers one more row than it lists, which is "Add server…"', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    // The two nearby hosts, their group's header, and the add row.
    expect(api.getItemCount()).toBe(mockHosts.length + 2)
    await cleanup()
  })

  it('opens the row once per Enter, not once per handler on the path', async () => {
    const { target, api, onHostSelect, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.findItemIndex('Naspolya'))

    const listContainer = target.querySelector('.row-list')
    expect(listContainer).not.toBeNull()
    listContainer?.dispatchEvent(plainKey('Enter'))

    // Twice means the hub acted, let the key bubble, and the document dispatcher's
    // `nav.open` posted Enter straight back into it.
    expect(onHostSelect).toHaveBeenCalledOnce()

    await cleanup()
  })

  it('opens an SMB host into its places list', async () => {
    const { api, onHostSelect, onServerSelect, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.findItemIndex('Naspolya'))
    api.openCursorItem()
    expect(onHostSelect).toHaveBeenCalledOnce()
    expect(onServerSelect).not.toHaveBeenCalled()
    await cleanup()
  })

  it('takes a one-place server to its place instead, which is where the pane dials', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { api, onHostSelect, onServerSelect, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    api.setCursorIndex(api.findItemIndex('Jump box'))
    api.openCursorItem()
    expect(onServerSelect).toHaveBeenCalledOnce()
    expect(onHostSelect).not.toHaveBeenCalled()
    await cleanup()
  })

  it('opens the add form from the last row', async () => {
    const { api, onConnectToServer, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.getItemCount() - 1)
    api.openCursorItem()
    expect(onConnectToServer).toHaveBeenCalledOnce()
    await cleanup()
  })

  it('has no row under the cursor on "Add server…", so a command acts on nothing', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.getItemCount() - 1)
    expect(api.getRowUnderCursor()).toBeNull()
    await cleanup()
  })
})

/**
 * cmdr-reports#6: after "Add", the new server shows up SELECTED, so it is
 * visibly saved, even though the saved list only learns of it a moment later.
 */
describe('ServersHub selectServer', () => {
  it('puts the cursor on a server that joins the list after the call', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    h.listSavedServers.mockResolvedValue([savedSftp])

    api.selectServer('sftp-jump.local-22-ada')
    await vi.waitFor(() => {
      expect(api.getRowUnderCursor()?.id).toBe('sftp-jump.local-22-ada')
    })
    await cleanup()
  })

  /**
   * ❗ The cursor stays on the ROW it was put on when the list reorders under it.
   * After a plain Add the new row was selected, then the list re-sorted as more
   * came in and the cursor sat on a neighbour (QA round 2, m5).
   */
  it('keeps the cursor on the selected row when the list reorders around it', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.selectServer('h2')
    await vi.waitFor(() => {
      expect(api.getRowUnderCursor()?.name).toBe('Attic')
    })

    // A saved server arrives and sorts above the nearby hosts.
    h.listSavedServers.mockResolvedValue([savedSftp])
    api.refresh()
    await vi.waitFor(() => {
      expect(api.getItemCount()).toBe(mockHosts.length + 3)
    })
    await settle()

    expect(api.getRowUnderCursor()?.name).toBe('Attic')
    await cleanup()
  })

  it('finds an added SMB host by its discovery id too', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.selectServer('h2')
    await vi.waitFor(() => {
      expect(api.getRowUnderCursor()?.name).toBe('Attic')
    })
    await cleanup()
  })
})

describe('ServersHub F8', () => {
  it('forgets a saved server through the servers family, so the hub asks what the switcher asks', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    api.setCursorIndex(api.findItemIndex('Jump box'))
    api.handleKeyDown(plainKey('F8'))
    await settle()
    expect(h.forgetSavedServer).toHaveBeenCalledWith('sftp-jump.local-22-ada', 'Jump box')
    await cleanup()
  })

  /**
   * ❗ F8 is `file.delete` everywhere else, and the document dispatcher runs after
   * the hub's own handler: a claimed-but-bubbling F8 asked twice (QA round 2, m3).
   */
  it('claims F8, so the document dispatcher never runs file.delete behind it', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    api.setCursorIndex(api.findItemIndex('Jump box'))
    target.querySelector('.row-list')?.dispatchEvent(plainKey('F8'))
    await settle()
    expect(h.forgetSavedServer).toHaveBeenCalledOnce()
    expect(h.dispatched).toEqual([])
    await cleanup()
  })

  it('says a discovered host is not the user’s to remove, rather than doing nothing', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.findItemIndex('Naspolya'))
    api.handleKeyDown(plainKey('F8'))
    await settle()
    expect(h.forgetSavedServer).not.toHaveBeenCalled()
    expect(h.addToast).toHaveBeenCalledOnce()
    await cleanup()
  })
})

/**
 * A one-place row's right-click: the house `Menu` at the pointer, holding the same list
 * the volume switcher row's → submenu shows.
 */
describe('ServersHub row menu', () => {
  async function rightClick(name: string): Promise<void> {
    const row = [...document.querySelectorAll<HTMLElement>('.server-row')].find((el) => el.textContent.includes(name))
    row?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 60 }))
    await settle()
    await settle()
  }

  it('opens the server list at the pointer, read off the saved entry', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    await rightClick('Jump box')
    const labels = [...document.querySelectorAll('[data-menu] [data-menu-row]')].map((el) => el.textContent.trim())
    // Saved, not connected: nothing to disconnect. Saved, so its "Reconnect automatically" rides below.
    expect(labels).toEqual([
      'Open',
      'Edit server…',
      expect.stringMatching(/pin/i),
      'Forget saved password',
      'Forget server',
      'Reconnect automatically',
    ])
    await cleanup()
  })

  it('opens the place in THIS pane from the menu’s Open, the way Enter does', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { onServerSelect, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    await rightClick('Jump box')
    document
      .querySelector('[data-menu] [data-menu-row="row:sftp-jump.local-22-ada:open"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await settle()
    expect(onServerSelect).toHaveBeenCalledOnce()
    expect(document.querySelector('[data-menu]')).toBeNull()
    await cleanup()
  })
})

/**
 * ⌃⏎ opens the cursor row's menu, the way it does on a file row, so every row
 * action is reachable without a mouse (QA 2026-09-25: it did nothing here).
 */
describe('ServersHub keyboard context menu', () => {
  /** ⌃⏎, the `file.contextMenu` binding on every platform. */
  function ctrlEnter(): KeyboardEvent {
    return new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true })
  }

  it('opens a one-place row’s menu on ⌃⏎, once', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, api, onServerSelect, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    api.setCursorIndex(api.findItemIndex('Jump box'))

    target.querySelector('.row-list')?.dispatchEvent(ctrlEnter())
    await settle()
    await settle()

    const labels = [...document.querySelectorAll('[data-menu] [data-menu-row]')].map((el) => el.textContent.trim())
    expect(labels[0]).toBe('Open')
    // ⌃⏎ is not Enter: nothing opened.
    expect(onServerSelect).not.toHaveBeenCalled()
    await cleanup()
  })

  it('raises an SMB host’s menu for the row under the cursor, placed at the row', async () => {
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.findItemIndex('Attic'))

    target.querySelector('.row-list')?.dispatchEvent(ctrlEnter())
    await vi.waitFor(() => {
      expect(h.showNetworkHostContextMenu).toHaveBeenCalledOnce()
    })

    const args = h.showNetworkHostContextMenu.mock.calls[0] as unknown[]
    expect(args[0]).toBe('h2')
    // An anchor, not `null`: a keypress has no pointer for macOS to use.
    const anchor = args[5] as { x: unknown; y: unknown } | null
    expect(typeof anchor?.x).toBe('number')
    expect(typeof anchor?.y).toBe('number')
    await cleanup()
  })

  it('opens a saved share’s menu on ⌃⏎, the same one its right-click opens', async () => {
    h.listSavedServers.mockResolvedValue([
      {
        id: 'manual-10-0-0-9-445',
        protocol: 'smb',
        displayName: 'Box',
        nameSource: 'user',
        address: '10.0.0.9',
        username: null,
        pinned: false,
        lastConnectedAt: null,
        autoReconnect: null,
        places: [
          {
            volumeId: 'smb-box-public',
            name: 'public',
            pinned: true,
            connected: false,
            appRoot: '/Volumes/public',
            username: null,
            autoReconnect: null,
          },
        ],
      } satisfies SavedServer,
    ])
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    api.setCursorIndex(api.findItemIndex('public'))

    target.querySelector('.row-list')?.dispatchEvent(ctrlEnter())
    await settle()
    await settle()

    const labels = [...document.querySelectorAll('[data-menu] [data-menu-row]')].map((el) => el.textContent.trim())
    expect(labels).toEqual(['Open', expect.stringMatching(/pin/i), 'Forget share'])
    await cleanup()
  })

  it('opens nothing on the "Add server…" row', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.getItemCount() - 1)
    await api.openContextMenuAtCursor()
    expect(h.showNetworkHostContextMenu).not.toHaveBeenCalled()
    expect(document.querySelector('[data-menu]')).toBeNull()
    await cleanup()
  })
})

describe('ServersHub row text', () => {
  const withShare: SavedServer = {
    id: 'manual-10-0-0-9-445',
    protocol: 'smb',
    displayName: 'Box',
    nameSource: 'user',
    address: '10.0.0.9',
    username: null,
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [
      {
        volumeId: 'smb-box-public',
        name: 'public',
        pinned: true,
        connected: false,
        appRoot: '/Volumes/public',
        username: 'testuser',
        autoReconnect: null,
      },
    ],
  }

  /** ❗ Shares are rows, not servers: "7 servers" for 5 hosts and 2 shares (QA round 2, m4). */
  it('counts servers in the status bar, not the share rows under them', async () => {
    h.listSavedServers.mockResolvedValue([withShare])
    const { target, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    // Two discovered hosts plus the saved one; neither its share row nor the nearby group's header counts.
    expect(target.querySelector('.status-text')?.textContent.trim()).toBe('3 servers')
    await cleanup()
  })

  /**
   * ❗ The name and its "as testuser" sit in ONE text span that clips with an
   * ellipsis. As bare text in the flex cell they ran into the Type column with
   * no ellipsis (QA round 2, m1).
   */
  it('puts a row`s name and account in one clipping text span', async () => {
    h.listSavedServers.mockResolvedValue([withShare])
    const { target, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    const share = [...target.querySelectorAll('.server-row')].find((row) => row.textContent.includes('public'))
    // ❗ With a space: "privateas testuser" ran the two together (QA round 3).
    expect(share?.querySelector('.col-name .name-text')?.textContent).toBe('public as testuser')
    await cleanup()
  })
})

/**
 * ❗ A Pin or Unpin leaves the row menu up (like the switcher's), so the menu has to
 * read the row as it is NOW: it held the row it opened on and kept offering the
 * pin it had just flipped (QA round 2, m2).
 */
describe('ServersHub row menu after a pin', () => {
  const share = (pinned: boolean): SavedServer => ({
    id: 'manual-10-0-0-9-445',
    protocol: 'smb',
    displayName: 'Box',
    nameSource: 'user',
    address: '10.0.0.9',
    username: null,
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [
      {
        volumeId: 'smb-box-public',
        name: 'public',
        pinned,
        connected: false,
        appRoot: '/Volumes/public',
        username: null,
        autoReconnect: null,
      },
    ],
  })

  it('shows the pin as it stands now, not as it stood when the menu opened', async () => {
    h.listSavedServers.mockResolvedValue([share(true)])
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    await settle()
    const row = [...target.querySelectorAll<HTMLElement>('.server-row')].find((el) => el.textContent.includes('public'))
    row?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 40, clientY: 60 }))
    await settle()
    await settle()
    const labels = () =>
      [...document.querySelectorAll('[data-menu] [data-menu-row]')].map((el) => el.textContent.trim())
    expect(labels()).toContain('Unpin from switcher')

    h.listSavedServers.mockResolvedValue([share(false)])
    api.refresh()
    await vi.waitFor(() => {
      expect(labels()).toContain('Pin to switcher')
    })
    await cleanup()
  })
})

/**
 * The servers Cmdr only FOUND sit in one group under the saved ones: collapsed for
 * someone who saved a server, open for someone who saved none, and as the person
 * last left it once they toggled it.
 */
describe('ServersHub nearby group', () => {
  const header = (target: HTMLElement) => target.querySelector<HTMLElement>('.nearby-group-row')
  const rowNames = (target: HTMLElement) =>
    [...target.querySelectorAll('.server-row .name-text')].map((el) => el.textContent.trim())
  const cursorRow = (target: HTMLElement) => target.querySelector<HTMLElement>('.server-row.is-under-cursor')

  it('opens expanded for someone with no saved server, with the cursor on the first server', async () => {
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    expect(header(target)?.textContent.trim()).toBe('2 servers found nearby')
    expect(header(target)?.querySelector('button')?.getAttribute('aria-expanded')).toBe('true')
    expect(rowNames(target)).toEqual(['Attic', 'Naspolya'])
    expect(api.getRowUnderCursor()?.name).toBe('Attic')
    await cleanup()
  })

  it('opens collapsed for someone with a saved server, showing the header alone', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, cleanup } = mountBehindBothHandlers()
    await settle()
    expect(rowNames(target)).toEqual(['Jump box'])
    expect(header(target)?.textContent.trim()).toBe('2 servers found nearby')
    expect(header(target)?.querySelector('button')?.getAttribute('aria-expanded')).toBe('false')
    await cleanup()
  })

  it('keeps a saved host mDNS also sees among the saved ones, out of the group', async () => {
    h.listSavedServers.mockResolvedValue([savedSmb(mockHosts[0])])
    const { target, cleanup } = mountBehindBothHandlers()
    await settle()
    expect(rowNames(target)).toEqual(['Naspolya'])
    expect(header(target)?.textContent.trim()).toBe('1 server found nearby')
    await cleanup()
  })

  it('walks the arrows over the header and never onto a hidden server', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    expect(api.getRowUnderCursor()?.name).toBe('Jump box')

    api.handleKeyDown(plainKey('ArrowDown'))
    await settle()
    expect(cursorRow(target)).toBe(header(target))
    expect(api.getRowUnderCursor()).toBeNull()

    api.handleKeyDown(plainKey('ArrowDown'))
    await settle()
    expect(cursorRow(target)?.classList.contains('add-row')).toBe(true)
    await cleanup()
  })

  it('toggles on Enter and on Space, once per key, and remembers each choice', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.handleKeyDown(plainKey('ArrowDown'))
    const list = target.querySelector('.row-list')

    // Through both handlers on the key's path: twice would leave it as it was.
    list?.dispatchEvent(plainKey('Enter'))
    await settle()
    expect(h.setSetting).toHaveBeenLastCalledWith('network.nearbyServersGroup', 'expanded')
    expect(rowNames(target)).toEqual(['Jump box', 'Attic', 'Naspolya'])
    expect(cursorRow(target)).toBe(header(target))

    list?.dispatchEvent(plainKey(' '))
    await settle()
    expect(h.setSetting).toHaveBeenLastCalledWith('network.nearbyServersGroup', 'collapsed')
    expect(h.setSetting).toHaveBeenCalledTimes(2)
    expect(rowNames(target)).toEqual(['Jump box'])
    expect(h.dispatched).toEqual([])
    await cleanup()
  })

  it('toggles on a click, which also takes the cursor', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, cleanup } = mountBehindBothHandlers()
    await settle()
    header(target)?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await settle()
    expect(rowNames(target)).toEqual(['Jump box', 'Attic', 'Naspolya'])
    expect(cursorRow(target)).toBe(header(target))
    await cleanup()
  })

  it('opens the way the person last left it, whatever they have saved', async () => {
    h.settings.set('network.nearbyServersGroup', 'expanded')
    h.listSavedServers.mockResolvedValue([savedSftp])
    const saved = mountBehindBothHandlers()
    await settle()
    expect(rowNames(saved.target)).toEqual(['Jump box', 'Attic', 'Naspolya'])
    await saved.cleanup()

    h.settings.set('network.nearbyServersGroup', 'collapsed')
    h.listSavedServers.mockResolvedValue([])
    const none = mountBehindBothHandlers()
    await settle()
    expect(rowNames(none.target)).toEqual([])
    expect(header(none.target)).not.toBeNull()
    await none.cleanup()
  })

  it('stays open through a first save while the view is up, so nothing folds away under the person', async () => {
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    h.listSavedServers.mockResolvedValue([savedSftp])
    api.refresh()
    await settle()
    expect(rowNames(target)).toEqual(['Jump box', 'Attic', 'Naspolya'])
    await cleanup()
  })

  it('moves the cursor to the header when the group collapses over it', async () => {
    h.settings.set('network.nearbyServersGroup', 'expanded')
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, api, cleanup } = mountBehindBothHandlers()
    await settle()
    api.setCursorIndex(api.findItemIndex('Naspolya'))
    expect(api.getRowUnderCursor()?.name).toBe('Naspolya')

    // The other pane's hub collapsed it: the setting is one, the views are two.
    h.settings.set('network.nearbyServersGroup', 'collapsed')
    await settle()
    expect(cursorRow(target)).toBe(header(target))
    expect(api.getRowUnderCursor()).toBeNull()
    await cleanup()
  })

  it('shows a hidden server an agent moves the cursor to, without recording a choice', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, api, onHostSelect, cleanup } = mountBehindBothHandlers()
    await settle()
    // The full list: Jump box, the header, Attic, Naspolya, "Add server…".
    expect(api.getItemCount()).toBe(5)
    expect(api.findItemIndex('Naspolya')).toBe(3)

    api.setCursorIndex(api.findItemIndex('Naspolya'))
    await settle()
    expect(api.getRowUnderCursor()?.name).toBe('Naspolya')
    expect(rowNames(target)).toEqual(['Jump box', 'Attic', 'Naspolya'])
    expect(h.setSetting).not.toHaveBeenCalled()

    api.openCursorItem()
    expect(onHostSelect).toHaveBeenCalledOnce()
    await cleanup()
  })

  it('reaches the header and the add row by the index an agent counts', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { target, api, onConnectToServer, cleanup } = mountBehindBothHandlers()
    await settle()

    api.setCursorIndex(api.findItemIndex('Found nearby'))
    await settle()
    expect(cursorRow(target)).toBe(header(target))

    api.setCursorIndex(api.getItemCount() - 1)
    api.openCursorItem()
    expect(onConnectToServer).toHaveBeenCalledOnce()
    // Reaching past a collapsed group is no reason to open it.
    expect(rowNames(target)).toEqual(['Jump box'])
    await cleanup()
  })
})
