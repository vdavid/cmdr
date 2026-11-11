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

import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { mount, unmount, tick } from 'svelte'
import PlacesBrowser from './PlacesBrowser.svelte'
import { resolveGlobalKeyAction } from '../../../routes/(main)/global-keydown'
import { initShortcutDispatch, destroyShortcutDispatch } from '#lib/shortcuts/shortcut-dispatch.js'
import type { NetworkHost, ShareInfo, ShareListError } from '../types'
import { renderShareListError } from './share-list-error-messages'
import { ShareListFailure } from './share-list-error'

const h = vi.hoisted(() => ({
  fetchShares: vi.fn(),
  listSharesWithCredentials: vi.fn(),
  getSmbCredentials: vi.fn(),
  noteCachedCredentials: vi.fn(() => Promise.resolve()),
  credentialStatus: 'unknown',
  saveSmbCredentials: vi.fn(),
  openSignInSheet: vi.fn(),
  // A `SvelteMap` once the store mock is built, so the header follows it like the real store's.
  listed: new Map<string, unknown>(),
  guestWorks: false,
  setSmbAccountPreference: vi.fn(() => Promise.resolve(true)),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  listSharesWithCredentials: h.listSharesWithCredentials,
  saveSmbCredentials: h.saveSmbCredentials,
  getSmbCredentials: h.getSmbCredentials,
  isUsingCredentialFileFallback: vi.fn(() => Promise.resolve(false)),
  updateKnownShare: vi.fn(() => Promise.resolve()),
  setSmbAccountPreference: h.setSmbAccountPreference,
  getUsernameHint: vi.fn(() => Promise.resolve(null)),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
}))

vi.mock('./network-store.svelte', async () => {
  const { SvelteMap } = await import('svelte/reactivity')
  h.listed = new SvelteMap<string, unknown>()
  return {
    getShareState: () => undefined,
    fetchShares: h.fetchShares,
    clearShareState: vi.fn(),
    setShareState: vi.fn(),
    setCredentialStatus: vi.fn(),
    getCredentialStatus: () => h.credentialStatus,
    noteCachedCredentials: h.noteCachedCredentials,
    forgetCredentials: vi.fn(() => Promise.resolve()),
    getListedAccount: (hostId: string) => h.listed.get(hostId),
    guestListingWorked: () => h.guestWorks,
    setListedAccount: (hostId: string, account: unknown) => h.listed.set(hostId, account),
  }
})

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn(() => 'id') }))

vi.mock('#lib/servers/sign-in-sheet-state.svelte.js', () => ({ openSignInSheet: h.openSignInSheet }))

