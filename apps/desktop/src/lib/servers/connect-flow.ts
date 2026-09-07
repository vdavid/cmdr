/**
 * Bringing a place to life, whatever standing it is in.
 *
 * ❗ **The one caller of `connectSavedPlace` and the reconnect manager's lazy
 * start.** The switcher, the hub, the pane banner, and the sheet's Connect
 * button all come through here, because the move depends on the volume's
 * standing and picking it wrong is silent: re-dialing a registered volume
 * registers a SECOND one, and dialing a volume the backoff loop already owns
 * races it.
 *
 * Three arms (`DETAILS.md` § "The three arms"):
 *
 *  1. registered and `disconnected` → the reconnect manager owns recovery, so
 *     this subscribes it and renders `connecting`. ❌ Never a dial.
 *  2. registered and `needs_sign_in` → the sign-in sheet, through the
 *     `openSignIn` seam. A caller that supplies none gets the refusal instead,
 *     which is the honest thing to show: the server is asking for a credential
 *     and there is nowhere to type one.
 *  3. absent (a `saved` row) → a dial by saved entry.
 *
 * The attempt id is minted BEFORE the first dial and handed to the caller
 * synchronously, so a Cancel button is armed from the first millisecond: a dial
 * can run for 30 s and this promise doesn't settle until it is over.
 */

import {
  cancelServerConnect,
  connectSavedPlace,
  newServerAttemptId,
  type ServerConnectOutcome,
} from '$lib/tauri-commands'
import { smbReconnectManager } from '$lib/file-explorer/network/smb-reconnect-manager.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { ConnectionState } from '$lib/file-explorer/types'
import type { ConnectRefusalKind } from './connect-refusals'
import { needsAHuman, readConnectOutcome, type ServerDialOutcome } from './server-outcomes'

const log = getAppLogger('servers')

/** How a connect ended. */
export type ConnectFlowResult =
  /** A live volume under this id. The pane navigates into it. */
  | { kind: 'connected'; volumeId: string }
  /** The backoff loop owns it now; the pane keeps rendering `connecting`. */
  | { kind: 'reconnecting' }
  /** The user pressed Cancel. ❗ Says nothing: they know. */
  | { kind: 'cancelled' }
  /** Something to tell the user, with a Try again beside it. */
  | { kind: 'refused'; refusal: ConnectRefusalKind }
  /** Nothing to do: the session is already serving. */
  | { kind: 'already_live' }

/** What a sign-in sheet answers. */
export type SignInSeamResult = { signedIn: true; volumeId: string } | { signedIn: false }

/** What the flow needs a human for. */
export interface SignInSeamRequest {
  /** The place's volume id, the same one a `saved` row carries. */
  volumeId: string
  /**
   * Whether a volume is REGISTERED under that id. It decides which command the
   * sheet's attempt uses, and getting it wrong is silent: re-dialing a
   * registered volume registers a second one under a second id.
   */
  registered: boolean
  /**
   * What the first dial answered, when there was one. It is what opens the sheet
   * on the host-key step rather than on the credential fields, and what the
   * sheet's first-round sentence is read off.
   */
  firstOutcome?: ServerConnectOutcome
  /**
   * Why the person is being asked, for a place there was no dial to read it off:
   * a REGISTERED volume already sitting in `needs_sign_in`. ❗ Lower precedence
   * than `firstOutcome`, which is the live answer when there is one.
   */
  refusal?: ConnectRefusalKind
}

/**
 * Opens the sign-in sheet and resolves on the user's answer.
 *
 * ❗ The sheet, not this flow, owns the ROUNDS: a first connect to a new SFTP
 * server is three round-trips, and a sheet that closed between them would lose
 * what the user typed. This flow decides WHEN a human is needed; the sheet
 * decides how many times to ask.
 */
export type SignInSeam = (request: SignInSeamRequest) => Promise<SignInSeamResult>

export interface ConnectPlaceRequest {
  /** The place's volume id, the same one a `saved` row carries. */
  volumeId: string
  /** Its standing, read from the volume list. */
  connectionState?: ConnectionState | null
  /**
   * Called with the attempt id the moment it exists, BEFORE the dial. A Cancel
   * button that waits for the dial to return has nothing to aim at for 30 s.
   */
  onAttemptStarted?: (attemptId: string) => void
  /**
   * The sign-in sheet, awaited when the backend says a human is what's missing.
   * Every production caller supplies `openSignInForPlace`; a caller without one
   * gets the refusal, and ❌ never an inert "Sign in…" button.
   */
  openSignIn?: SignInSeam
}

