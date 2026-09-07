/**
 * Which command the sheet's attempt actually sends, for each standing a place
 * can be in.
 *
 * ❗ This is the file where getting it wrong is SILENT. Re-dialing a REGISTERED
 * volume registers a second one under a second id; mending an ABSENT one asks the
 * backend to fix a session that was never there. The cells drive the seam over
 * the real bindings and read the IPC that came out.
 *
 * The sheet itself isn't mounted here: `openSignInSheet` parks the request and
 * hands back a promise, so a test can pick the request up, call its `attempt` as
 * many times as a user would, and close it. That IS the sheet's contract, minus
 * the DOM.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { clearIpcMocks, installIpcMock, type IpcRecorder } from '$lib/ipc/test-helpers'

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { openAddServerSheet, openSignInForPlace } from './open-sign-in'
import { closeSignInSheet, currentSignInRequest } from './sign-in-sheet-state.svelte'
import type { SignInAttempt, SignInSheetRequest } from './sign-in-contract'

const VOLUME_ID = 'sftp-nas-local-22-ada'

const SAVED_SERVER = {
  id: VOLUME_ID,
  protocol: 'sftp',
  displayName: 'Naspolya',
  address: 'nas.local:22',
  username: 'ada',
  pinned: true,
  lastConnectedAt: null,
  places: [
    {
      volumeId: VOLUME_ID,
      name: 'Naspolya',
      pinned: true,
      connected: false,
      appRoot: 'sftp://ada@nas.local:22/srv/data',
    },
  ],
}

let ipc: IpcRecorder

beforeEach(() => {
  vi.clearAllMocks()
  ipc = installIpcMock()
  ipc.mock('list_saved_servers', () => [SAVED_SERVER])
  ipc.mock('get_volume_sign_in_state', () => ({ kind: 'password' }))
  ipc.mock('has_server_secret', () => false)
})
afterEach(() => {
  closeSignInSheet({ kind: 'cancelled' })
  clearIpcMocks()
})

/**
 * The request the sheet would be rendering, once the seam has parked it.
 *
 * The seam asks the backend two questions (the shape and the endpoint) before it
 * opens anything, and the IPC mock answers on the macrotask queue, so a run of
 * microtasks isn't enough to get past them.
 */
async function parkedRequest(): Promise<SignInSheetRequest> {
  for (let i = 0; i < 20 && !currentSignInRequest(); i++) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
  const request = currentSignInRequest()
  if (!request) throw new Error('no sheet was opened')
  return request
}

/** The attempt the parked request carries. */
function attemptOf(request: SignInSheetRequest): SignInAttempt {
  if (request.mode === 'edit') throw new Error('edit mode has no attempt')
  return request.attempt
}

describe('a place that is asking, with nothing registered', () => {
  it('shows what the BACKEND says to ask, and dials the saved entry with the offer', async () => {
    ipc.mock('connect_saved_place', () => ({ outcome: 'connected', volumeId: VOLUME_ID }))
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false })
    const request = await parkedRequest()

    expect(request.mode).toBe('sign-in')
    if (request.mode !== 'sign-in') throw new Error('unreachable')
    // ❗ Asked when the sheet renders, ❌ never derived from a protocol or a rung.
    expect(request.shape).toEqual({ kind: 'password' })
    expect(request.endpoint).toMatchObject({ host: 'nas.local', username: 'ada', displayName: 'Naspolya' })

    const outcome = await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: false },
      username: null,
    })
    expect(outcome).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.lastCall('connect_saved_place')?.payload).toMatchObject({
      volumeId: VOLUME_ID,
      secret: { secret: 'hunter2', remember: false },
    })
    // ❗ `remember: false` rides the dial as a one-shot offer. A `save_*` here
    // would leave a Keychain entry the user declined.
    expect(ipc.callCount('save_sftp_credentials')).toBe(0)
    expect(ipc.callCount('reconnect_volume_with_credentials')).toBe(0)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    expect(await seam).toEqual({ signedIn: true, volumeId: VOLUME_ID })
  })

  it('carries a first connect through its three rounds without closing', async () => {
    // Round 1 asked for the key (the flow already had it, so the sheet opens
    // there), round 2 is refused, round 3 lands. One sheet throughout.
    const prompt = {
      outcome: 'needs_host_key_approval',
      host: 'nas.local',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:x',
      kind: 'unknown',
    } as const
    const answers = [{ outcome: 'authentication_rejected' }, { outcome: 'connected', volumeId: VOLUME_ID }]
    ipc.mock('connect_saved_place', () => answers.shift() ?? { outcome: 'unreachable' })

    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false, firstOutcome: prompt })
    const request = await parkedRequest()
    if (request.mode !== 'sign-in') throw new Error('unreachable')
    // The sheet opens ON the key step, so the fingerprint is what the user sees
    // first rather than a password box over a server nobody has trusted.
    expect(request.hostKey).toEqual(prompt)

    const attempt = attemptOf(request)
    const wrong = { mode: 'sign-in' as const, secret: { secret: 'nope', remember: false }, username: null }
    expect(await attempt(wrong)).toEqual({ kind: 'refused', refusal: 'authentication_rejected' })
    expect(currentSignInRequest()).toBe(request)

    const right = { mode: 'sign-in' as const, secret: { secret: 'hunter2', remember: true }, username: null }
    expect(await attempt(right)).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.callCount('connect_saved_place')).toBe(2)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    expect(await seam).toEqual({ signedIn: true, volumeId: VOLUME_ID })
  })

  it('reads a closed sheet as not signed in, and says nothing about it', async () => {
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false })
    await parkedRequest()
    closeSignInSheet({ kind: 'cancelled' })
    expect(await seam).toEqual({ signedIn: false })
  })
})

