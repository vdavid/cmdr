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
  asSavedPlaceRefusal,
  connectSavedPlace,
  connectServer,
  connectToServer,
  disconnectPlace,
  forgetServerSecret,
  getVolumeSignInState,
  hasServerSecret,
  knownS3PlaceOf,
  listSavedServers,
  newServerAttemptId,
  reconnectVolumeWithCredentials,
  saveS3Credentials,
  saveSftpCredentials,
  saveWebdavCredentials,
  savedServerId,
  updateSavedServer,
  type SavedServer,
  type SecretOffer,
  type ServerConnectOutcome,
  type ServerTarget,
} from '$lib/tauri-commands'
import { asReconnectError } from '$lib/file-explorer/network/reconnect-error'
import { forgetShareListsOfMachine } from '$lib/file-explorer/network/network-store.svelte'
import { isLiveSession } from '$lib/file-explorer/navigation/connection-state'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import type { NetworkHost } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'
import type { SignInSeamRequest, SignInSeamResult } from './connect-flow'
import type { ConnectRefusalKind } from './connect-refusals'
import { parseServerAddress } from './address-parser'
import { parseServerPath, serverProtocolOfVolumeId } from './server-path-utils'
import type {
  SignInAttempt,
  SignInAttemptOutcome,
  SignInEndpoint,
  SignInSheetResult,
  SignInSubmission,
} from './sign-in-contract'
import { readConnectOutcome, readSavedServerOutcome } from './server-outcomes'
import { openSignInSheet } from './sign-in-sheet-state.svelte'
import { saveTargetSecret } from './saved-server-io'
import { asAddServerError } from './add-server-error'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'

const log = getAppLogger('servers')

/** The SMB host the sheet handed over, for the caller to open. */
export interface SmbHandOff {
  /** The host the address named, injected as a manual server by `connectToServer`. */
  host: NetworkHost
  /** The share the address named, when it named one. */
  sharePath: string | null
}

/** A server an add saved without opening ("Add", "Add anyway"). */
export interface AddedServer {
  /** Its saved id: the id its row in the Servers list carries. */
  serverId: string
  /** What the list calls it. */
  name: string
}

/** The place an add just connected to, for the caller to put a pane on. */
export interface ConnectedPlace {
  volumeId: string
  /** The place's app root. A pane handed the root lands on its start folder. */
  root: string
}

/**
 * Opens add mode. `prefill` is an address the caller already has (a pasted link,
 * a go-to-path input), as the user spelled it.
 *
 * `onSmbHandOff` is what SMB's add path lands in: its connect is a share MOUNT
 * rather than a session, so `connectToServer` injects a manual host and the
 * caller opens its places list.
 *
 * `onConnected` is where an SFTP or WebDAV "Add and open" lands: the place it
 * just connected to. ❗ Every door calls it, ❌ never leaves it out: a sheet that
 * closes on a live server while every pane stays put reads as a Connect that did
 * nothing. The caller picks the pane (the focused one, or the hub's own).
 *
 * `onAdded` is where an "Add" (or "Add anyway") lands: saved, with no pane moved.
 * The hub selects the new row; a door with no list on screen leaves it out, and
 * a toast says where the server went instead, so the add is never invisible.
 */
export async function openAddServerSheet(options: {
  prefill?: string
  onSmbHandOff: (handOff: SmbHandOff) => void
  onConnected: (place: ConnectedPlace) => void
  onAdded?: (server: AddedServer) => void
}): Promise<SignInSheetResult> {
  const result = await openSignInSheet({
    mode: 'add',
    prefill: options.prefill,
    attempt: (submission) => attemptAdd(submission, options.onSmbHandOff),
  })
  if (result.kind === 'connected') {
    const root = await placeRootOf(result.volumeId)
    if (root) options.onConnected({ volumeId: result.volumeId, root })
    else log.warn('The place {volumeId} connected, but no saved server lists it', { volumeId: result.volumeId })
  }
  if (result.kind === 'added') {
    const added = { serverId: result.serverId, name: await savedNameOf(result.serverId) }
    if (options.onAdded) options.onAdded(added)
    else addToast(tString('servers.sheet.addedToast', { name: added.name }), { level: 'success' })
  }
  return result
}

