/**
 * Behavior tests for PlacesBrowser's credential gate and its sign-in hand-off.
 *
 * Regression (the "Naspolya dead end"): a share list can load successfully while Cmdr
 * holds no credentials. On macOS, the listing fallback (`smbutil view -N`) reads the
 * SYSTEM Keychain, so the backend returns shares with `authMode: 'creds_required'`
 * but the frontend never collected a username or password. Activating a share in that
 * state used to call `onShareSelect` with `null` credentials, producing a doomed
 * guest mount and a dead-end error pane. Activation must NOT pre-prompt: it attempts
 * the mount, and the mount's own refusal is what asks.
 *
 * ❗ The sign-in itself is the one sheet, so these assert the REQUEST the browser
 * makes (its shape, its endpoint, and what its `attempt` calls), ❌ never a form
 * rendered in the pane.
 */

import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import PlacesBrowser from './PlacesBrowser.svelte'
import type { NetworkHost, ShareInfo, ShareListError } from '../types'
import { renderShareListError } from './share-list-error-messages'

const h = vi.hoisted(() => ({
  fetchShares: vi.fn(),
  listSharesWithCredentials: vi.fn(),
  getSmbCredentials: vi.fn(),
  saveSmbCredentials: vi.fn(),
  openSignInSheet: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  listSharesWithCredentials: h.listSharesWithCredentials,
  saveSmbCredentials: h.saveSmbCredentials,
  getSmbCredentials: h.getSmbCredentials,
  isUsingCredentialFileFallback: vi.fn(() => Promise.resolve(false)),
  updateKnownShare: vi.fn(() => Promise.resolve()),
  getUsernameHint: vi.fn(() => Promise.resolve(null)),
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

vi.mock('$lib/servers/sign-in-sheet-state.svelte', () => ({ openSignInSheet: h.openSignInSheet }))

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

/** Narrows a queried element, failing the test with a readable message when absent. */
function must<T>(value: T | null | undefined, what: string): T {
  expect(value, `expected ${what} to be present`).toBeTruthy()
  return value as T
}

/** The exported PlacesBrowser API surface the tests drive. */
interface PlacesBrowserApi {
  openCursorItem: () => void
  handleKeyDown: (e: KeyboardEvent) => void
}

function mountBrowser(
  onShareSelect: (share: ShareInfo, creds: { username: string; password: string } | null) => void,
  onBack?: () => void,
) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(PlacesBrowser, {
    target,
    props: { account: { protocol: 'smb', host }, onShareSelect, onBack },
  })
  const api = component as unknown as PlacesBrowserApi
  return { target, component, api }
}

async function waitForShareList(target: HTMLElement) {
  await vi.waitFor(() => {
    expect(target.querySelector('.share-row')).toBeTruthy()
  })
}

// The pane's keys resolve through the command registry, whose ⌘-form defaults only
// match a keypress when `isMacOS()` says we're on a Mac (elsewhere ⌘↑ is stored as
// `Ctrl+↑`). happy-dom reports a Linux UA, so pin it to macOS for these combos.
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeAll(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterAll(() => navigatorSpy.mockReset())

describe('PlacesBrowser credential gate', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    // No stored credentials anywhere (the incident state).
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
    h.openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
  })

  it('words a share list that did not load from the catalog, never with the backend diagnostic', async () => {
    // The backend's `message` is an English diagnostic for the log, often a
    // fallback tool's own stderr, so the pane reads the typed reason instead.
    const failure: ShareListError = {
      type: 'host_unreachable',
      message: 'smbutil failed: Connection refused (os error 61)',
    }
    h.fetchShares.mockRejectedValue(failure)
    const { target, component } = mountBrowser(vi.fn())

    const errorPane = await vi.waitFor(() => must(target.querySelector('.error-state'), 'the error pane'))
    const message = must(errorPane.querySelector('.error-message'), 'the message').textContent
    expect(message).toBe(renderShareListError(failure, 'Naspolya'))
    expect(message).not.toContain('smbutil')

    await unmount(component)
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
    expect(h.openSignInSheet, 'must not pre-prompt on activation').not.toHaveBeenCalled()

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
    expect(h.openSignInSheet, 'must not prompt when stored creds exist').not.toHaveBeenCalled()

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

describe('PlacesBrowser back-navigation', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
    h.openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
  })

  it('⌘↑ goes back to the host list (like Escape / Backspace), not a cursor move', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    const onShareSelect = vi.fn()
    const onBack = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect, onBack)
    await waitForShareList(target)

    api.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowUp', metaKey: true }))

    expect(onBack).toHaveBeenCalledOnce()
    // The cursor-move path must NOT have fired (no share got activated).
    expect(onShareSelect).not.toHaveBeenCalled()

    await unmount(component)
  })
})

