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
  getVolumeSignInState,
  listSavedServers,
  newServerAttemptId,
  reconnectVolumeWithCredentials,
  type SavedServer,
  type ServerConnectOutcome,
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
  const endpoint = await endpointFor(volumeId)

  const result = await openSignInSheet({
    mode: 'sign-in',
    volumeId,
    endpoint,
    shape,
    hostKey: firstOutcome?.outcome === 'needs_host_key_approval' ? firstOutcome : undefined,
    attempt: registered ? mendAttempt(volumeId, endpoint) : dialSavedPlaceAttempt(volumeId),
  })
  return result.kind === 'connected' ? { signedIn: true, volumeId: result.volumeId } : { signedIn: false }
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
  return readAttempt(await connectServer(submission.target, attemptId, submission.secret))
}

/** An absent place: the first dial, now carrying whatever the user typed. */
function dialSavedPlaceAttempt(volumeId: string): SignInAttempt {
  return async (submission) => {
    if (submission.mode !== 'sign-in') return { kind: 'refused', refusal: 'needs_credentials' }
    const attemptId = newServerAttemptId()
    try {
      return readAttempt(await connectSavedPlace(volumeId, attemptId, submission.secret))
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
 * this account. The sheet's Remember box is what the user uses to change that,
 * and it writes through `save_*` / `delete_*` explicitly.
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

/**
 * The endpoint the sheet shows as its header.
 *
 * ❗ Read from `listSavedServers()` rather than parsed out of a volume id: the id
 * is a hash minted in Rust from `(host, port, username)`, and nothing on this
 * side can take it apart.
 */
async function endpointFor(volumeId: string): Promise<SignInEndpoint> {
  const servers = await listSavedServers()
  const owner = servers.find((server) => server.places.some((place) => place.volumeId === volumeId))
  if (!owner) return unknownEndpoint(volumeId)
  const place = owner.places.find((p) => p.volumeId === volumeId)
  const parsed = place ? parseServerPath(place.appRoot) : null
  return {
    protocol: owner.protocol,
    displayName: place?.name ?? owner.displayName,
    address: owner.address,
    host: parsed?.host ?? owner.address,
    username: parsed?.username ?? owner.username ?? undefined,
  }
}

/**
 * A place no saved server claims. It still has to say WHO is asking, so the id
 * stands in: the sheet's header is honest about knowing nothing more, and the
 * refusal sentences read fine with it.
 */
function unknownEndpoint(volumeId: string): SignInEndpoint {
  return { protocol: 'sftp', displayName: volumeId, address: volumeId, host: volumeId }
}

/**
 * One dial's answer, keeping the two host-key payloads.
 *
 * ❗ Exhaustive over `ServerConnectOutcome`: a new outcome fails to compile here
 * rather than falling into a default arm that words it as something else.
 */
function readAttempt(outcome: ServerConnectOutcome): SignInAttemptOutcome {
  switch (outcome.outcome) {
    case 'connected':
      return { kind: 'connected', volumeId: outcome.volumeId }
    case 'cancelled':
      return { kind: 'cancelled' }
    case 'needs_host_key_approval':
      return { kind: 'needs_host_key', prompt: outcome }
    case 'host_key_revoked':
      return { kind: 'host_key_revoked', key: outcome }
    case 'authentication_rejected':
      return { kind: 'refused', refusal: 'authentication_rejected' }
    case 'needs_credentials':
      return { kind: 'refused', refusal: 'needs_credentials' }
    case 'auth_method_unsupported':
      return { kind: 'refused', refusal: 'auth_method_unsupported' }
    case 'certificate_untrusted':
      return { kind: 'refused', refusal: 'certificate_untrusted' }
    case 'not_a_webdav_server':
      return { kind: 'refused', refusal: 'not_a_webdav_server' }
    case 'invalid_url':
      return { kind: 'refused', refusal: 'invalid_url' }
    case 'timed_out':
      return { kind: 'refused', refusal: 'timed_out' }
    case 'unreachable':
      return { kind: 'refused', refusal: 'unreachable' }
  }
}
