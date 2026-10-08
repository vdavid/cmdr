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
import type { MountError, NetworkHost, ShareInfo } from '../types'
import { MountFailure } from '../network/mount-error'
import { renderMountError } from '../network/mount-error-messages'

const h = vi.hoisted(() => ({
  fetchShares: vi.fn(),
  mountNetworkShare: vi.fn(),
  saveSmbCredentials: vi.fn(),
  getSmbCredentials: vi.fn(),
  resolvePathVolume: vi.fn(),
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  openSignInSheet: vi.fn(),
  warn: vi.fn(),
  hosts: [] as NetworkHost[],
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
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
  onNetworkHostContextAction: vi.fn(() => Promise.resolve(() => {})),
  disconnectNetworkHost: vi.fn(() => Promise.resolve()),
  connectToServer: vi.fn(() => Promise.resolve()),
  listSavedServers: vi.fn(() => Promise.resolve([])),
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
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: h.warn, error: vi.fn() }),
}))

vi.mock('../network/network-store.svelte', () => ({
  getNetworkHosts: () => h.hosts,
  getDiscoveryState: () => 'idle',
  holdDiscoveryForServersView: () => () => {},
  isHostResolving: () => false,
  getShareState: () => undefined,
  getShareCount: () => null,
  isListingShares: () => false,
  isShareDataStale: () => false,
  refreshAllStaleShares: vi.fn(),
  clearShareState: vi.fn(),
  setShareState: vi.fn(),
  setCredentialStatus: vi.fn(),
  noteCachedCredentials: vi.fn(() => Promise.resolve()),
  fetchShares: h.fetchShares,
  getCredentialStatus: () => 'unknown',
  getListedAccount: () => undefined,
  setListedAccount: vi.fn(),
  checkCredentialsForHost: vi.fn(() => Promise.resolve()),
  forgetCredentials: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: vi.fn(() => Promise.resolve(false)) }))
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn(() => 'id') }))
vi.mock('../network/lazy-trigger', () => ({ triggerNetworkDiscovery: vi.fn() }))

/** The address the view mounts by, which is NOT the name a person knows the host by. */
const ADDRESS = '192.168.1.111'

const host: NetworkHost = {
  id: 'naspolya-id',
  name: 'Naspolya',
  hostname: 'Naspolya.local',
  ipAddress: ADDRESS,
  port: 445,
  source: 'discovered',
}

const naspi: ShareInfo = { name: 'naspi', isDisk: true, comment: null }

/** What `mountNetworkShare` throws for a refusal: the typed value, carried across the throw. */
function refused(error: MountError): MountFailure {
  return new MountFailure(error)
}