vi.mock('#lib/settings/network-settings.js', () => ({
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
  jumpToPrefix: (prefix: string) => string | null
  getShareUnderCursor: () => ShareInfo | null
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
// `Ctrl+↑`). Vitest runs off macOS, so pin macOS for these combos.
beforeAll(() => withPlatform('macos'))

describe('PlacesBrowser credential gate', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    h.listed.clear()
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
    h.fetchShares.mockRejectedValue(new ShareListFailure(failure))
    const { target, component } = mountBrowser(vi.fn())

    const errorPane = await vi.waitFor(() => must(target.querySelector('.error-state'), 'the error pane'))
    const message = must(errorPane.querySelector('.error-message'), 'the message').textContent
    expect(message).toBe(renderShareListError(failure, 'Naspolya'))
    expect(message).not.toContain('smbutil')

    await unmount(component)
  })

  it('words a listing that threw something untyped, instead of handing the renderer an Error with no type', async () => {
    // Anything untyped that lands in the listing's catch (a runtime exception, an
    // IPC call that broke) was cast to `ShareListError`, reached
    // `renderShareListError` with `type` undefined, and the pane threw.
    h.fetchShares.mockRejectedValue(new Error('the IPC bridge went away'))
    const { target, component } = mountBrowser(vi.fn())

    const errorPane = await vi.waitFor(() => must(target.querySelector('.error-state'), 'the error pane'))
    const message = must(errorPane.querySelector('.error-message'), 'the message').textContent
    expect(message).toBe(renderShareListError({ type: 'protocol_error', message: '' }, 'Naspolya'))

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

  /**
   * ❗ "Forget saved password" shows once this session has READ a password for the
   * server, whichever way the listing went (a guest listing of a host whose password
   * is in use showed none, QA round 5), and learning that costs no Keychain access:
   * the store asks the backend's cache only (`noteCachedCredentials`).
   */
  it('offers Forget saved password after a guest listing, without asking the Keychain', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.credentialStatus = 'has_creds'
    const { target, component } = mountBrowser(vi.fn())
    await waitForShareList(target)
    expect(
      [...target.querySelectorAll('.header-row button')].find((b) => b.textContent.includes('Forget saved password')),
    ).toBeTruthy()
    expect(h.noteCachedCredentials).toHaveBeenCalledWith('Naspolya')
    expect(h.getSmbCredentials, 'no Keychain read to decide a button').not.toHaveBeenCalled()
    await unmount(component)
  })

  it('offers no Forget button while nothing has read a password', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.credentialStatus = 'unknown'
    const { target, component } = mountBrowser(vi.fn())
    await waitForShareList(target)
    await tick()
    expect(
      [...target.querySelectorAll('.header-row button')].find((b) => b.textContent.includes('Forget saved password')),
    ).toBeUndefined()
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

describe('PlacesBrowser type-to-jump', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
  })

  it('jumps to the first share whose name starts with the typed text, in any case', async () => {
    const shares = [naspi, { ...naspi, name: 'Backup' }, { ...naspi, name: 'backups-old' }]
    h.fetchShares.mockResolvedValue({ shares, authMode: 'guest_allowed', fromCache: false })
    const { target, component, api } = mountBrowser(vi.fn())
    await waitForShareList(target)

    expect(api.jumpToPrefix('BACK')).toBe('Backup')
    expect(api.getShareUnderCursor()?.name).toBe('Backup')
    expect(api.jumpToPrefix(naspi.name)).toBe(naspi.name)
    expect(api.getShareUnderCursor()?.name).toBe(naspi.name)

    expect(api.jumpToPrefix('zz')).toBeNull()
    expect(api.getShareUnderCursor()?.name).toBe(naspi.name)
    await unmount(component)
  })
})

describe('PlacesBrowser Enter', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
    h.openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
  })

  /**
   * Enter reaches this browser twice unless its own branch claims the key: the pane's
   * element-level handler runs first, then the document dispatcher in `+page.svelte`
   * (bubble-phase, no `defaultPrevented` guard) resolves bare Enter to `nav.open`,
   * whose handler posts Enter straight back to the focused pane, which hands the
   * network view every key. Mounting a share twice is the visible half.
   */
  it('activates the share under the cursor once, not once per handler on the path', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    const onShareSelect = vi.fn()
    const { target, component, api } = mountBrowser(onShareSelect)
    await waitForShareList(target)

    const documentDispatcher = (e: KeyboardEvent) => {
      const action = resolveGlobalKeyAction(e, { dialogOpen: false, paletteOpen: false })
      if (action.kind === 'dispatch' && action.commandId === 'nav.open') {
        api.handleKeyDown(new KeyboardEvent('keydown', { key: 'Enter' }))
      }
    }
    const paneHandler = (e: KeyboardEvent) => {
      api.handleKeyDown(e)
    }
    target.addEventListener('keydown', paneHandler)
    document.addEventListener('keydown', documentDispatcher)
    initShortcutDispatch()
    try {
      target.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      await tick()

      expect(onShareSelect).toHaveBeenCalledOnce()
    } finally {
      destroyShortcutDispatch()
      target.removeEventListener('keydown', paneHandler)
      document.removeEventListener('keydown', documentDispatcher)
      await unmount(component)
    }
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
    h.fetchShares.mockRejectedValue(new ShareListFailure({ type: 'auth_required', message: 'Authentication required' }))
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

  it('waits for a sign-in calmly behind the sheet, instead of saying it couldn’t connect', async () => {
    // ❗ A server that wants an account is a normal step, not a failure: no red
    // icon, no "Couldn't connect", and no Retry, since retrying without an account
    // gets the same answer (cmdr-reports#10).
    h.fetchShares.mockRejectedValue(new ShareListFailure({ type: 'auth_required', message: 'Authentication required' }))
    const { target, component } = mountBrowser(vi.fn())

    const pane = await vi.waitFor(() => must(target.querySelector('.sign-in-state'), 'the sign-in pane'))
    expect(target.querySelector('.error-state')).toBeNull()
    expect(must(pane.querySelector('.sign-in-title'), 'the title').textContent.trim()).toBe('Sign in to Naspolya')
    const labels = [...pane.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(labels).toEqual(['Sign in', 'Back'])

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
    h.fetchShares.mockRejectedValue(new ShareListFailure({ type: 'auth_required', message: 'Authentication required' }))
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
      'typed',
      5000,
      30000,
    )
    expect(h.saveSmbCredentials).toHaveBeenCalledWith('Naspolya', null, 'david', 'hunter2')

    await unmount(component)
  })

  it('keeps the sheet open on a password the server refused, with the reason it gave', async () => {
    h.fetchShares.mockRejectedValue(new ShareListFailure({ type: 'auth_required', message: 'Authentication required' }))
    h.listSharesWithCredentials.mockRejectedValue(
      new ShareListFailure({ type: 'auth_failed', message: 'Invalid username or password' }),
    )
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
    h.fetchShares.mockRejectedValue(new ShareListFailure({ type: 'auth_required', message: 'Authentication required' }))
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
      'typed',
      5000,
      30000,
    )

    await unmount(component)
  })

  it('goes back to the host list when the sign-in is cancelled, rather than sitting on a locked list', async () => {
    h.fetchShares.mockRejectedValue(new ShareListFailure({ type: 'auth_required', message: 'Authentication required' }))
    const onBack = vi.fn()
    const { component } = mountBrowser(vi.fn(), onBack)

    await vi.waitFor(() => {
      expect(onBack).toHaveBeenCalledOnce()
    })

    await unmount(component)
  })
})

