/**
 * Behavior tests for ShareBrowser's credential gate.
 *
 * Regression (the "Naspolya dead end"): a share list can load successfully while Cmdr
 * holds no credentials. On macOS, the listing fallback (`smbutil view -N`) reads the
 * SYSTEM Keychain, so the backend returns shares with `authMode: 'creds_required'`
 * but the frontend never collected a username or password. Activating a share in that
 * state used to call `onShareSelect` with `null` credentials, producing a doomed
 * guest mount and a dead-end error pane. The gate shows the login form first and
 * fires the share selection only after credentials are validated.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import ShareBrowser from './ShareBrowser.svelte'
import type { NetworkHost, ShareInfo } from '../types'

const h = vi.hoisted(() => ({
  fetchShares: vi.fn(),
  listSharesWithCredentials: vi.fn(),
  getSmbCredentials: vi.fn(),
  saveSmbCredentials: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  listSharesWithCredentials: h.listSharesWithCredentials,
  saveSmbCredentials: h.saveSmbCredentials,
  getSmbCredentials: h.getSmbCredentials,
  isUsingCredentialFileFallback: vi.fn(() => Promise.resolve(false)),
  updateKnownShare: vi.fn(() => Promise.resolve()),
  getUsernameHints: vi.fn(() => Promise.resolve({})),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
}))

vi.mock('./network-store.svelte', () => ({
  getShareState: () => undefined,
  fetchShares: h.fetchShares,
  clearShareState: vi.fn(),
  setShareState: vi.fn(),
  setCredentialStatus: vi.fn(),
  forgetCredentials: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn(() => 'id') }))

vi.mock('$lib/settings/network-settings', () => ({
  getNetworkTimeoutMs: () => 5000,
  getShareCacheTtlMs: () => 30000,
}))

const host: NetworkHost = {
  id: 'naspolya-id',
  name: 'Naspolya',
  hostname: 'Naspolya.local',
  ipAddress: '192.168.1.111',
  port: 445,
  source: 'discovered',
}

const naspi: ShareInfo = { name: 'naspi', isDisk: true, comment: null }

/** The exported ShareBrowser API surface the tests drive. */
interface ShareBrowserApi {
  openCursorItem: () => void
}

/** Narrows a queried element, failing the test with a readable message when absent. */
function must<T>(value: T | null | undefined, what: string): T {
  expect(value, `expected ${what} to be present`).toBeTruthy()
  return value as T
}

function mountBrowser(
  onShareSelect: (share: ShareInfo, creds: { username: string; password: string } | null) => void,
  onBack?: () => void,
) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(ShareBrowser, { target, props: { host, onShareSelect, onBack } })
  const api = component as unknown as ShareBrowserApi
  return { target, component, api }
}

async function waitForShareList(target: HTMLElement) {
  await vi.waitFor(() => {
    expect(target.querySelector('.share-row')).toBeTruthy()
  })
}

describe('ShareBrowser credential gate', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    // No stored credentials anywhere (the incident state).
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
  })

  it('shows the login form instead of selecting the share when creds are required and none are held', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    const onShareSelect = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect)
    await waitForShareList(target)

    api.openCursorItem()
    await vi.waitFor(() => {
      expect(target.querySelector('.login-title')).toBeTruthy()
    })

    expect(onShareSelect).not.toHaveBeenCalled()
    const title = must(target.querySelector('.login-title'), 'the login form')
    expect(title.textContent).toContain('naspi')

    await unmount(component)
  })

  it('uses stored credentials silently when creds are required (no prompt)', async () => {
    // The listing came back creds_required (it succeeded via the system Keychain), but
    // Cmdr has the password saved. Activating the share must reuse it, not re-prompt.
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    h.getSmbCredentials.mockResolvedValue({ username: 'david', password: 'hunter2' })
    const onShareSelect = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect)
    await waitForShareList(target)

    api.openCursorItem()
    await vi.waitFor(() => {
      expect(onShareSelect).toHaveBeenCalledWith(expect.objectContaining({ name: 'naspi' }), {
        username: 'david',
        password: 'hunter2',
      })
    })
    expect(target.querySelector('.login-title'), 'must not prompt when stored creds exist').toBeNull()

    await unmount(component)
  })

  it('selects the pending share with the entered credentials after a successful sign-in', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    h.listSharesWithCredentials.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    const onShareSelect = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect)
    await waitForShareList(target)

    api.openCursorItem()
    await vi.waitFor(() => {
      expect(target.querySelector('#username')).toBeTruthy()
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
      expect(onShareSelect).toHaveBeenCalledWith(expect.objectContaining({ name: 'naspi' }), {
        username: 'david',
        password: 'hunter2',
      })
    })

    await unmount(component)
  })

  it('cancel on the share-gate login form returns to the share list, not the host list', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    const onShareSelect = vi.fn()
    const onBack = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect, onBack)
    await waitForShareList(target)

    api.openCursorItem()
    await vi.waitFor(() => {
      expect(target.querySelector('.login-title')).toBeTruthy()
    })

    // Cancel: the share list is loaded and fine, so stay on it.
    const cancelButton = must(
      Array.from(target.querySelectorAll('button')).find((b) => b.textContent.includes('Cancel')),
      'the Cancel button',
    )
    cancelButton.click()
    await tick()

    expect(onBack).not.toHaveBeenCalled()
    expect(target.querySelector('.share-row')).toBeTruthy()

    await unmount(component)
  })

  it('selects the share directly when guest is allowed (no gate)', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    const onShareSelect = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect)
    await waitForShareList(target)

    api.openCursorItem()
    await tick()

    expect(onShareSelect).toHaveBeenCalledWith(expect.objectContaining({ name: 'naspi' }), null)

    await unmount(component)
  })
})
