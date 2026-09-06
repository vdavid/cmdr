/**
 * Behavior tests for `ServersHub`: the refresh key's double-handler hazard, and
 * what Enter and F8 do to each kind of row.
 *
 * The refresh key (⌘R, `pane.refresh`) has two handlers on its path: the pane's
 * own element-level one, which reaches `ServersHub.handleKeyDown`, and the
 * document-level dispatcher in `+page.svelte`, registered bubble-phase with no
 * `defaultPrevented` guard, which routes `pane.refresh` back into the same
 * component through `refreshNetworkHosts()` → `ServersHub.refresh()`.
 *
 * The local handler therefore has to stop propagation, or one keypress re-reads
 * every host's shares twice. These tests wire both handlers the way the app does
 * and count the actual store work, not the handler's return value.
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
  openServerRowMenu: vi.fn(() => Promise.resolve()),
  addToast: vi.fn(() => 'id'),
}))

const mockHosts: NetworkHost[] = [
  { id: 'h1', name: 'Naspolya', hostname: 'Naspolya.local', ipAddress: '192.168.1.111', port: 445 },
  { id: 'h2', name: 'Attic', hostname: 'attic.local', ipAddress: '192.168.1.112', port: 445 },
]

const savedSftp: SavedServer = {
  id: 'sftp-jump.local-22-ada',
  protocol: 'sftp',
  displayName: 'Jump box',
  address: 'jump.local:22',
  username: 'ada',
  pinned: true,
  lastConnectedAt: '2026-09-01T10:00:00Z',
  places: [
    {
      volumeId: 'sftp-jump.local-22-ada',
      name: 'Jump box',
      pinned: true,
      connected: false,
      appRoot: 'sftp://ada@jump.local:22',
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
}))

vi.mock('./lazy-trigger', () => ({ triggerNetworkDiscovery: vi.fn() }))
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getNetworkEnabled: () => true,
  formattedDate: () => ({ text: '', segments: [] }),
}))
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  settingAnchorId: (id: string) => `setting-${id}`,
}))
vi.mock('../navigation/server-row-actions', () => ({
  forgetSavedServer: h.forgetSavedServer,
  openServerRowMenu: h.openServerRowMenu,
}))

vi.mock('$lib/tauri-commands', () => ({
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
  removeManualServer: vi.fn(() => Promise.resolve()),
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
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
 * Mounts the hub behind the two handlers ⌘R really passes through: the pane's
 * element-level one (a descendant of `document`, so it runs first) and the
 * document-level dispatcher, which turns a `pane.refresh` dispatch back into
 * `refresh()` the way `refreshPane` in `pane-commands.ts` does.
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
    const action = resolveGlobalKeyAction(e, false)
    if (action.kind === 'dispatch' && action.commandId === 'pane.refresh') api.refresh()
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

/** An unmodified keypress the hub's own handler claims. */
function plainKey(key: string): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, bubbles: true })
}

beforeEach(() => {
  vi.clearAllMocks()
  h.listSavedServers.mockResolvedValue([])
  document.body.innerHTML = ''
  // The document dispatcher's reverse lookup is built here in the app's startup.
  initShortcutDispatch()
})

afterEach(() => {
  destroyShortcutDispatch()
})

describe('ServersHub refresh key', () => {
  it('re-reads each host once per ⌘R, not once per handler on the path', async () => {
    const { target, cleanup } = mountBehindBothHandlers()
    await tick()
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
    const { api, cleanup } = mountBehindBothHandlers()
    await tick()
    h.clearShareState.mockClear()
    h.fetchShares.mockClear()

    // The pane can be unfocused (no local handler on the path) while the window
    // shortcut still fires; `refresh()` stays the entry point for that.
    api.refresh()

    expect(h.clearShareState).toHaveBeenCalledTimes(mockHosts.length)
    expect(h.fetchShares).toHaveBeenCalledTimes(mockHosts.length)

    await cleanup()
  })
})

describe('ServersHub rows', () => {
  it('always offers one more row than it lists, which is "Add server…"', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await tick()
    expect(api.getItemCount()).toBe(mockHosts.length + 1)
    await cleanup()
  })

  it('opens an SMB host into its places list', async () => {
    const { api, onHostSelect, onServerSelect, cleanup } = mountBehindBothHandlers()
    await tick()
    api.setCursorIndex(api.findItemIndex('Naspolya'))
    api.openCursorItem()
    expect(onHostSelect).toHaveBeenCalledOnce()
    expect(onServerSelect).not.toHaveBeenCalled()
    await cleanup()
  })

  it('takes a one-place server to its place instead, which is where the pane dials', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { api, onHostSelect, onServerSelect, cleanup } = mountBehindBothHandlers()
    await tick()
    await tick()
    api.setCursorIndex(api.findItemIndex('Jump box'))
    api.openCursorItem()
    expect(onServerSelect).toHaveBeenCalledOnce()
    expect(onHostSelect).not.toHaveBeenCalled()
    await cleanup()
  })

  it('opens the add form from the last row', async () => {
    const { api, onConnectToServer, cleanup } = mountBehindBothHandlers()
    await tick()
    api.setCursorIndex(api.getItemCount() - 1)
    api.openCursorItem()
    expect(onConnectToServer).toHaveBeenCalledOnce()
    await cleanup()
  })

  it('has no row under the cursor on "Add server…", so a command acts on nothing', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await tick()
    api.setCursorIndex(api.getItemCount() - 1)
    expect(api.getRowUnderCursor()).toBeNull()
    await cleanup()
  })
})

describe('ServersHub F8', () => {
  it('forgets a saved server through the servers family, so the hub asks what the switcher asks', async () => {
    h.listSavedServers.mockResolvedValue([savedSftp])
    const { api, cleanup } = mountBehindBothHandlers()
    await tick()
    await tick()
    api.setCursorIndex(api.findItemIndex('Jump box'))
    api.handleKeyDown(plainKey('F8'))
    await tick()
    expect(h.forgetSavedServer).toHaveBeenCalledWith('sftp-jump.local-22-ada', 'Jump box')
    await cleanup()
  })

  it('says a discovered host is not the user’s to remove, rather than doing nothing', async () => {
    const { api, cleanup } = mountBehindBothHandlers()
    await tick()
    api.setCursorIndex(api.findItemIndex('Naspolya'))
    api.handleKeyDown(plainKey('F8'))
    await tick()
    expect(h.forgetSavedServer).not.toHaveBeenCalled()
    expect(h.addToast).toHaveBeenCalledOnce()
    await cleanup()
  })
})
