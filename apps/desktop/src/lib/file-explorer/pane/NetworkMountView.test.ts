/**
 * Behavior tests for NetworkMountView's mount-failure auth loop.
 *
 * Regression (the "Naspolya dead end"): a mount that failed with an auth-class error
 * (`auth_failed` / `auth_required`, including the formerly unmapped NetAuth -6600)
 * rendered the dead-end error pane: "Try again" replayed the identical credentials and
 * nothing routed to a credential prompt. Auth-class mount errors must ASK; answering
 * retries the mount with the entered credentials and saves them on success when
 * "Remember in Keychain" is checked.
 *
 * ❗ The asking is the one sign-in sheet, so these assert the REQUEST the view makes
 * (its shape, its endpoint, and what its `attempt` calls), ❌ never a form in the pane.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import NetworkMountView from './NetworkMountView.svelte'
import type { NetworkHost, ShareInfo } from '../types'

const h = vi.hoisted(() => ({
  fetchShares: vi.fn(),
  mountNetworkShare: vi.fn(),
  saveSmbCredentials: vi.fn(),
  getSmbCredentials: vi.fn(),
  resolvePathVolume: vi.fn(),
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  openSignInSheet: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  mountNetworkShare: h.mountNetworkShare,
  resolvePathVolume: h.resolvePathVolume,
  saveSmbCredentials: h.saveSmbCredentials,
  getSmbCredentials: h.getSmbCredentials,
  listSharesWithCredentials: vi.fn(() => Promise.resolve({ shares: [], authMode: 'unknown', fromCache: false })),
  isUsingCredentialFileFallback: vi.fn(() => Promise.resolve(false)),
  updateKnownShare: vi.fn(() => Promise.resolve()),
  getUsernameHint: vi.fn(() => Promise.resolve(null)),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
  updateLeftPaneState: h.updateLeftPaneState,
  updateRightPaneState: vi.fn(() => Promise.resolve()),
  removeManualServer: vi.fn(() => Promise.resolve()),
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
  onNetworkHostContextAction: vi.fn(() => Promise.resolve(() => {})),
  disconnectNetworkHost: vi.fn(() => Promise.resolve()),
  connectToServer: vi.fn(() => Promise.resolve()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/servers/sign-in-sheet-state.svelte', () => ({ openSignInSheet: h.openSignInSheet }))

vi.mock('$lib/settings/network-settings', () => ({
  getMountTimeoutMs: () => 15000,
  getNetworkTimeoutMs: () => 5000,
  getShareCacheTtlMs: () => 30000,
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

vi.mock('../network/network-store.svelte', () => ({
  getNetworkHosts: () => [],
  getDiscoveryState: () => 'idle',
  isHostResolving: () => false,
  getShareState: () => undefined,
  getShareCount: () => null,
  isListingShares: () => false,
  isShareDataStale: () => false,
  refreshAllStaleShares: vi.fn(),
  clearShareState: vi.fn(),
  setShareState: vi.fn(),
  setCredentialStatus: vi.fn(),
  fetchShares: h.fetchShares,
  getCredentialStatus: () => 'unknown',
  checkCredentialsForHost: vi.fn(() => Promise.resolve()),
  forgetCredentials: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: vi.fn(() => Promise.resolve(false)) }))
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn(() => 'id') }))
vi.mock('../network/lazy-trigger', () => ({ triggerNetworkDiscovery: vi.fn() }))

const host: NetworkHost = {
  id: 'naspolya-id',
  name: 'Naspolya',
  hostname: 'Naspolya.local',
  ipAddress: '192.168.1.111',
  port: 445,
  source: 'discovered',
}

const naspi: ShareInfo = { name: 'naspi', isDisk: true, comment: null }

/** The sheet request under test, as much of it as these assert. */
interface SheetRequest {
  mode: string
  shape: { kind: string; guestAllowed?: boolean }
  endpoint: { address: string; host: string; username?: string }
  refusal?: string
  attempt: (submission: {
    mode: 'sign-in'
    secret: { secret: string; remember: boolean } | null
    username: string | null
  }) => Promise<{ kind: string; refusal?: string }>
}

/** The one request the view made of the sign-in sheet. */
function sheetRequest(): SheetRequest {
  expect(h.openSignInSheet, 'the sheet to have been asked for').toHaveBeenCalledTimes(1)
  return h.openSignInSheet.mock.calls[0][0] as SheetRequest
}

