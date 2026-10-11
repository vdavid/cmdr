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

const { warn } = vi.hoisted(() => ({ warn: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn, info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

/** The volume list as the store holds it: empty unless a cell says a place was already live. */
let liveVolumes: { id: string; connectionState: string }[] = []
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => liveVolumes }))

import { openAddServerSheet, openSignInForPlace } from './open-sign-in'
import { closeSignInSheet, currentSignInRequest, dismissSignInForPlaces } from './sign-in-sheet-state.svelte'
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
  liveVolumes = []
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

    // The third round turns Remember on, which is a write of its own before the
    // dial: the box is the user's act, ❌ never a side effect of the dial.
    ipc.mock('save_sftp_credentials', () => null)
    const right = { mode: 'sign-in' as const, secret: { secret: 'hunter2', remember: true }, username: null }
    expect(await attempt(right)).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.callCount('connect_saved_place')).toBe(2)
    expect(ipc.callCount('save_sftp_credentials')).toBe(1)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    expect(await seam).toEqual({ signedIn: true, volumeId: VOLUME_ID })
  })

  it('closes onto the place when another dial registered it while the sheet was open', async () => {
    // ❗ `volumes-changed` is debounced, so a sheet can be open over a place a
    // second pane registers a moment later. The dial then refuses as
    // `already_connected`: the place is live, ❌ never "couldn't reach".
    ipc.mock('connect_saved_place', () => {
      throw { reason: 'already_connected', volumeId: VOLUME_ID }
    })
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: false },
      username: null,
    })
    expect(outcome).toEqual({ kind: 'connected', volumeId: VOLUME_ID })

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await seam
  })

  it('still says the connection did not happen when nothing saved answers for the place', async () => {
    // ❗ Not a silent close: a closed sheet hands the pane nothing to say, and a
    // row that outlives the refusal would leave the pane blank for good.
    ipc.mock('connect_saved_place', () => {
      throw { reason: 'no_such_server', volumeId: VOLUME_ID }
    })
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: false },
      username: null,
    })
    expect(outcome).toEqual({ kind: 'refused', refusal: 'unreachable' })

    closeSignInSheet({ kind: 'cancelled' })
    await seam
  })

  it('reads a closed sheet as not signed in, and says nothing about it', async () => {
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false })
    await parkedRequest()
    closeSignInSheet({ kind: 'cancelled' })
    expect(await seam).toEqual({ signedIn: false })
  })

  /**
   * ❗ A place that moved to a new address takes its open sheet down with it: a
   * host key approved there afterwards would dial the OLD address. The pane
   * follows the place and asks again at the new one.
   */
  it('closes as not signed in when its place moves, and leaves another place’s sheet open', async () => {
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false })
    await parkedRequest()

    dismissSignInForPlaces(['sftp-somewhere-else-22-ada'])
    expect(currentSignInRequest()).not.toBeNull()
    dismissSignInForPlaces([VOLUME_ID])

    expect(currentSignInRequest()).toBeNull()
    expect(await seam).toEqual({ signedIn: false })
  })
})