/** What the Servers list calls the saved server `serverId`, or the id when nothing lists it. */
async function savedNameOf(serverId: string): Promise<string> {
  try {
    const servers = await listSavedServers()
    return servers.find((server) => server.id === serverId)?.displayName ?? serverId
  } catch {
    return serverId
  }
}

/**
 * A place's app root, off the saved list.
 *
 * ❗ The backend's list, ❌ not the volume store: the connect registered the
 * volume a moment ago, and `volumes-changed` is debounced (and waits on local
 * mount discovery), so the store can still hold nothing for it, or a row from an
 * earlier registration with an old root.
 */
export async function placeRootOf(volumeId: string): Promise<string | null> {
  const servers = await listSavedServers()
  for (const server of servers) {
    const place = server.places.find((p) => p.volumeId === volumeId)
    if (place) return place.appRoot
  }
  return null
}

/**
 * Opens edit mode on a saved server. Save writes; nothing dials. `placeVolumeId` picks the
 * place for a server whose places are each saved on their own (an S3 account's buckets).
 */
export async function openEditServerSheet(server: SavedServer, placeVolumeId?: string): Promise<SignInSheetResult> {
  return await openSignInSheet({ mode: 'edit', server, placeVolumeId })
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
  // seed one the user already declined. SMB is the exception, and asks nothing:
  // every read of its Keychain can raise a system prompt, and the backend writes
  // a password only once a mount went through (`sign-in-contract.ts`).
  const remembered = endpoint.protocol === 'smb' ? true : await hasServerSecret(volumeId)

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
        try {
          await options.identity.saveSecret(submission.secret.secret)
        } catch (e) {
          // ❗ A store that refuses (Deny on the Keychain prompt, a locked
          // keychain, no secret service) is an ANSWER, ❌ never a throw out of the
          // attempt, which left the sheet stuck on busy. Nothing was filed, so
          // `stored` stays put and the next press writes again, and no round runs
          // on a box the store just refused. The line carries the store's words,
          // ❌ never the secret.
          log.warn('The Keychain refused the password for {volumeId}: {error}', {
            volumeId: options.volumeId,
            error: String(e),
          })
          return { kind: 'refused', refusal: 'secret_not_stored' }
        }
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

/**
 * Add mode's attempt: a brand-new server, or SMB's hand-off.
 *
 * ❗ Every intent but "Add anyway" checks the server before anything is saved
 * (cmdr-reports#6): a TCP probe for SMB, the real connect for SFTP and WebDAV,
 * because a typo saved silently is found only later, somewhere else. Only
 * "Add and open" moves a pane.
 */
async function attemptAdd(
  submission: SignInSubmission,
  onSmbHandOff: (handOff: SmbHandOff) => void,
): Promise<SignInAttemptOutcome> {
  if (submission.mode === 'add_smb') return await attemptAddSmb(submission, onSmbHandOff)
  if (submission.mode !== 'add') return { kind: 'refused', refusal: 'needs_credentials' }

  if (submission.intent === 'save_unchecked') return await saveUnchecked(submission.target, submission.secret)
  // Read BEFORE the check dials: a place that was already live keeps its session.
  const liveBefore = new Set(
    getVolumes()
      .filter((volume) => isLiveSession(volume.connectionState))
      .map((volume) => volume.id),
  )
  const attemptId = newServerAttemptId()
  const outcome = readConnectOutcome(await connectServer(submission.target, attemptId, submission.secret))
  // A WebDAV or S3 connect asks the store before it dials, so "needs credentials"
  // with an empty field is the field, not the server: word it as such.
  if (
    outcome.kind === 'refused' &&
    outcome.refusal === 'needs_credentials' &&
    (submission.target.protocol === 'webdav' || submission.target.protocol === 's3') &&
    !submission.secret?.secret
  ) {
    return { kind: 'refused', refusal: 'password_missing' }
  }
  // "Add": the connect proved the server and saved it, and ❗ "Add" only saves, so
  // the session the check opened goes and the row reads Saved. A remembered
  // password stays filed. A place that was live before the Add keeps its session,
  // since a pane may be standing on it.
  if (outcome.kind === 'connected' && submission.intent === 'save') {
    if (!liveBefore.has(outcome.volumeId)) await dropCheckSession(outcome.volumeId)
    return { kind: 'added', serverId: outcome.volumeId }
  }
  return outcome
}

/** Drops the session an "Add" check opened. One that won't go leaves the server Connected, which is only untidy. */
async function dropCheckSession(volumeId: string): Promise<void> {
  try {
    await disconnectPlace(volumeId)
  } catch (e) {
    log.warn('Dropping the session the Add check opened for {volumeId} broke down: {error}', {
      volumeId,
      error: String(e),
    })
  }
}

/** SMB's add: a TCP probe (unless "Add anyway"), then a hand-off or just the save. */
async function attemptAddSmb(
  submission: Extract<SignInSubmission, { mode: 'add_smb' }>,
  onSmbHandOff: (handOff: SmbHandOff) => void,
): Promise<SignInAttemptOutcome> {
  try {
    const result = await connectToServer(
      submission.address,
      submission.name,
      submission.username,
      submission.intent !== 'save_unchecked',
    )
    // A list fetched before this add may be the guest one a typed account now
    // refuses, so the places list asks again.
    forgetShareListsOfMachine(result.host)
    if (submission.intent !== 'open') return { kind: 'added', serverId: result.host.id }
    onSmbHandOff({ host: result.host, sharePath: result.sharePath })
    return { kind: 'handed_off' }
  } catch (e) {
    // ❗ Two answers, and only one can be added anyway: an address that doesn't
    // parse is a typo, not a server that's asleep.
    const typed = asAddServerError(e)
    if (typed?.type === 'invalid_address') return { kind: 'refused', refusal: 'invalid_url' }
    if (typed) return { kind: 'refused', refusal: 'unreachable', hint: typed.hint ?? undefined }
    // The host, ❌ never the typed address: `smb://user:password@host` is a
    // spelling people paste, and this line reaches error-report bundles.
    const parsed = parseServerAddress(submission.address)
    log.warn('Adding the SMB host {host} broke down: {error}', {
      host: parsed.kind === 'parsed' ? parsed.host : 'an address that does not parse',
      error: String(e),
    })
    return { kind: 'refused', refusal: 'unreachable' }
  }
}

/**
 * "Add anyway" for SFTP and WebDAV: saves the server as typed, connecting to
 * nothing, and files a typed password when Remember is on.
 */
async function saveUnchecked(target: ServerTarget, secret: SecretOffer | null): Promise<SignInAttemptOutcome> {
  let saved
  try {
    saved = readSavedServerOutcome(await updateSavedServer(target, null))
  } catch (e) {
    log.warn('Saving the unchecked server broke down: {error}', { error: String(e) })
    return { kind: 'refused', refusal: 'save_unconfirmed' }
  }
  if (saved.kind === 'refused') return saved
  if (secret?.remember) {
    try {
      await saveTargetSecret(target, secret.secret)
    } catch (e) {
      log.warn('The server saved, but its password did not: {error}', { error: String(e) })
      return { kind: 'refused', refusal: 'saved_secret_not_updated' }
    }
  }
  const serverId = await savedIdOf(target)
  return serverId ? { kind: 'added', serverId } : { kind: 'refused', refusal: 'save_unconfirmed' }
}

/**
 * The saved id of the server `target` names, once the listing has it.
 *
 * ❗ The id comes from the backend, which is the only side that mints one (a
 * Rust-side hash of the account's tuple), and the listing only confirms it's
 * there. ❌ Never matched on the typed address: the stores normalize it (a WebDAV
 * URL gains its trailing slash), so "Add anyway" reported a saved server as not
 * saved and the sheet stayed open over it.
 */
async function savedIdOf(target: ServerTarget): Promise<string | null> {
  const id = await savedServerId(target)
  if (id === null) return null
  const servers = await listSavedServers()
  return servers.some((server) => server.id === id) ? id : null
}

/** An absent place: the first dial, now carrying whatever the user typed. */
function dialSavedPlaceAttempt(volumeId: string): SignInAttempt {
  return async (submission) => {
    if (submission.mode !== 'sign-in') return { kind: 'refused', refusal: 'needs_credentials' }
    const attemptId = newServerAttemptId()
    try {
      return readConnectOutcome(await connectSavedPlace(volumeId, attemptId, submission.secret, submission.username))
    } catch (e) {
      // ❗ A typed refusal means the place's standing moved while the sheet was
      // open (`volumes-changed` is debounced). Another dial registered it, so the
      // sheet closes onto the live place rather than dialing a second volume.
      const refusal = asSavedPlaceRefusal(e)
      if (refusal?.reason === 'already_connected') return { kind: 'connected', volumeId }
      // Nothing saved answers for it. ❌ Not a silent close: a closed sheet hands
      // the pane nothing to say, and a row that outlives the refusal stays blank.
      if (refusal) {
        log.info('Nothing saved answers for the place {volumeId}', { volumeId })
      } else {
        log.warn('Dialing the saved place {volumeId} broke down: {error}', { volumeId, error: String(e) })
      }
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
    address: headerAddressOf(owner, place, parsed),
    host: parsed?.host ?? owner.address,
    // ❗ The PLACE's account first: an SMB host's shares each remember their own.
    username: parsed?.username ?? place?.username ?? owner.username ?? undefined,
  }
  const saveSecret = owner.protocol === 's3' ? await s3SecretWriterFor(volumeId) : secretWriterFor(owner, parsed)
  return { endpoint, saveSecret }
}

/**
 * An S3 place files its ACCOUNT's secret, keyed on the provider choice plus the
 * access key id, which only the saved entry knows (the listing carries neither the
 * preset nor its fields). `null` when no saved place answers for the id.
 */
async function s3SecretWriterFor(volumeId: string): Promise<((secret: string) => Promise<void>) | null> {
  const place = await knownS3PlaceOf(volumeId)
  if (!place) return null
  return async (secret) => {
    await saveS3Credentials(place.provider, place.accessKeyId, secret)
  }
}

/**
 * The sheet's header line for a place. An SMB share names the share on its host, and
 * an S3 bucket the bucket on its endpoint: in both, the place IS that share or bucket.
 * Every other place is its server's address.
 */
function headerAddressOf(
  owner: SavedServer,
  place: SavedServer['places'][number] | undefined,
  parsed: ReturnType<typeof parseServerPath>,
): string {
  if (owner.protocol === 'smb' && place) return `smb://${owner.address}/${place.name}`
  const bucket = parsed?.protocol === 's3' ? parsed.path.split('/')[0] : ''
  return bucket ? `${owner.address}/${bucket}` : owner.address
}

/**
 * How each protocol files a secret: SFTP keys on `(host, port, username)`, the
 * same tuple its volume id hashes; WebDAV keys on the base URL and the account.
 * An SMB share has none here: `connect_saved_place` writes the password itself,
 * once the mount went through.
 *
 * A path that didn't parse leaves SFTP without a port, and a made-up one would
 * write an entry nothing reads, so it answers `null` instead.
 */
function secretWriterFor(
  owner: SavedServer,
  parsed: ReturnType<typeof parseServerPath>,
): ((secret: string) => Promise<void>) | null {
  // `null` is an SMB host, which is not an account yet and files no secret here.
  const username = parsed?.username ?? owner.username
  if (username === null) return null
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
 *
 * ❗ The PROTOCOL is still read off the id's own prefix, ❌ never defaulted to
 * SFTP: `cmdr_fs::volume::ids` mints that prefix, so it is a real answer, and a
 * WebDAV place that fell out of the listing described as SFTP puts the wrong word
 * in the header and in the refusal under it. An id that names neither is not a
 * server place at all, and SFTP is the honest guess for a shape nothing else
 * explains.
 */
function unknownEndpoint(volumeId: string): SignInEndpoint {
  return {
    protocol: serverProtocolOfVolumeId(volumeId) ?? 'sftp',
    displayName: volumeId,
    address: volumeId,
    host: volumeId,
  }
}
