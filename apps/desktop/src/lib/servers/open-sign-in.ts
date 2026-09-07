/**
 * The three ways the sign-in sheet opens, each carrying the attempt its own
 * caller owns.
 *
 * ❗ **This is where the protocol lives, and the sheet is where the form lives.**
 * The sheet never dials; it calls an `attempt` as many times as the user retries.
 * So every command choice that depends on a place's STANDING is made here:
 * a registered volume is mended with `reconnectVolumeWithCredentials`, an absent
 * one is dialed with `connectSavedPlace`, and a brand-new server goes through
 * `connectServer`. Picking one wrong is silent — re-dialing a registered volume
 * registers a SECOND one under a second id.
 *
 * It sits between `connect-flow.ts` (which decides WHEN a human is needed) and
 * `sign-in-sheet-state.svelte.ts` (which owns the one mounted sheet), so neither
 * of those has to know about the other.
 */

import {
  connectSavedPlace,
  connectServer,
  connectToServer,
  forgetServerSecret,
  getVolumeSignInState,
  hasServerSecret,
  listSavedServers,
  newServerAttemptId,
  reconnectVolumeWithCredentials,
  saveSftpCredentials,
  saveWebdavCredentials,
  type SavedServer,
} from '$lib/tauri-commands'
import { asReconnectError } from '$lib/file-explorer/network/reconnect-error'
import type { NetworkHost } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'
import type { SignInSeamRequest, SignInSeamResult } from './connect-flow'
import type { ConnectRefusalKind } from './connect-refusals'
import { parseServerPath } from './server-path-utils'
import type {
  SignInAttempt,
  SignInAttemptOutcome,
  SignInEndpoint,
  SignInSheetResult,
  SignInSubmission,
} from './sign-in-contract'
import { readConnectOutcome } from './server-outcomes'
import { openSignInSheet } from './sign-in-sheet-state.svelte'

const log = getAppLogger('servers')

/** The SMB host the sheet handed over, for the caller to open. */
export interface SmbHandOff {
  /** The host the address named, injected as a manual server by `connectToServer`. */
  host: NetworkHost
  /** The share the address named, when it named one. */
  sharePath: string | null
}

/**
 * Opens add mode. `prefill` is an address the caller already has (a pasted link,
 * a go-to-path input), as the user spelled it.
 *
 * `onSmbHandOff` is what SMB's add path lands in: its connect is a share MOUNT
 * rather than a session, so `connectToServer` injects a manual host and the
 * caller opens its places list.
 */
export async function openAddServerSheet(options: {
  prefill?: string
  onSmbHandOff: (handOff: SmbHandOff) => void
}): Promise<SignInSheetResult> {
  return await openSignInSheet({
    mode: 'add',
    prefill: options.prefill,
    attempt: (submission) => attemptAdd(submission, options.onSmbHandOff),
  })
}

/** Opens edit mode on a saved server. Save writes; nothing dials. */
export async function openEditServerSheet(server: SavedServer): Promise<SignInSheetResult> {
  return await openSignInSheet({ mode: 'edit', server })
}

/**
 * `connect-flow.ts`'s seam: the sheet, opened for a place that is asking.
 *
 * ❗ Asks the backend what to show (`getVolumeSignInState`) when the sheet
 * renders, ❌ never derives it from a protocol, a rung, or a connect result: a
 * backend that authenticates per connection can prove itself differently each
 * dial, so an answer kept from earlier describes a session that may be gone.
 */
export async function openSignInForPlace(request: SignInSeamRequest): Promise<SignInSeamResult> {
  const { volumeId, registered, firstOutcome } = request
  const shape = await getVolumeSignInState(volumeId)
  if (shape.kind === 'nothing') {
    // ❗ The guard lives HERE rather than in each caller: `nothing` means no
    // secret a person could type would help (a key-only or agent-only server,
    // whose `reconnect_with_credentials` answers `NotSupported` every time), and
    // `SignInCredentialFields` would otherwise fall through to a password box
    // over it. The pane's `signed_out` banner with no button is the honest view,
    // and answering "not signed in" is what leaves it standing.
    log.info('No sheet for {volumeId}: nothing a person could type would help', { volumeId })
    return { signedIn: false }
  }
  const identity = await identityFor(volumeId)
  const { endpoint } = identity
  // ❗ Seeded from what is STORED, ❌ never defaulted on: an attended sign-in
  // REFRESHES a remembered secret and never seeds one, so a default-on box would
  // seed one the user already declined.
  const remembered = await hasServerSecret(volumeId)

  const result = await openSignInSheet({
    mode: 'sign-in',
    endpoint,
    shape,
    remembered,
    hostKey: firstOutcome?.outcome === 'needs_host_key_approval' ? firstOutcome : undefined,
    // Why the person is being asked. ❗ Read off the dial that sent them here
    // rather than assumed, so `needs_credentials` (nothing was ever offered) and
    // `authentication_rejected` (something was, and was refused) keep their own
    // sentences. A registered place had no dial to read, so its caller says.
    refusal: refusalFrom(firstOutcome) ?? request.refusal,
    attempt: withRememberFlip({
      volumeId,
      identity,
      remembered,
      attempt: registered ? mendAttempt(volumeId, endpoint) : dialSavedPlaceAttempt(volumeId),
    }),
  })
  return result.kind === 'connected' ? { signedIn: true, volumeId: result.volumeId } : { signedIn: false }
}