describe('a REGISTERED place whose session wants a credential', () => {
  it('mends it and ❌ never dials', async () => {
    ipc.mock('reconnect_volume_with_credentials', () => null)
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: false },
      username: null,
    })
    expect(outcome).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.lastCall('reconnect_volume_with_credentials')?.payload).toMatchObject({
      volumeId: VOLUME_ID,
      // The account the volume already has: `password` renders it read-only,
      // because the volume id IS the account.
      username: 'ada',
      password: 'hunter2',
    })
    // ❗ A dial would register a SECOND volume under a second id.
    expect(ipc.callCount('connect_saved_place')).toBe(0)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await seam
  })

  it('words a typed refusal as its own reason, never off a message', async () => {
    ipc.mock('reconnect_volume_with_credentials', () => {
      throw { type: 'volume', error: { type: 'permissionDenied', data: '/srv' } }
    })
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    const request = await parkedRequest()
    const outcome = await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'nope', remember: false },
      username: null,
    })
    expect(outcome).toEqual({ kind: 'refused', refusal: 'authentication_rejected' })
    closeSignInSheet({ kind: 'cancelled' })
    await seam
  })
})

describe('add mode', () => {
  it('dials a typed server through the add command', async () => {
    ipc.mock('connect_server', () => ({ outcome: 'connected', volumeId: VOLUME_ID }))
    const sheet = openAddServerSheet({ onSmbHandOff: () => {} })
    const request = await parkedRequest()
    expect(request.mode).toBe('add')

    const target = {
      protocol: 'sftp' as const,
      displayName: 'Naspolya',
      host: 'nas.local',
      port: 22,
      username: 'ada',
      remoteRoot: '/',
      keyFile: null,
      useAgent: true,
      autoReconnect: true,
    }
    const outcome = await attemptOf(request)({ mode: 'add', target, secret: null })
    expect(outcome).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.lastCall('connect_server')?.payload).toMatchObject({ target, secret: null })

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await sheet
  })

  it('hands an SMB address to its own places list instead of dialing a session', async () => {
    ipc.mock('connect_to_server', () => ({ host: { id: 'h1', name: 'naspolya' }, sharePath: null }))
    const handOffs: unknown[] = []
    const sheet = openAddServerSheet({
      onSmbHandOff: (handOff) => {
        handOffs.push(handOff)
      },
    })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({ mode: 'add_smb', address: 'naspolya' })
    expect(outcome).toEqual({ kind: 'handed_off' })
    expect(handOffs).toEqual([{ host: { id: 'h1', name: 'naspolya' }, sharePath: null }])
    // ❗ SMB's connect is a share MOUNT, not a session: no server command runs.
    expect(ipc.callCount('connect_server')).toBe(0)

    closeSignInSheet({ kind: 'handed_off' })
    await sheet
  })
})