describe('why the sheet opened', () => {
  it('carries the refusal that sent the user here, so the first round already says so', async () => {
    ipc.mock('connect_saved_place', () => ({ outcome: 'authentication_rejected' }))
    const seam = openSignInForPlace({
      volumeId: VOLUME_ID,
      registered: false,
      firstOutcome: { outcome: 'authentication_rejected' },
    })
    const request = await parkedRequest()
    if (request.mode !== 'sign-in') throw new Error('unreachable')
    // ❗ An empty password box with no sentence asks a person to guess why they
    // are being asked. The refusal goes under the field it is about.
    expect(request.refusal).toBe('authentication_rejected')
    closeSignInSheet({ kind: 'cancelled' })
    await seam
  })

  it('says "nothing was offered" for a place that was never asked for one', async () => {
    const seam = openSignInForPlace({
      volumeId: VOLUME_ID,
      registered: false,
      firstOutcome: { outcome: 'needs_credentials' },
    })
    const request = await parkedRequest()
    if (request.mode !== 'sign-in') throw new Error('unreachable')
    // ❌ `needs_credentials` is NOT `authentication_rejected`: nothing was ever
    // offered, so calling it a rejection accuses a password that never left.
    expect(request.refusal).toBe('needs_credentials')
    closeSignInSheet({ kind: 'cancelled' })
    await seam
  })

  it('❌ never opens over a shape that asks NOTHING', async () => {
    // ❗ A key-only or agent-only server: `reconnect_with_credentials` answers
    // `NotSupported` every time, so a password box over it asks for something
    // that cannot help. The pane's `signed_out` banner with no button is what
    // the person should see instead.
    ipc.mock('get_volume_sign_in_state', () => ({ kind: 'nothing' }))
    const result = await openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    expect(result).toEqual({ signedIn: false })
    expect(currentSignInRequest()).toBeNull()
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
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()
    expect(request.mode).toBe('add')

    const target = {
      protocol: 'sftp' as const,
      displayName: 'Naspolya',
      host: 'nas.local',
      port: 22,
      username: 'ada',
      remoteRoot: '/',
      startFolder: null,
      keyFile: null,
      useAgent: true,
      autoReconnect: true,
    }
    const outcome = await attemptOf(request)({ mode: 'add', target, secret: null, intent: 'open' })
    expect(outcome).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.lastCall('connect_server')?.payload).toMatchObject({ target, secret: null })

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await sheet
  })

  it('hands the caller the place it connected to, at its root, so a pane can land there', async () => {
    // Pre-fix nobody was told: the sheet closed on a live server and every pane
    // stayed where it was, so "Connect" looked like it had done nothing.
    const landed: unknown[] = []
    const sheet = openAddServerSheet({
      onSmbHandOff: () => {},
      onConnected: (place) => {
        landed.push(place)
      },
    })
    await parkedRequest()

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await sheet
    expect(landed).toEqual([{ volumeId: VOLUME_ID, root: 'sftp://ada@nas.local:22/srv/data' }])
  })

  it('lands nobody anywhere when the sheet closes without connecting', async () => {
    const landed: unknown[] = []
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: (place) => landed.push(place) })
    await parkedRequest()

    closeSignInSheet({ kind: 'cancelled' })
    await sheet
    expect(landed).toEqual([])
  })

  it('hands an SMB address to its own places list instead of dialing a session', async () => {
    ipc.mock('connect_to_server', () => ({ host: { id: 'h1', name: 'naspolya' }, sharePath: null }))
    const handOffs: unknown[] = []
    const sheet = openAddServerSheet({
      onSmbHandOff: (handOff) => {
        handOffs.push(handOff)
      },
      onConnected: () => {},
    })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'add_smb',
      address: 'naspolya',
      name: '',
      username: null,
      intent: 'open',
    })
    expect(outcome).toEqual({ kind: 'handed_off' })
    expect(handOffs).toEqual([{ host: { id: 'h1', name: 'naspolya' }, sharePath: null }])
    // ❗ SMB's connect is a share MOUNT, not a session: no server command runs.
    expect(ipc.callCount('connect_server')).toBe(0)

    closeSignInSheet({ kind: 'handed_off' })
    await sheet
  })

  it('sends the name and the account typed beside an SMB address', async () => {
    ipc.mock('connect_to_server', () => ({ host: { id: 'h1', name: '192.168.0.153' }, sharePath: null }))
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()

    await attemptOf(request)({
      mode: 'add_smb',
      address: 'smb://192.168.0.153',
      name: "Sven's NAS",
      username: 'sven',
      intent: 'open',
    })

    expect(ipc.lastCall('connect_to_server')?.payload).toEqual({
      address: 'smb://192.168.0.153',
      name: "Sven's NAS",
      username: 'sven',
      checkReachability: true,
    })
    closeSignInSheet({ kind: 'handed_off' })
    await sheet
  })

  it('keeps a password typed into an SMB address out of the log when adding it breaks down', async () => {
    // Warn lines reach the log file and every error-report bundle, and
    // `smb://user:password@host` is a spelling people really paste.
    ipc.mock('connect_to_server', () => {
      throw new Error("Couldn't reach 192.168.1.5:445")
    })
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'add_smb',
      address: 'smb://ada:hunter2@naspolya/photos',
      name: '',
      username: null,
      intent: 'open',
    })
    expect(outcome).toEqual({ kind: 'refused', refusal: 'unreachable' })
    expect(warn).toHaveBeenCalledOnce()
    expect(JSON.stringify(warn.mock.calls)).not.toContain('hunter2')
    // Still names the machine, so the line stays useful at triage.
    expect(warn.mock.calls[0][1]).toMatchObject({ host: 'naspolya' })

    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })
})