/**
 * ❗ The share list says the name the person knows the server by: after renaming
 * `localhost:11482` to "My NAS" its header still read the address (QA round 2).
 */
describe('PlacesBrowser header', () => {
  it('names the server by the label it was opened with', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(PlacesBrowser, {
      target,
      props: { account: { protocol: 'smb', host, label: 'My NAS' }, onShareSelect: vi.fn() },
    })
    await vi.waitFor(() => {
      expect(target.querySelector('.host-name')?.textContent.trim()).toBe('My NAS')
    })
    await unmount(component)
  })

  /** ❗ The sheet it opens says the same name: "Sign in to localhost:11482" named a server nobody calls that. */
  it('names the server by that label in the sign-in sheet too', async () => {
    vi.clearAllMocks()
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(PlacesBrowser, {
      target,
      props: { account: { protocol: 'smb', host, label: 'My NAS' }, onShareSelect: vi.fn() },
    })
    await waitForShareList(target)
    ;(
      [...target.querySelectorAll('.header-row button')].find((b) =>
        b.textContent.includes('Sign in as'),
      ) as HTMLButtonElement
    ).click()
    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalledOnce()
    })
    const request = h.openSignInSheet.mock.lastCall?.[0] as SheetRequest & { endpoint: { displayName: string } }
    expect(request.endpoint.displayName).toBe('My NAS')
    expect(request.endpoint.host, 'the key the hints and the store use stays the host').toBe('Naspolya')
    await unmount(component)
  })
})

/**
 * ❗ The share list's header says which account the list is signed in as, and offers
 * "Sign in as…" to see it as someone else: on a `map to guest = bad user` server the
 * guest list is almost empty, and the account's is the one the person came for.
 */