const authFailed: MountError = { type: 'auth_failed', server: ADDRESS }
const authRequired: MountError = { type: 'auth_required', server: ADDRESS, share: 'naspi' }
const accountRefused: MountError = { type: 'permission_denied', server: ADDRESS, share: 'naspi', username: 'david' }
const unreachable: MountError = { type: 'host_unreachable', server: ADDRESS }

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
    h.mountNetworkShare.mockRejectedValue(refused(authFailed))
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
      .mockRejectedValueOnce(refused(authFailed))
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
    // The host's own name rides along, which is what the saved share is filed under.
    expect(h.mountNetworkShare).toHaveBeenLastCalledWith(ADDRESS, 'naspi', 'david', 'hunter2', 445, 15000, 'Naspolya')
    // Remembered only once the mount actually went through.
    await vi.waitFor(() => {
      expect(h.saveSmbCredentials).toHaveBeenCalledWith('Naspolya', null, 'david', 'hunter2')
    })

    await unmount(component)
  })

  it('keeps the sheet open on a second refusal, and saves nothing', async () => {
    h.mountNetworkShare.mockRejectedValue(refused(authFailed))
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

  it('asks for a sign-in when the share refuses guests, saying a password is needed', async () => {
    // ERR-SHUSC: a share guests can SEE but can't OPEN. The backend clarifies the
    // mount's "not found" into `auth_required`, and that has to open the sheet
    // rather than the dead-end error pane.
    h.mountNetworkShare.mockRejectedValue(refused(authRequired))
    const { component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })
    // ❗ Nothing was offered, so it isn't a rejected password.
    expect(sheetRequest().refusal).toBe('needs_credentials')

    await unmount(component)
  })

  it('waits for the sign-in calmly behind the sheet, instead of saying the mount failed', async () => {
    // ❗ A share that wants an account is a normal step: no ❌, no "Couldn't mount
    // share", and no Try again, since trying again without an account gets the
    // same answer (cmdr-reports#10). The sheet stays up for this check.
    h.openSignInSheet.mockReturnValue(new Promise(() => {}))
    h.mountNetworkShare.mockRejectedValue(refused(authRequired))
    const { target, component } = await mountViewAndActivateShare()

    const pane = await vi.waitFor(() => must(target.querySelector('.mount-sign-in-state'), 'the sign-in pane'))
    expect(target.querySelector('.mount-error-state')).toBeNull()
    expect(must(pane.querySelector('.sign-in-title'), 'the title').textContent.trim()).toBe('Sign in to open “naspi”')
    const labels = [...pane.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(labels).toEqual(['Sign in', 'Back'])

    await unmount(component)
  })

  it('asks for a different account when the share refuses the one that signed in', async () => {
    // The account got past sign-in; the SHARE doesn't let it in. The fix is another
    // account, which the sheet takes, so this is its question and not the pane's.
    h.mountNetworkShare.mockRejectedValue(refused(accountRefused))
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })
    expect(sheetRequest().refusal).toBe('account_not_permitted')
    expect(target.querySelector('.mount-error-state'), 'no dead-end pane in front of the sheet').toBeNull()

    await unmount(component)
  })

  it('keeps the sheet open when the share refuses the account the user just signed in with', async () => {
    h.mountNetworkShare.mockRejectedValueOnce(refused(authRequired)).mockRejectedValueOnce(refused(accountRefused))
    const { component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.openSignInSheet).toHaveBeenCalled()
    })

    const outcome = await sheetRequest().attempt({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: true },
      username: 'david',
    })

    // ❗ Not `authentication_rejected`: the password worked, so "that password didn't
    // work" would send the user to fix the one thing that isn't wrong.
    expect(outcome).toEqual({ kind: 'refused', refusal: 'account_not_permitted' })
    expect(h.saveSmbCredentials, 'nothing is remembered for a mount that did not go through').not.toHaveBeenCalled()

    await unmount(component)
  })

  it('hands a refusal about the SHARE back to the pane, which has the words for it', async () => {
    // The sheet's vocabulary is about credentials. A share that went missing
    // between the listing and the mount is the pane's error state to render, with
    // its own Try again.
    h.mountNetworkShare
      .mockRejectedValueOnce(refused(authFailed))
      .mockRejectedValueOnce(refused({ type: 'share_not_found', server: ADDRESS, share: 'naspi' }))
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

  it('words a refusal in the pane from the catalog, naming the host by its own name', async () => {
    // ERR-SHUSC: the pane showed the backend's English `Share "data" not found on
    // "observermch"` in an otherwise Hungarian UI. The words are the catalog's now,
    // and the server is the host a person picked, not the address it was mounted by.
    const notFound: MountError = { type: 'share_not_found', server: ADDRESS, share: 'naspi' }
    h.mountNetworkShare.mockRejectedValue(refused(notFound))
    const { target, component } = await mountViewAndActivateShare()

    const errorPane = await vi.waitFor(() => must(target.querySelector('.mount-error-state'), 'the error pane'))
    const message = must(errorPane.querySelector('.error-message'), 'the message').textContent
    expect(message).toBe(renderMountError(notFound, 'Naspolya'))
    expect(message).toContain('“Naspolya”')
    expect(message).not.toContain(ADDRESS)

    await unmount(component)
  })

  it('logs a refusal as readable text, not "[object Object]"', async () => {
    // The log bridge renders each property with `String()`, so an object logged as-is
    // reached the log file as `did not go through: [object Object]` (QA round 3).
    const notFound: MountError = { type: 'share_not_found', server: ADDRESS, share: 'naspi' }
    h.mountNetworkShare.mockRejectedValue(refused(notFound))
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => must(target.querySelector('.mount-error-state'), 'the error pane'))
    const [template, properties] = h.warn.mock.calls[0] as [string, Record<string, unknown>]
    expect(template).toContain('{error}')
    expect(String(properties.error)).toContain('share_not_found')

    await unmount(component)
  })

  it('words a mount call that broke down without a typed refusal as the catch-all', async () => {
    // Not a refusal at all: the IPC call itself threw. Its text is a diagnostic for
    // the log, so the pane says the honest general thing instead of showing it.
    h.mountNetworkShare.mockRejectedValue(new Error('IPC transport closed'))
    const { target, component } = await mountViewAndActivateShare()

    const errorPane = await vi.waitFor(() => must(target.querySelector('.mount-error-state'), 'the error pane'))
    const message = must(errorPane.querySelector('.error-message'), 'the message').textContent
    expect(message).toBe(
      renderMountError({ type: 'unexpected', server: ADDRESS, share: 'naspi', detail: 'ignored' }, 'Naspolya'),
    )
    expect(message).not.toContain('IPC transport')
    expect(h.openSignInSheet, 'no credential answers a broken call').not.toHaveBeenCalled()

    await unmount(component)
  })

  it('shows a mount the system reported but never made as the pane error, and Try again mounts again', async () => {
    // ERR-SHUSC: macOS said the share connected and nothing got mounted. No
    // credential answers that, so the sheet stays out of it, and the pane's
    // Try again is a real second attempt rather than a dead end.
    h.mountNetworkShare
      .mockRejectedValueOnce(refused({ type: 'mount_missing', server: ADDRESS, share: 'naspi' }))
      .mockResolvedValueOnce({ mountPath: '/Volumes/naspi', alreadyMounted: false })
    const { target, component } = await mountViewAndActivateShare()

    const errorPane = await vi.waitFor(() => must(target.querySelector('.mount-error-state'), 'the error pane'))
    expect(h.openSignInSheet).not.toHaveBeenCalled()

    must(errorPane.querySelector<HTMLButtonElement>('.error-actions button'), 'Try again').click()

    await vi.waitFor(() => {
      expect(h.mountNetworkShare).toHaveBeenCalledTimes(2)
    })
    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state'), 'the retry went through').toBeNull()
    })

    await unmount(component)
  })

  it('goes back to the share list when the sign-in is cancelled', async () => {
    h.mountNetworkShare.mockRejectedValue(refused(authFailed))
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
    // reason nowhere in the resource. The sentence is in the UI's language, so
    // the typed reason rides beside it.
    h.mountNetworkShare.mockRejectedValue(refused(unreachable))
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })

    await vi.waitFor(() => {
      expect(h.updateLeftPaneState).toHaveBeenCalledWith(
        expect.objectContaining({
          mountError: {
            share: 'naspi',
            reason: 'host_unreachable',
            message: renderMountError(unreachable, 'Naspolya'),
          },
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
    h.mountNetworkShare.mockRejectedValue(refused(unreachable))
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(h.updateLeftPaneState).toHaveBeenCalledWith(
        expect.objectContaining({
          mountError: {
            share: 'naspi',
            reason: 'host_unreachable',
            message: renderMountError(unreachable, 'Naspolya'),
          },
        }),
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
    h.mountNetworkShare.mockRejectedValue(refused(unreachable))
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
    h.mountNetworkShare.mockRejectedValue(refused(unreachable))
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
    h.mountNetworkShare.mockRejectedValue(refused(unreachable))
    const { target, component } = await mountViewAndActivateShare()

    await vi.waitFor(() => {
      expect(target.querySelector('.mount-error-state')).toBeTruthy()
    })
    expect(h.openSignInSheet, 'a non-auth failure asks for no credential').not.toHaveBeenCalled()

    await unmount(component)
  })
})

/**
 * ❗ cmdr-reports#7: "Cmdr tried to open the share on that server." A share queued
 * to auto-mount (an `smb://host/share` address, "Copy path between panes") is a
 * one-shot for the visit it was queued with. It used to survive every later
 * visit, and each fresh share list fired it again, so opening a host row the
 * person meant to browse mounted a share they never picked.
 */
describe('NetworkMountView auto-mount', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.hosts = [host]
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
    h.resolvePathVolume.mockResolvedValue({ volume: null })
    h.mountNetworkShare.mockResolvedValue({ mountPath: '/Volumes/naspi', alreadyMounted: false })
  })

  it('mounts a queued share once, and opening the host again later only lists its shares', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const consumed = vi.fn()
    const component = mount(NetworkMountView, {
      target,
      props: {
        paneId: 'left',
        isFocused: true,
        initialNetworkHost: host,
        initialAutoMountShare: 'naspi',
        onAutoMountConsumed: consumed,
      },
    })
    const api = component as unknown as NetworkMountViewApi

    await vi.waitFor(() => {
      expect(h.mountNetworkShare).toHaveBeenCalledTimes(1)
    })
    // The mount went through, so the pane is back on the servers list.
    await vi.waitFor(() => {
      expect(target.querySelector('.server-row')).toBeTruthy()
    })
    expect(consumed).toHaveBeenCalled()

    // The person opens the same host from the list, to browse it.
    api.openCursorItem()
    await vi.waitFor(() => {
      expect(target.querySelector('.share-row')).toBeTruthy()
    })
    await tick()

    expect(h.mountNetworkShare, 'no second mount nobody asked for').toHaveBeenCalledTimes(1)
    await unmount(component)
  })
})