/** Brings the place at `volumeId` to life, picking the move by its standing. */
export async function connectPlace(request: ConnectPlaceRequest): Promise<ConnectFlowResult> {
  const { volumeId, connectionState } = request

  if (connectionState === 'direct' || connectionState === 'os_mount') {
    return { kind: 'already_live' }
  }

  if (connectionState === 'disconnected') {
    // Arm 1. The manager starts the cycle if none is running (the lazy-nav path)
    // and is idempotent, so landing on the same place twice costs nothing.
    smbReconnectManager.startCycle(volumeId)
    return { kind: 'reconnecting' }
  }

  if (connectionState === 'needs_sign_in') {
    // Arm 2. ❗ The volume is REGISTERED, so the sheet mends it with
    // `reconnectVolumeWithCredentials` rather than dialing: a dial would
    // register a second volume under a second id.
    return await handOver(request, { volumeId, registered: true, refusal: 'needs_credentials' }, 'needs_credentials')
  }

  // Arm 3: nothing is registered, so this is a dial by saved entry.
  const attemptId = newServerAttemptId()
  request.onAttemptStarted?.(attemptId)
  let outcome: ServerConnectOutcome
  try {
    outcome = await connectSavedPlace(volumeId, attemptId)
  } catch (e) {
    // A typed `SavedPlaceRefusal`: the caller picked the wrong arm for this
    // volume's standing, which is a bug to read in a log, ❌ never a sentence to
    // put in front of a person. The pane says the connection didn't happen.
    log.warn('Dialing the saved place {volumeId} was refused: {error}', { volumeId, error: String(e) })
    return { kind: 'refused', refusal: 'unreachable' }
  }

  const dialed = readConnectOutcome(outcome)
  if (needsAHuman(dialed)) {
    // The sheet opens on the step this outcome names, and dials again itself
    // through the attempt it is handed. ❗ It stays open across those rounds.
    return await handOver(request, { volumeId, registered: false, firstOutcome: outcome }, refusalFor(dialed))
  }
  return readOutcome(dialed)
}

/** Hands the place to the sheet, or refuses with `whenNoSheet` when there is none. */
async function handOver(
  request: ConnectPlaceRequest,
  seamRequest: SignInSeamRequest,
  whenNoSheet: ConnectRefusalKind,
): Promise<ConnectFlowResult> {
  if (!request.openSignIn) return { kind: 'refused', refusal: whenNoSheet }
  const result = await request.openSignIn(seamRequest)
  return result.signedIn ? { kind: 'connected', volumeId: result.volumeId } : { kind: 'cancelled' }
}

/** What to say when the outcome needed a human and no sheet was supplied. */
function refusalFor(outcome: ServerDialOutcome): ConnectRefusalKind {
  const result = readOutcome(outcome)
  return result.kind === 'refused' ? result.refusal : 'needs_credentials'
}

/** Calls off the attempt `attemptId` names. A cancel that lands late is fine. */
export async function cancelPlaceConnect(attemptId: string): Promise<void> {
  try {
    await cancelServerConnect(attemptId)
  } catch (e) {
    log.warn('Cancelling the connect attempt {attemptId} broke down: {error}', { attemptId, error: String(e) })
  }
}

/**
 * The dial's answer, folded into the flow's own result.
 *
 * ❗ Folds `server-outcomes.ts`'s reading rather than re-reading the wire enum: a
 * second switch over `ServerConnectOutcome` would be a second chance to word one
 * outcome differently.
 */
function readOutcome(outcome: ServerDialOutcome): ConnectFlowResult {
  switch (outcome.kind) {
    case 'connected':
      return { kind: 'connected', volumeId: outcome.volumeId }
    case 'cancelled':
      return { kind: 'cancelled' }
    case 'needs_host_key':
      return { kind: 'refused', refusal: 'host_key_untrusted' }
    case 'host_key_revoked':
      return { kind: 'refused', refusal: 'host_key_revoked' }
    case 'refused':
      return { kind: 'refused', refusal: outcome.refusal }
  }
}