/**
 * ❗ "Remember" means exactly "the Keychain holds a secret for this account"
 * (`crates/cmdr-sftp/DETAILS.md` § "The two switches"). The box is the user's
 * explicit act, so a flip is WRITTEN here, and ❌ never left to a dial's side
 * effect: an attended sign-in refreshes a remembered secret and never seeds one,
 * so turning the box ON without a write buys nothing, and turning it OFF without
 * one leaves `refresh_remembered_secret` free to put the typed password straight
 * back into the entry the user just declined.
 */
describe('the Remember box in sign-in mode', () => {
  /** Where a command first ran, so a cell can say which of two landed first. */
  const firstRan = (command: string) => ipc.calls.findIndex((call) => call.command === command)

  it('starts where the STORE stands, so the box reports rather than proposes', async () => {
    ipc.mock('has_server_secret', () => true)
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    const request = await parkedRequest()
    if (request.mode !== 'sign-in') throw new Error('unreachable')
    expect(request.remembered).toBe(true)
    closeSignInSheet({ kind: 'cancelled' })
    await seam
  })

  it('forgets the stored secret the moment the box goes OFF, and saves nothing', async () => {
    ipc.mock('has_server_secret', () => true)
    ipc.mock('forget_server_secret', () => true)
    ipc.mock('reconnect_volume_with_credentials', () => null)
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: false },
      username: null,
    })
    expect(outcome).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.lastCall('forget_server_secret')?.payload).toMatchObject({ id: VOLUME_ID })
    // ❗ The delete lands BEFORE the mend: `refresh_remembered_secret` writes
    // wherever the store already holds something, so a mend over a live entry
    // would put the typed password straight back.
    expect(firstRan('forget_server_secret')).toBeLessThan(firstRan('reconnect_volume_with_credentials'))
    expect(ipc.callCount('save_sftp_credentials')).toBe(0)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await seam
  })

  it('files the typed secret when the box goes ON over an empty store, so the mend can refresh it', async () => {
    ipc.mock('save_sftp_credentials', () => null)
    ipc.mock('reconnect_volume_with_credentials', () => null)
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    const request = await parkedRequest()

    await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: true },
      username: null,
    })
    // The whole tuple the volume id is minted from, so the entry this writes is
    // the one the next dial reads.
    expect(ipc.lastCall('save_sftp_credentials')?.payload).toMatchObject({
      host: 'nas.local',
      port: 22,
      username: 'ada',
      secret: 'hunter2',
    })
    expect(firstRan('save_sftp_credentials')).toBeLessThan(firstRan('reconnect_volume_with_credentials'))
    expect(ipc.callCount('forget_server_secret')).toBe(0)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await seam
  })

  it('files an S3 place’s secret as its ACCOUNT’s, under the provider its saved entry knows', async () => {
    // ❗ The listing carries neither the preset nor its fields, so the writer reads the
    // saved place (`get_known_s3_places`), matched to this volume by the id it publishes.
    const s3Id = 's3-s3-eu-west-1-amazonaws-com-443-akia-photos'
    const provider = { kind: 'aws', region: 'eu-west-1' }
    ipc.mock('list_saved_servers', () => [
      {
        ...SAVED_SERVER,
        id: 's3-root',
        protocol: 's3',
        address: 's3.eu-west-1.amazonaws.com',
        username: 'AKIA',
        places: [
          {
            ...SAVED_SERVER.places[0],
            volumeId: s3Id,
            name: 'photos',
            appRoot: 's3://AKIA@s3.eu-west-1.amazonaws.com:443/photos',
          },
        ],
      },
    ])
    ipc.mock('get_known_s3_places', () => [
      {
        volumeId: s3Id,
        provider,
        accessKeyId: 'AKIA',
        bucket: 'photos',
        displayName: '',
        autoReconnect: true,
        pinned: true,
      },
    ])
    ipc.mock('save_s3_credentials', () => null)
    ipc.mock('reconnect_volume_with_credentials', () => null)
    const seam = openSignInForPlace({ volumeId: s3Id, registered: true })
    const request = await parkedRequest()
    if (request.mode !== 'sign-in') throw new Error('unreachable')
    expect(request.endpoint).toMatchObject({ address: 's3.eu-west-1.amazonaws.com/photos', username: 'AKIA' })

    await attemptOf(request)({ mode: 'sign-in', secret: { secret: 's3cr3t', remember: true }, username: null })
    expect(ipc.lastCall('save_s3_credentials')?.payload).toMatchObject({
      provider,
      accessKeyId: 'AKIA',
      secret: 's3cr3t',
    })
    expect(firstRan('save_s3_credentials')).toBeLessThan(firstRan('reconnect_volume_with_credentials'))

    closeSignInSheet({ kind: 'connected', volumeId: s3Id })
    await seam
  })

  it('files the secret for an SFTP account whose username is an email address', async () => {
    // ❗ The account is read off the place's `appRoot`, which Rust mints with the
    // username raw. A path that doesn't parse leaves SFTP without a port, so such
    // a place got no writer: the box went ON and nothing was ever filed.
    const emailId = 'sftp-nas-local-22-ada-corp'
    ipc.mock('list_saved_servers', () => [
      {
        ...SAVED_SERVER,
        id: emailId,
        username: 'ada@corp.example',
        places: [
          { ...SAVED_SERVER.places[0], volumeId: emailId, appRoot: 'sftp://ada@corp.example@nas.local:22/srv/data' },
        ],
      },
    ])
    ipc.mock('save_sftp_credentials', () => null)
    ipc.mock('reconnect_volume_with_credentials', () => null)
    const seam = openSignInForPlace({ volumeId: emailId, registered: true })
    const request = await parkedRequest()
    if (request.mode !== 'sign-in') throw new Error('unreachable')
    expect(request.endpoint).toMatchObject({ host: 'nas.local', username: 'ada@corp.example' })

    await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: true },
      username: null,
    })
    expect(ipc.lastCall('save_sftp_credentials')?.payload).toMatchObject({
      host: 'nas.local',
      port: 22,
      username: 'ada@corp.example',
      secret: 'hunter2',
    })

    closeSignInSheet({ kind: 'connected', volumeId: emailId })
    await seam
  })

  it('says the Keychain refused, and dials nothing, when the box goes ON and the password will not store', async () => {
    // ❗ The flip lands before the round, so a store that refuses it (Deny on the
    // Keychain prompt, a locked keychain, no secret service) must answer as a
    // refusal: thrown out of the attempt, it left the sheet stuck on busy.
    ipc.mock('save_sftp_credentials', () => {
      throw { type: 'access_denied', message: 'User canceled the operation.' }
    })
    ipc.mock('reconnect_volume_with_credentials', () => null)
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    const request = await parkedRequest()
    const attempt = attemptOf(request)
    const offer = { mode: 'sign-in' as const, secret: { secret: 'hunter2', remember: true }, username: null }

    expect(await attempt(offer)).toEqual({ kind: 'refused', refusal: 'secret_not_stored' })
    // A box the store refused isn't what the user asked for, so the round waits.
    expect(ipc.callCount('reconnect_volume_with_credentials')).toBe(0)
    expect(JSON.stringify(warn.mock.calls)).not.toContain('hunter2')

    // Nothing was filed, so the next press tries the write again.
    ipc.mock('save_sftp_credentials', () => null)
    expect(await attempt(offer)).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.callCount('save_sftp_credentials')).toBe(2)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await seam
  })

  it('writes nothing at all when the box is left where it started', async () => {
    ipc.mock('has_server_secret', () => true)
    ipc.mock('reconnect_volume_with_credentials', () => null)
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: true })
    const request = await parkedRequest()

    await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: true },
      username: null,
    })
    expect(ipc.callCount('save_sftp_credentials')).toBe(0)
    expect(ipc.callCount('forget_server_secret')).toBe(0)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await seam
  })

  it('writes the flip once, however many rounds the sheet takes', async () => {
    ipc.mock('has_server_secret', () => true)
    ipc.mock('forget_server_secret', () => true)
    const answers = [{ outcome: 'authentication_rejected' }, { outcome: 'connected', volumeId: VOLUME_ID }]
    ipc.mock('connect_saved_place', () => answers.shift() ?? { outcome: 'unreachable' })
    const seam = openSignInForPlace({ volumeId: VOLUME_ID, registered: false })
    const request = await parkedRequest()
    const attempt = attemptOf(request)

    await attempt({ mode: 'sign-in', secret: { secret: 'nope', remember: false }, username: null })
    await attempt({ mode: 'sign-in', secret: { secret: 'hunter2', remember: false }, username: null })
    // The store is off after the first round, so the second has nothing to flip.
    expect(ipc.callCount('forget_server_secret')).toBe(1)

    closeSignInSheet({ kind: 'connected', volumeId: VOLUME_ID })
    await seam
  })
})