/**
 * Writes the Remember box's flip, then runs the round it belongs to.
 *
 * ❗ **The write lands BEFORE the attempt, and it is the whole mechanism.**
 * "Remember" means exactly "the Keychain holds a secret for this account"
 * (`crates/cmdr-sftp/DETAILS.md` § "The two switches"), so:
 *
 * - Turned OFF, the entry goes NOW. Waiting would leave `refresh_remembered_secret`
 *   a live entry to write the typed password back into, which is the dial
 *   changing a switch behind the user's back.
 * - Turned ON over an empty store, the typed secret is filed NOW, so the mend
 *   has something to refresh. An attended sign-in never seeds one, so a box
 *   flipped on with nothing written would promise a thing that never happens.
 *
 * ❗ Once per flip, ❌ not once per round: the local reading moves with the
 * store, so a second retry after a delete has nothing left to do.
 */
function withRememberFlip(options: {
  volumeId: string
  identity: PlaceIdentity
  remembered: boolean
  attempt: SignInAttempt
}): SignInAttempt {
  let stored = options.remembered
  return async (submission) => {
    const wanted = submission.mode === 'sign-in' ? (submission.secret?.remember ?? stored) : stored
    if (wanted !== stored) {
      if (!wanted) {
        await forgetServerSecret(options.volumeId)
        stored = false
      } else if (submission.mode === 'sign-in' && submission.secret && options.identity.saveSecret) {
        await options.identity.saveSecret(submission.secret.secret)
        stored = true
      }
    }
    return await options.attempt(submission)
  }
}

/**
 * The refusal a first dial answered, or `undefined` when it answered something
 * else (a host key to approve, a cancel, a connect that landed).
 *
 * ❗ Folds `server-outcomes.ts`'s reading rather than re-reading the wire enum: a
 * second switch would be a second chance to word one outcome differently.
 */
function refusalFrom(outcome: ServerConnectOutcome | undefined): ConnectRefusalKind | undefined {
  if (!outcome) return undefined
  const read = readConnectOutcome(outcome)
  return read.kind === 'refused' ? read.refusal : undefined
}

/** Add mode's attempt: a brand-new server, or SMB's hand-off. */
async function attemptAdd(
  submission: SignInSubmission,
  onSmbHandOff: (handOff: SmbHandOff) => void,
): Promise<SignInAttemptOutcome> {
  if (submission.mode === 'add_smb') {
    try {
      const result = await connectToServer(submission.address)
      onSmbHandOff({ host: result.host, sharePath: result.sharePath })
      return { kind: 'handed_off' }
    } catch (e) {
      log.warn('Adding the SMB host {address} broke down: {error}', { address: submission.address, error: String(e) })
      return { kind: 'refused', refusal: 'unreachable' }
    }
  }
  if (submission.mode !== 'add') return { kind: 'refused', refusal: 'needs_credentials' }

  const attemptId = newServerAttemptId()
  return readConnectOutcome(await connectServer(submission.target, attemptId, submission.secret))
}

/** An absent place: the first dial, now carrying whatever the user typed. */
function dialSavedPlaceAttempt(volumeId: string): SignInAttempt {
  return async (submission) => {
    if (submission.mode !== 'sign-in') return { kind: 'refused', refusal: 'needs_credentials' }
    const attemptId = newServerAttemptId()
    try {
      return readConnectOutcome(await connectSavedPlace(volumeId, attemptId, submission.secret))
    } catch (e) {
      // A typed `SavedPlaceRefusal`: the wrong move for this volume's standing.
      // A bug to read in a log, ❌ never a sentence to put in front of a person.
      log.warn('Dialing the saved place {volumeId} was refused: {error}', { volumeId, error: String(e) })
      return { kind: 'refused', refusal: 'unreachable' }
    }
  }
}