/** The exported NetworkMountView API surface the tests drive. */
interface NetworkMountViewApi {
  openCursorItem: () => void
  handleKeyDown: (e: KeyboardEvent) => void
  getItemCount: () => number
}

/** Narrows a queried element, failing the test with a readable message when absent. */
function must<T>(value: T | null | undefined, what: string): T {
  expect(value, `expected ${what} to be present`).toBeTruthy()
  return value as T
}

/** Mounts the view on the given host and activates the first share. */
async function mountViewAndActivateShare() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(NetworkMountView, {
    target,
    props: { paneId: 'left', isFocused: true, initialNetworkHost: host },
  })
  const api = component as unknown as NetworkMountViewApi
  await vi.waitFor(() => {
    expect(target.querySelector('.share-row')).toBeTruthy()
  })
  api.openCursorItem()
  await tick()
  return { target, component, api }
}

describe('NetworkMountView mount-failure auth loop', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    // Guest listing succeeded; no creds in play (the incident state).
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
    h.resolvePathVolume.mockResolvedValue({ volume: null })
    h.openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
  })

  it('asks the sheet for this share (not the dead-end error pane) on an auth-class mount error', async () => {
    h.mountNetworkShare.mockRejectedValue({ type: 'auth_failed', message: 'Invalid username or password' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    const request = sheetRequest()
    expect(request.mode).toBe('sign-in')
    // ❗ No guest option: guest is what the mount just tried, and offering it
    // again would be a button that cannot work.
    expect(request.shape).toEqual({ kind: 'username_password', guestAllowed: false })
    // The header names the SHARE, which is what the mount was about.
    expect(request.endpoint.address).toBe('smb://Naspolya/naspi')
    // The refusal that opened the sheet, in the app's vocabulary rather than the
    // backend's sentence.
    expect(request.refusal).toBe('authentication_rejected')
    expect(target.querySelector('.login-container'), 'no form renders in the pane').toBeNull()

    await unmount(component)
  })

  it('retries the mount with entered credentials and saves them on success', async () => {
    h.mountNetworkShare
      .mockRejectedValueOnce({ type: 'auth_failed', message: 'Invalid username or password' })
      .mockResolvedValueOnce({ mountPath: '/Volumes/naspi', alreadyMounted: false })
    const { component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    const outcome = await sheetRequest().attempt({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: true },
      username: 'david',
    })

    expect(outcome).toEqual({ kind: 'handed_off' })
    expect(h.mountNetworkShare).toHaveBeenCalledTimes(2)
    expect(h.mountNetworkShare).toHaveBeenLastCalledWith('192.168.1.111', 'naspi', 'david', 'hunter2', 445, 15000)
    // Remembered only once the mount actually went through.
    await vi.waitFor(() => {
      expect(h.saveSmbCredentials).toHaveBeenCalledWith('Naspolya', null, 'david', 'hunter2')
    })

    await unmount(component)
  })

  it('keeps the sheet open on a second refusal, and saves nothing', async () => {
    h.mountNetworkShare.mockRejectedValue({ type: 'auth_failed', message: 'Invalid username or password' })
    const { component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    const outcome = await sheetRequest().attempt({
      mode: 'sign-in',
      secret: { secret: 'wrong', remember: true },
      username: 'david',
    })

    expect(outcome).toEqual({ kind: 'refused', refusal: 'authentication_rejected' })
    expect(h.saveSmbCredentials).not.toHaveBeenCalled()

    await unmount(component)
  })

  it('hands a refusal about the SHARE back to the pane, which has the words for it', async () => {
    // The sheet's vocabulary is about credentials. A share that went missing
    // between the listing and the mount is the pane's error state to render, with
    // its own Try again.
    h.mountNetworkShare
      .mockRejectedValueOnce({ type: 'auth_failed', message: 'Invalid username or password' })
      .mockRejectedValueOnce({ type: 'share_not_found', message: 'No such share' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    const outcome = await sheetRequest().attempt({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: false },
      username: 'david',
    })

    expect(outcome).toEqual({ kind: 'handed_off' })
    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })

    await unmount(component)
  })

  it('goes back to the share list when the sign-in is cancelled', async () => {
    h.mountNetworkShare.mockRejectedValue({ type: 'auth_failed', message: 'Invalid username or password' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    await vi.waitFor(() => {
      expect(target.querySelector('.share-row'), 'the share list is where cancelling lands').toBeTruthy()
    })
    expect(target.querySelector('.mount-error-state')).toBeNull()

    await unmount(component)
  })

  it('mirrors the mount failure into the pane state MCP reads', async () => {
    // The error pane replaces the share list, but the pane's own `path` and
    // `files` still describe that list. Without this mirror, a failed mount
    // reads from `cmdr://state` as a pane that simply didn't move, with the
    // reason nowhere in the resource.
    h.mountNetworkShare.mockRejectedValue({ type: 'host_unreachable', message: 'Can\'t connect to "Naspolya"' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })

    await vi.waitFor(() => {
      expect(h.updateLeftPaneState).toHaveBeenCalledWith(
        expect.objectContaining({
          mountError: { share: 'naspi', message: 'Can\'t connect to "Naspolya"' },
        }),
      )
    })

    await unmount(component)
  })

  it('clears the mirror itself when the error pane goes away', async () => {
    // A `mountError` that outlives its pane misleads a reader worse than the
    // silence it replaced, and the view that comes next can't be relied on to
    // clear it: `PlacesBrowser` only pushes once it has a share list, so a host
    // that has since gone quiet pushes nothing at all.
    h.mountNetworkShare.mockRejectedValue({ type: 'host_unreachable', message: 'Can\'t connect to "Naspolya"' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.updateLeftPaneState).toHaveBeenCalledWith(
        expect.objectContaining({ mountError: { share: 'naspi', message: 'Can\'t connect to "Naspolya"' } }),
      )
    })

    must(target.querySelector<HTMLElement>('.mount-error-state'), 'the error pane')
    const backButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.includes('Back'))
    must(backButton, 'the Back button').click()

    await vi.waitFor(() => {
      // An explicit `null`, which only this view sends: `PlacesBrowser`'s own push
      // omits the key entirely, so this can't pass on its remount alone.
      expect(h.updateLeftPaneState).toHaveBeenCalledWith(expect.objectContaining({ mountError: null }))
    })

    await unmount(component)
  })

  it('answers the keyboard in the error pane, one step back to the share list', async () => {
    // The error pane mounts neither browser, so the delegation below it landed on
    // nothing: every key went dead and the two buttons were the only way out. That
    // also left the MCP `nav_to_parent` tool (a synthetic Backspace) acking `OK`
    // while the pane sat still, with no way for an agent to leave a failed mount.
    h.mountNetworkShare.mockRejectedValue({ type: 'host_unreachable', message: 'Can\'t connect to "Naspolya"' })
    const { target, component, api } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })

    const event = new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true })
    api.handleKeyDown(event)
    await tick()

    expect(event.defaultPrevented, 'the key must be consumed here').toBe(true)
    expect(target.querySelector('.mount-error-state'), 'the error pane must be gone').toBeNull()
    // ONE step: back to this host's share list, not out to the host list.
    await vi.waitFor(() => {
      expect(target.querySelector('.share-row')).toBeTruthy()
    })

    await unmount(component)
  })

  it('counts no rows while the error pane is up, so a cursor move is refused instead of faked', async () => {
    h.mountNetworkShare.mockRejectedValue({ type: 'host_unreachable', message: 'Can\'t connect to "Naspolya"' })
    const { target, component, api } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })
    expect(api.getItemCount()).toBe(0)

    await unmount(component)
  })

  it('counts the shares on offer while the share list is up', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(NetworkMountView, {
      target,
      props: { paneId: 'left', isFocused: true, initialNetworkHost: host },
    })
    const api = component as unknown as NetworkMountViewApi

    await vi.waitFor(() => {
      expect(target.querySelector('.share-row')).toBeTruthy()
    })
    expect(api.getItemCount()).toBe(1)

    await unmount(component)
  })

  it('keeps the error pane for non-auth mount errors', async () => {
    h.mountNetworkShare.mockRejectedValue({ type: 'host_unreachable', message: 'Can\'t connect to "Naspolya"' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })
    expect(h.openSignInSheet, 'a non-auth failure asks for no credential').not.toHaveBeenCalled()

    await unmount(component)
  })
})