/**
 * A saved SMB share is its account (`docs/specs/saved-smb-shares.md`): the sheet
 * asks for THAT account's password, and SMB's Keychain is never probed to seed
 * the box, since each read can raise a system prompt.
 */
describe('openSignInForPlace: a saved SMB share', () => {
  const SHARE_ID = 'smb-192-168-0-153-445-container'
  const SMB_HOST = {
    id: 'manual-192-168-0-153-445',
    protocol: 'smb',
    displayName: "Sven's NAS",
    nameSource: 'user',
    address: '192.168.0.153',
    username: 'sven',
    pinned: false,
    lastConnectedAt: null,
    autoReconnect: null,
    places: [
      {
        volumeId: SHARE_ID,
        name: 'Container',
        pinned: true,
        connected: false,
        appRoot: '/Volumes/Container',
        username: 'sven',
      },
    ],
  }

  it('asks for the share account’s password, remembering by default, without asking the Keychain', async () => {
    ipc.mock('list_saved_servers', () => [SMB_HOST])
    ipc.mock('connect_saved_place', () => ({ outcome: 'connected', volumeId: SHARE_ID }))
    const sheet = openSignInForPlace({ volumeId: SHARE_ID, registered: false })
    const request = await parkedRequest()
    if (request.mode !== 'sign-in') throw new Error('expected the sign-in sheet')

    expect(request.endpoint).toMatchObject({
      protocol: 'smb',
      displayName: 'Container',
      address: 'smb://192.168.0.153/Container',
      host: '192.168.0.153',
      username: 'sven',
    })
    expect(request.remembered).toBe(true)
    expect(ipc.callCount('has_server_secret')).toBe(0)

    const outcome = await attemptOf(request)({
      mode: 'sign-in',
      secret: { secret: 'pw', remember: true },
      username: 'sven',
    })
    expect(outcome).toEqual({ kind: 'connected', volumeId: SHARE_ID })
    expect(ipc.lastCall('connect_saved_place')?.payload).toMatchObject({
      volumeId: SHARE_ID,
      secret: { secret: 'pw', remember: true },
      username: 'sven',
    })

    closeSignInSheet({ kind: 'connected', volumeId: SHARE_ID })
    await sheet
  })
})