describe('PlacesBrowser listing sign-in', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
    h.openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
  })

  /** The one request the browser made of the sign-in sheet. */
  function sheetRequest(): SheetRequest {
    expect(h.openSignInSheet, 'the sheet to have been asked for').toHaveBeenCalledTimes(1)
    return h.openSignInSheet.mock.calls[0][0] as SheetRequest
  }

  it('asks the sheet for a username and a password when the LISTING needs one', async () => {
    h.fetchShares.mockRejectedValue({ type: 'auth_required', message: 'Authentication required' })
    const { target, component } = mountBrowser(vi.fn())

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })
    const request = sheetRequest()
    expect(request.mode).toBe('sign-in')
    expect(request.shape).toEqual({ kind: 'username_password', guestAllowed: false })
    // The header names the SERVER, not a share: listing auth is server-level.
    expect(request.endpoint.address).toBe('smb://Naspolya')
    expect(request.endpoint.host).toBe('Naspolya')
    expect(request.refusal).toBe('needs_credentials')
    expect(target.querySelector('.login-container'), 'no form renders in the pane').toBeNull()

    await unmount(component)
  })

  it('offers guest where the host allows one, so a shy share is one click away', async () => {
    // A listing that needs auth on a host whose cached state says guest is allowed.
    h.fetchShares.mockResolvedValueOnce({ shares: [], authMode: 'guest_allowed', fromCache: false })
    const { component, target } = mountBrowser(vi.fn())
    await vi.waitFor(() => {
      expect(target.querySelector('.empty-state')).toBeTruthy()
    })

    must(target.querySelector<HTMLButtonElement>('.empty-state button'), 'the Sign in button').click()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })
    expect(sheetRequest().shape).toEqual({ kind: 'username_password', guestAllowed: true })

    await unmount(component)
  })

  it('lists with what the user typed, and remembers it only once the listing works', async () => {
    h.fetchShares.mockRejectedValue({ type: 'auth_required', message: 'Authentication required' })
    h.listSharesWithCredentials.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    const { component } = mountBrowser(vi.fn())
    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    const outcome = await sheetRequest().attempt({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: true },
      username: 'david',
    })

    expect(outcome).toEqual({ kind: 'handed_off' })
    expect(h.listSharesWithCredentials).toHaveBeenCalledWith(
      'naspolya-id',
      'Naspolya.local',
      '192.168.1.111',
      445,
      'david',
      'hunter2',
      5000,
      30000,
    )
    expect(h.saveSmbCredentials).toHaveBeenCalledWith('Naspolya', null, 'david', 'hunter2')

    await unmount(component)
  })

  it('keeps the sheet open on a password the server refused, with the reason it gave', async () => {
    h.fetchShares.mockRejectedValue({ type: 'auth_required', message: 'Authentication required' })
    h.listSharesWithCredentials.mockRejectedValue({ type: 'auth_failed', message: 'Invalid username or password' })
    const { component } = mountBrowser(vi.fn())
    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    const outcome = await sheetRequest().attempt({
      mode: 'sign-in',
      secret: { secret: 'wrong', remember: true },
      username: 'david',
    })

    expect(outcome).toEqual({ kind: 'refused', refusal: 'authentication_rejected' })
    // ❗ Nothing saved: a credential is written only once it has worked.
    expect(h.saveSmbCredentials).not.toHaveBeenCalled()

    await unmount(component)
  })

  it('sends no account for guest, so the server is asked for exactly what was offered', async () => {
    h.fetchShares.mockRejectedValue({ type: 'auth_required', message: 'Authentication required' })
    h.listSharesWithCredentials.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    const { component } = mountBrowser(vi.fn())
    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    await sheetRequest().attempt({ mode: 'sign-in', secret: null, username: null })

    expect(h.listSharesWithCredentials).toHaveBeenCalledWith(
      'naspolya-id',
      'Naspolya.local',
      '192.168.1.111',
      445,
      null,
      null,
      5000,
      30000,
    )

    await unmount(component)
  })

  it('goes back to the host list when the sign-in is cancelled, rather than sitting on a locked list', async () => {
    h.fetchShares.mockRejectedValue({ type: 'auth_required', message: 'Authentication required' })
    const onBack = vi.fn()
    const { component } = mountBrowser(vi.fn(), onBack)

    await vi.waitFor(() => {
      expect(onBack).toHaveBeenCalledOnce()
    })

    await unmount(component)
  })
})
