/**
 * Behavior tests for `NetworkBrowser`'s keyboard handling.
 *
 * The refresh key (⌘R, `pane.refresh`) has two handlers on its path: the pane's
 * own element-level one, which reaches `NetworkBrowser.handleKeyDown`, and the
 * document-level dispatcher in `+page.svelte`, registered bubble-phase with no
 * `defaultPrevented` guard, which routes `pane.refresh` back into the same
 * component through `refreshNetworkHosts()` → `NetworkBrowser.refresh()`.
 *
 * The local handler therefore has to stop propagation, or one keypress re-reads
 * every host's shares twice. These tests wire both handlers the way the app does
 * and count the actual store work, not the handler's return value.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import NetworkBrowser from './NetworkBrowser.svelte'
import { resolveGlobalKeyAction } from '../../../routes/(main)/global-keydown'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { initShortcutDispatch, destroyShortcutDispatch } from '$lib/shortcuts/shortcut-dispatch'
import type { NetworkHost } from '../types'

const h = vi.hoisted(() => ({
  clearShareState: vi.fn(),
  fetchShares: vi.fn(() => Promise.resolve()),
  refreshAllStaleShares: vi.fn(),
}))

const mockHosts: NetworkHost[] = [
  { id: 'h1', name: 'Naspolya', hostname: 'Naspolya.local', ipAddress: '192.168.1.111', port: 445 },
  { id: 'h2', name: 'Attic', hostname: 'attic.local', ipAddress: '192.168.1.112', port: 445 },
]

vi.mock('./network-store.svelte', () => ({
  getNetworkHosts: () => mockHosts,
  getDiscoveryState: () => 'idle',
  isHostResolving: () => false,
  getShareState: () => undefined,
  getShareCount: () => null,
  isListingShares: () => false,
  isShareDataStale: () => false,
  refreshAllStaleShares: h.refreshAllStaleShares,
  clearShareState: h.clearShareState,
  fetchShares: h.fetchShares,
  getCredentialStatus: () => 'unknown',
  checkCredentialsForHost: vi.fn(() => Promise.resolve()),
  forgetCredentials: vi.fn(() => Promise.resolve()),
}))

vi.mock('./lazy-trigger', () => ({ triggerNetworkDiscovery: vi.fn() }))

vi.mock('$lib/tauri-commands', () => ({
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
  removeManualServer: vi.fn(() => Promise.resolve()),
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
  onNetworkHostContextAction: vi.fn(() => Promise.resolve(() => {})),
  disconnectNetworkHost: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: vi.fn(() => Promise.resolve(false)) }))
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn(() => 'id') }))

/**
 * A `pane.refresh` keypress for the platform the test runs on: the default binding
 * is ⌘R on macOS and Ctrl+R elsewhere, and the test env reports non-macOS.
 */
function refreshKeyEvent(): KeyboardEvent {
  const modifier = isMacOS() ? { metaKey: true } : { ctrlKey: true }
  return new KeyboardEvent('keydown', { key: 'r', bubbles: true, ...modifier })
}

/** The exported `NetworkBrowser` API surface these tests drive. */
interface NetworkBrowserApi {
  handleKeyDown: (e: KeyboardEvent) => void
  refresh: () => void
}

/**
 * Mounts the browser behind the two handlers ⌘R really passes through: the pane's
 * element-level one (a descendant of `document`, so it runs first) and the
 * document-level dispatcher, which turns a `pane.refresh` dispatch back into
 * `refresh()` the way `refreshPane` in `pane-commands.ts` does.
 */
function mountBehindBothHandlers() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(NetworkBrowser, {
    target,
    props: { paneId: 'left', isFocused: true, onHostSelect: () => {}, onConnectToServer: () => {} },
  })
  const api = component as unknown as NetworkBrowserApi

  const paneHandler = (e: KeyboardEvent) => { api.handleKeyDown(e); }
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
  return { target, api, cleanup }
}

describe('NetworkBrowser refresh key', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    // The document dispatcher's reverse lookup is built here in the app's startup.
    initShortcutDispatch()
  })

  afterEach(() => {
    destroyShortcutDispatch()
  })

  it('re-reads each host once per ⌘R, not once per handler on the path', async () => {
    const { target, cleanup } = mountBehindBothHandlers()
    await tick()
    h.clearShareState.mockClear()
    h.fetchShares.mockClear()

    const listContainer = target.querySelector('.host-list')
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
