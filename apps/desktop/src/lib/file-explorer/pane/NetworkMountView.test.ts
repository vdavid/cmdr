/**
 * Behavior tests for NetworkMountView's mount-failure auth loop.
 *
 * Regression (the "Naspolya dead end"): a mount that failed with an auth-class error
 * (`auth_failed` / `auth_required`, including the formerly unmapped NetAuth -6600)
 * rendered the dead-end error pane: "Try again" replayed the identical credentials and
 * nothing routed to a credential prompt. Auth-class mount errors must render the login
 * form; submitting retries the mount with the entered credentials and saves them on
 * success when "Remember in Keychain" is checked.
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
}))

vi.mock('$lib/tauri-commands', () => ({
  mountNetworkShare: h.mountNetworkShare,
  resolvePathVolume: h.resolvePathVolume,
  saveSmbCredentials: h.saveSmbCredentials,
  getSmbCredentials: h.getSmbCredentials,
  listSharesWithCredentials: vi.fn(() => Promise.resolve({ shares: [], authMode: 'unknown', fromCache: false })),
  isUsingCredentialFileFallback: vi.fn(() => Promise.resolve(false)),
  updateKnownShare: vi.fn(() => Promise.resolve()),
  getUsernameHints: vi.fn(() => Promise.resolve({})),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
  removeManualServer: vi.fn(() => Promise.resolve()),
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
  onNetworkHostContextAction: vi.fn(() => Promise.resolve(() => {})),
  disconnectNetworkHost: vi.fn(() => Promise.resolve()),
  connectToServer: vi.fn(() => Promise.resolve()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

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

/** The exported NetworkMountView API surface the tests drive. */
interface NetworkMountViewApi {
  openCursorItem: () => void
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
  return { target, component }
}

describe('NetworkMountView mount-failure auth loop', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    // Guest listing succeeded; no creds in play (the incident state).
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
    h.resolvePathVolume.mockResolvedValue({ volume: null })
  })

  it('renders the login form (not the dead-end error pane) on an auth-class mount error', async () => {
    h.mountNetworkShare.mockRejectedValue({ type: 'auth_failed', message: 'Invalid username or password' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.mountNetworkShare).toHaveBeenCalled()
    })
    await tick()

    expect(target.querySelector('.mount-error-state'), 'auth errors must not dead-end in the error pane').toBeNull()
    const title = must(target.querySelector('.login-title'), 'the login form')
    expect(title.textContent).toContain('naspi')
    // The mount error is surfaced inside the form.
    expect(must(target.querySelector('.error-message'), 'the inline error').textContent).toContain(
      'Invalid username or password',
    )

    await unmount(component)
  })

  it('retries the mount with entered credentials and saves them on success', async () => {
    h.mountNetworkShare
      .mockRejectedValueOnce({ type: 'auth_failed', message: 'Invalid username or password' })
      .mockResolvedValueOnce({ mountPath: '/Volumes/naspi', alreadyMounted: false })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.login-title')).toBeTruthy()
    })

    const usernameInput = must(target.querySelector<HTMLInputElement>('#username'), 'the username input')
    const passwordInput = must(target.querySelector<HTMLInputElement>('#password'), 'the password input')
    usernameInput.value = 'david'
    usernameInput.dispatchEvent(new Event('input', { bubbles: true }))
    passwordInput.value = 'hunter2'
    passwordInput.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    must(target.querySelector('form'), 'the login form element').dispatchEvent(
      new Event('submit', { bubbles: true, cancelable: true }),
    )

    await vi.waitFor(() => {
      expect(h.mountNetworkShare).toHaveBeenCalledTimes(2)
    })
    expect(h.mountNetworkShare).toHaveBeenLastCalledWith('192.168.1.111', 'naspi', 'david', 'hunter2', 445, 15000)

    // "Remember in Keychain" defaults to on → credentials saved after the successful mount.
    await vi.waitFor(() => {
      expect(h.saveSmbCredentials).toHaveBeenCalledWith('Naspolya', null, 'david', 'hunter2')
    })

    await unmount(component)
  })

  it('keeps the error pane for non-auth mount errors', async () => {
    h.mountNetworkShare.mockRejectedValue({ type: 'host_unreachable', message: 'Can\'t connect to "Naspolya"' })
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })
    expect(target.querySelector('.login-title')).toBeNull()

    await unmount(component)
  })
})