/**
 * A REGISTERED place whose session wants a credential: mended, ❌ never dialed.
 *
 * ❗ It refreshes a remembered secret and never seeds one — that rule lives in
 * the backend, which writes the store only where it already holds a secret for
 * this account. The Remember box is what the user changes that with, and
 * `withRememberFlip` has already written it by the time this runs.
 */
function mendAttempt(volumeId: string, endpoint: SignInEndpoint): SignInAttempt {
  return async (submission) => {
    if (submission.mode !== 'sign-in') return { kind: 'refused', refusal: 'needs_credentials' }
    // ❗ The username the SHAPE allowed. `null` means the variant renders it
    // read-only, and the account the volume already has is the one to send.
    const username = submission.username ?? endpoint.username ?? ''
    try {
      await reconnectVolumeWithCredentials(volumeId, username, submission.secret?.secret ?? '')
      return { kind: 'connected', volumeId }
    } catch (e) {
      return { kind: 'refused', refusal: mendRefusal(e) }
    }
  }
}

/** A typed reconnect refusal, in the app's own vocabulary. ❌ Never off a message. */
function mendRefusal(error: unknown): ConnectRefusalKind {
  const typed = asReconnectError(error)
  if (!typed || typed.type === 'volumeNotFound') return 'unreachable'
  switch (typed.error.type) {
    case 'permissionDenied':
      return 'authentication_rejected'
    case 'connectionTimeout':
      return 'timed_out'
    case 'notSupported':
      return 'auth_method_unsupported'
    default:
      return 'unreachable'
  }
}

/** Who the sheet says is asking, and where a secret for them is filed. */
interface PlaceIdentity {
  /** The read-only header the sheet shows. */
  endpoint: SignInEndpoint
  /**
   * Files the Keychain entry the next dial reads, or `null` when no saved server
   * claims this place and there is nothing to key one on.
   *
   * ❗ It takes the whole tuple the volume id is minted from, ❌ never a host
   * plus a default port: an entry written under a different key is one the dial
   * never finds, and the box would then be lying in the other direction.
   */
  saveSecret: ((secret: string) => Promise<void>) | null
}

/**
 * The endpoint the sheet shows as its header, and the secret writer beside it.
 *
 * ❗ Read from `listSavedServers()` rather than parsed out of a volume id: the id
 * is a hash minted in Rust from `(host, port, username)`, and nothing on this
 * side can take it apart.
 */
async function identityFor(volumeId: string): Promise<PlaceIdentity> {
  const servers = await listSavedServers()
  const owner = servers.find((server) => server.places.some((place) => place.volumeId === volumeId))
  if (!owner) return { endpoint: unknownEndpoint(volumeId), saveSecret: null }
  const place = owner.places.find((p) => p.volumeId === volumeId)
  const parsed = place ? parseServerPath(place.appRoot) : null
  const endpoint: SignInEndpoint = {
    protocol: owner.protocol,
    displayName: place?.name ?? owner.displayName,
    address: owner.address,
    host: parsed?.host ?? owner.address,
    username: parsed?.username ?? owner.username ?? undefined,
  }
  return { endpoint, saveSecret: secretWriterFor(owner, parsed) }
}

/**
 * How each protocol files a secret: SFTP keys on `(host, port, username)`, the
 * same tuple its volume id hashes; WebDAV keys on the base URL and the account.
 *
 * A path that didn't parse leaves SFTP without a port, and a made-up one would
 * write an entry nothing reads, so it answers `null` instead.
 */
function secretWriterFor(
  owner: SavedServer,
  parsed: ReturnType<typeof parseServerPath>,
): ((secret: string) => Promise<void>) | null {
  const username = parsed?.username ?? owner.username
  if (username === null || username === undefined) return null
  if (owner.protocol === 'sftp') {
    if (!parsed) return null
    const { host, port } = parsed
    return async (secret) => {
      await saveSftpCredentials(host, port, username, secret)
    }
  }
  if (owner.protocol === 'webdav') {
    const url = owner.address
    return async (secret) => {
      await saveWebdavCredentials(url, username, secret)
    }
  }
  return null
}

/**
 * A place no saved server claims. It still has to say WHO is asking, so the id
 * stands in: the sheet's header is honest about knowing nothing more, and the
 * refusal sentences read fine with it.
 */
function unknownEndpoint(volumeId: string): SignInEndpoint {
  return { protocol: 'sftp', displayName: volumeId, address: volumeId, host: volumeId }
}
