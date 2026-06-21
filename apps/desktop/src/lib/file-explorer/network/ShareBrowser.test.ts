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
  handleKeyDown: (e: KeyboardEvent) => boolean
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

  it('attempts the mount (no in-pane prompt) when creds are required and none are stored', async () => {
    // The listing is creds_required but Cmdr holds no password. Activation must NOT show
    // an in-pane login form: it attempts the mount with no creds, so an already-mounted
    // share just navigates (backend short-circuit) and a genuinely-locked share routes
    // to NetworkMountView's mount-failure login form. Pre-prompting here would re-prompt
    // for already-mounted shares (#6).
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    const onShareSelect = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect)
    await waitForShareList(target)

    api.openCursorItem()
    await vi.waitFor(() => {
      expect(onShareSelect).toHaveBeenCalledWith(expect.objectContaining({ name: 'naspi' }), null)
    })
    expect(target.querySelector('.login-title'), 'must not show an in-pane prompt on activation').toBeNull()

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

describe('ShareBrowser back-navigation', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
  })

  it('⌘↑ goes back to the host list (like Escape / Backspace), not a cursor move', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    const onShareSelect = vi.fn()
    const onBack = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect, onBack)
    await waitForShareList(target)

    const handled = api.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowUp', metaKey: true }))

    expect(handled).toBe(true)
    expect(onBack).toHaveBeenCalledOnce()
    // The cursor-move path must NOT have fired (no share got activated).
    expect(onShareSelect).not.toHaveBeenCalled()

    await unmount(component)
  })
})