describe('PlacesBrowser: the account it is signed in as', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.listed.clear()
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
  })

  it('names the account and signs in as another one from the header, staying on the list', async () => {
    h.listed.set(host.id, { kind: 'guest' })
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.listSharesWithCredentials.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    let request: SheetRequest | undefined
    h.openSignInSheet.mockImplementation(async (r: SheetRequest) => {
      request = r
      await r.attempt({ mode: 'sign-in', secret: { secret: 'testpass', remember: false }, username: 'testuser' })
      return { kind: 'handed_off' }
    })
    const onBack = vi.fn()
    const { target, component } = mountBrowser(vi.fn(), onBack)
    await waitForShareList(target)
    expect(target.querySelector('.header-row')?.textContent).toContain('as guest')

    const signInAs = must(
      [...target.querySelectorAll('.header-row button')].find((b) => b.textContent.includes('Sign in as')),
      'the Sign in as… button',
    ) as HTMLButtonElement
    signInAs.click()
    await vi.waitFor(() => {
      expect(target.querySelector('.header-row')?.textContent).toContain('as testuser')
    })

    expect(request?.shape).toEqual({ kind: 'username_password', guestAllowed: false })
    expect(request?.refusal, 'nothing was refused: the person asked').toBeUndefined()
    expect(h.listSharesWithCredentials).toHaveBeenCalledWith(
      host.id,
      expect.anything(),
      expect.anything(),
      host.port,
      'testuser',
      'testpass',
      expect.anything(),
      expect.anything(),
      expect.anything(),
    )
    expect(onBack).not.toHaveBeenCalled()
    // ❗ The choice sticks: it becomes the server's account, so later listings sign in as it.
    expect(h.setSmbAccountPreference).toHaveBeenCalledWith('Naspolya', 'testuser')
    await unmount(component)
  })

  /** The way back: "Browse as guest" where guest is known to work, clearing the account. */
  it('offers Browse as guest when signed in on a guest-allowing server, and goes back to guest', async () => {
    h.listed.set(host.id, { kind: 'user', username: 'testuser' })
    h.guestWorks = true
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'creds_required', fromCache: false })
    h.listSharesWithCredentials.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    const { target, component } = mountBrowser(vi.fn())
    await waitForShareList(target)

    const useGuest = must(
      [...target.querySelectorAll('.header-row button')].find((b) => b.textContent.includes('Browse as guest')),
      'the Browse as guest button',
    ) as HTMLButtonElement
    useGuest.click()
    await vi.waitFor(() => {
      // The button's own label says "as guest" too, so wait for it to go first.
      const buttons = [...target.querySelectorAll('.header-row button')]
      expect(buttons.some((b) => b.textContent.includes('Browse as guest'))).toBe(false)
      expect(target.querySelector('.header-row')?.textContent).toContain('as guest')
    })
    expect(h.setSmbAccountPreference).toHaveBeenCalledWith('Naspolya', null)
    // ❗ The account goes FIRST: while it's set, the backend never lists this server as guest.
    expect(h.setSmbAccountPreference.mock.invocationCallOrder[0]).toBeLessThan(
      h.listSharesWithCredentials.mock.invocationCallOrder[0],
    )
    expect(h.listSharesWithCredentials).toHaveBeenCalledWith(
      host.id,
      expect.anything(),
      expect.anything(),
      host.port,
      null,
      null,
      expect.anything(),
      expect.anything(),
      expect.anything(),
    )
    h.guestWorks = false
    await unmount(component)
  })

  it('stays on the list when the person cancels Sign in as…', async () => {
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
    const onBack = vi.fn()
    const { target, component } = mountBrowser(vi.fn(), onBack)
    await waitForShareList(target)
    ;(
      [...target.querySelectorAll('.header-row button')].find((b) =>
        b.textContent.includes('Sign in as'),
      ) as HTMLButtonElement
    ).click()
    await tick()
    await tick()
    expect(h.openSignInSheet).toHaveBeenCalledOnce()
    expect(onBack).not.toHaveBeenCalled()
    await unmount(component)
  })
})