/**
 * ❗ Back from a host's share list puts the cursor on THAT host's row, found by the
 * host's identity: it used to land on the first row whichever host the person came from
 * (final QA).
 */
describe('NetworkMountView: back from a share list', () => {
  const attic: NetworkHost = { id: 'attic-id', name: 'Attic', hostname: 'attic.local', port: 445, source: 'discovered' }

  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    h.hosts = [attic, host]
    h.fetchShares.mockResolvedValue({ shares: [naspi], authMode: 'guest_allowed', fromCache: false })
    h.getSmbCredentials.mockRejectedValue(new Error('not found'))
  })

  it('puts the cursor on the host it came back from', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(NetworkMountView, { target, props: { paneId: 'left', isFocused: true } })
    const api = component as unknown as NetworkMountViewApi
    await vi.waitFor(() => {
      expect(target.querySelectorAll('.server-row').length).toBeGreaterThan(1)
    })

    // Down to Naspolya (the second row), then in.
    api.handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }))
    await tick()
    api.openCursorItem()
    await vi.waitFor(() => {
      expect(target.querySelector('.share-row')).toBeTruthy()
    })

    api.handleKeyDown(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(target.querySelector('.server-row.is-under-cursor')?.textContent).toContain('Naspolya')
    })
    await unmount(component)
  })
})