/**
 * cmdr-reports#6: the add sheet says what it does. "Add and open" takes the pane
 * to the new server; "Add" only saves it, after the same check; "Add anyway"
 * saves a server the check couldn't reach, and says nothing was checked.
 */
describe('add mode: Add, Add and open, and Add anyway', () => {
  const SFTP_TARGET = {
    protocol: 'sftp' as const,
    displayName: '',
    host: 'nas.local',
    port: 22,
    username: 'ada',
    remoteRoot: '/',
    startFolder: null,
    keyFile: null,
    useAgent: true,
    autoReconnect: true,
  }

  it('saves an SMB host after the reachability check and opens nothing', async () => {
    ipc.mock('connect_to_server', () => ({ host: { id: 'manual-nas-445', name: 'nas' }, sharePath: null }))
    const handOffs: unknown[] = []
    const sheet = openAddServerSheet({ onSmbHandOff: (h) => handOffs.push(h), onConnected: () => {} })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'add_smb',
      address: 'smb://nas',
      name: '',
      username: null,
      intent: 'save',
    })

    expect(outcome).toEqual({ kind: 'added', serverId: 'manual-nas-445' })
    expect(ipc.lastCall('connect_to_server')?.payload).toMatchObject({ checkReachability: true })
    expect(handOffs).toEqual([])
    closeSignInSheet({ kind: 'added', serverId: 'manual-nas-445' })
    await sheet
  })

  it('saves an SMB host without probing it when the person said Add anyway', async () => {
    ipc.mock('connect_to_server', () => ({ host: { id: 'manual-nas-445', name: 'nas' }, sharePath: null }))
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()

    await attemptOf(request)({
      mode: 'add_smb',
      address: 'smb://nas',
      name: '',
      username: null,
      intent: 'save_unchecked',
    })

    expect(ipc.lastCall('connect_to_server')?.payload).toMatchObject({ checkReachability: false })
    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })

  it('tells a server that did not answer from an address that does not parse', async () => {
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()
    const smb = { mode: 'add_smb' as const, address: 'smb://nas', name: '', username: null, intent: 'open' as const }

    ipc.mock('connect_to_server', () => {
      throw { type: 'unreachable', message: "Couldn't reach nas:445" }
    })
    expect(await attemptOf(request)(smb)).toEqual({ kind: 'refused', refusal: 'unreachable' })

    // ERR-XGS9X: this Mac refused the route to a LAN address, which a stuck Local
    // Network permission does too. Still `unreachable` (Add anyway stays), plus the hint.
    ipc.mock('connect_to_server', () => {
      throw { type: 'unreachable', message: "Couldn't reach nas:445", hint: 'local_network_permission' }
    })
    expect(await attemptOf(request)(smb)).toEqual({
      kind: 'refused',
      refusal: 'unreachable',
      hint: 'local_network_permission',
    })

    ipc.mock('connect_to_server', () => {
      throw { type: 'invalid_address', message: 'Enter a server address' }
    })
    expect(await attemptOf(request)(smb)).toEqual({ kind: 'refused', refusal: 'invalid_url' })

    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })

  it('connects an SFTP server to check it, then only saves it: no pane moves', async () => {
    ipc.mock('connect_server', () => ({ outcome: 'connected', volumeId: VOLUME_ID }))
    const landed: unknown[] = []
    const added: unknown[] = []
    const sheet = openAddServerSheet({
      onSmbHandOff: () => {},
      onConnected: (place) => landed.push(place),
      onAdded: (server) => added.push(server),
    })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({ mode: 'add', target: SFTP_TARGET, secret: null, intent: 'save' })
    expect(outcome).toEqual({ kind: 'added', serverId: VOLUME_ID })

    closeSignInSheet({ kind: 'added', serverId: VOLUME_ID })
    await sheet
    expect(landed).toEqual([])
    expect(added).toEqual([{ serverId: VOLUME_ID, name: 'Naspolya' }])
  })

  /**
   * ❗ "Add" only SAVES: the session its check opened is dropped, so the row reads
   * Saved rather than Connected. Only "Add and open" keeps one.
   */
  it('drops the session the check opened once an Add saved the server', async () => {
    ipc.mock('connect_server', () => ({ outcome: 'connected', volumeId: VOLUME_ID }))
    ipc.mock('disconnect_place', () => true)
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {}, onAdded: () => {} })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({ mode: 'add', target: SFTP_TARGET, secret: null, intent: 'save' })

    expect(outcome).toEqual({ kind: 'added', serverId: VOLUME_ID })
    expect(ipc.lastCall('disconnect_place')?.payload).toEqual({ volumeId: VOLUME_ID })
    closeSignInSheet({ kind: 'added', serverId: VOLUME_ID })
    await sheet
  })

  it('keeps the session of an Add and open, which is the one that opens', async () => {
    ipc.mock('connect_server', () => ({ outcome: 'connected', volumeId: VOLUME_ID }))
    ipc.mock('disconnect_place', () => true)
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()

    await attemptOf(request)({ mode: 'add', target: SFTP_TARGET, secret: null, intent: 'open' })

    expect(ipc.callCount('disconnect_place')).toBe(0)
    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })

  /** A server already connected before the Add keeps its session: a pane may be standing on it. */
  it('leaves a session that was already live before the Add alone', async () => {
    liveVolumes = [{ id: VOLUME_ID, connectionState: 'direct' }]
    ipc.mock('connect_server', () => ({ outcome: 'connected', volumeId: VOLUME_ID }))
    ipc.mock('disconnect_place', () => true)
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {}, onAdded: () => {} })
    const request = await parkedRequest()

    await attemptOf(request)({ mode: 'add', target: SFTP_TARGET, secret: null, intent: 'save' })

    expect(ipc.callCount('disconnect_place')).toBe(0)
    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })

  it('saves an unreachable SFTP server without connecting when the person said Add anyway', async () => {
    ipc.mock('update_saved_server', () => ({ outcome: 'saved' }))
    ipc.mock('saved_server_id', () => VOLUME_ID)
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'add',
      target: { ...SFTP_TARGET, host: 'nas.local' },
      secret: null,
      intent: 'save_unchecked',
    })

    expect(ipc.callCount('connect_server')).toBe(0)
    expect(ipc.lastCall('update_saved_server')?.payload).toMatchObject({ server: { host: 'nas.local' } })
    expect(outcome).toEqual({ kind: 'added', serverId: VOLUME_ID })
    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })

  /**
   * ❗ **Add anyway on a WebDAV URL closes on the server it saved.** The store
   * normalizes the URL (`…/dav` → `…/dav/`), so finding the new row by comparing
   * the typed URL with the listed one missed it, and the sheet stayed open saying
   * "didn't answer in time, so nothing was saved" over a saved, pinned row (QA
   * round 2, M6). The id comes from the backend's own id funnel instead.
   */
  it('finds the WebDAV server Add anyway saved, whatever spelling the store keeps', async () => {
    const id = 'webdav-10-255-255-1-443-ada'
    ipc.mock('update_saved_server', () => ({ outcome: 'saved' }))
    ipc.mock('saved_server_id', () => id)
    ipc.mock('list_saved_servers', () => [
      { ...SAVED_SERVER, id, protocol: 'webdav', address: 'https://10.255.255.1/dav/', places: [] },
    ])
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'add',
      target: {
        protocol: 'webdav' as const,
        displayName: '',
        url: 'https://10.255.255.1/dav',
        username: 'ada',
        remoteRoot: '/',
        startFolder: null,
        autoReconnect: true,
      },
      secret: null,
      intent: 'save_unchecked',
    })

    expect(outcome).toEqual({ kind: 'added', serverId: id })
    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })

  /**
   * An empty password on a WebDAV add is caught before any server is asked (the
   * store has none for the account either), so it's worded as the field it is,
   * ❌ never "This server asks for a password", which reads as a server's answer
   * (QA round 2, M6).
   */
  it('words a WebDAV add with no password as the empty field, not as a server answer', async () => {
    ipc.mock('connect_server', () => ({ outcome: 'needs_credentials' }))
    const sheet = openAddServerSheet({ onSmbHandOff: () => {}, onConnected: () => {} })
    const request = await parkedRequest()

    const outcome = await attemptOf(request)({
      mode: 'add',
      target: {
        protocol: 'webdav' as const,
        displayName: '',
        url: 'https://nas.local/dav',
        username: 'ada',
        remoteRoot: '/',
        startFolder: null,
        autoReconnect: true,
      },
      secret: null,
      intent: 'open',
    })

    expect(outcome).toEqual({ kind: 'refused', refusal: 'password_missing' })
    closeSignInSheet({ kind: 'cancelled' })
    await sheet
  })
})
