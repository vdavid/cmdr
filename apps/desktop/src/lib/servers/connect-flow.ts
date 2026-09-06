/**
 * Bringing a place to life, whatever standing it is in.
 *
 * ❗ **The one caller of `connectSavedPlace` and the reconnect manager's lazy
 * start.** The switcher, the hub, the pane banner, and (from M2) the sheet's
 * Connect button all come through here, because the move depends on the volume's
 * standing and picking it wrong is silent: re-dialing a registered volume
 * registers a SECOND one, and dialing a volume the backoff loop already owns
 * races it.
 *
 * Three arms (`docs/specs/servers-hub-plan.md` § D8):
 *
 *  1. registered and `disconnected` → the reconnect manager owns recovery, so
 *     this subscribes it and renders `connecting`. ❌ Never a dial.
 *  2. registered and `needs_sign_in` → the sign-in sheet, through the
 *     `openSignIn` seam. Until M2 supplies one, the flow refuses with the
 *     reason, which is the honest thing to show: the server is asking for a
 *     credential and nothing here can collect it yet.
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

const log = getAppLogger('servers')

/**
 * Why a connect stopped, in the vocabulary the pane words.
 *
 * ❗ One kind per outcome that is a REASON, ❌ never collapsed:
 * `auth_method_unsupported` is not `authentication_rejected` (the server never
 * saw the secret, so "check your password" is the wrong fix), and
 * `needs_credentials` is not one either (telling someone who has never entered a
 * password that theirs is wrong is what collapsing the two does).
 */
export type ConnectRefusalKind =
  | 'authentication_rejected'
  | 'needs_credentials'
  | 'auth_method_unsupported'
  | 'certificate_untrusted'
  | 'not_a_webdav_server'
  | 'invalid_url'
  | 'timed_out'
  | 'unreachable'
  | 'host_key_untrusted'
  | 'host_key_revoked'

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

/** What a sign-in sheet answers. M2 supplies the sheet; this is its seam. */
export type SignInSeamResult = { signedIn: true; volumeId: string } | { signedIn: false }

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
   * ❗ M2's seam. The sign-in sheet, awaited when the backend says a credential
   * is what's missing. While it is absent the flow refuses with the reason
   * instead, and ❌ never renders an inert "Sign in…" button.
   */
  openSignIn?: (volumeId: string) => Promise<SignInSeamResult>
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
    // Arm 2.
    if (!request.openSignIn) return { kind: 'refused', refusal: 'needs_credentials' }
    const result = await request.openSignIn(volumeId)
    return result.signedIn ? { kind: 'connected', volumeId: result.volumeId } : { kind: 'cancelled' }
  }

  // Arm 3: nothing is registered, so this is a dial by saved entry.
  const attemptId = newServerAttemptId()
  request.onAttemptStarted?.(attemptId)
  try {
    return readOutcome(await connectSavedPlace(volumeId, attemptId))
  } catch (e) {
    // A typed `SavedPlaceRefusal`: the caller picked the wrong arm for this
    // volume's standing, which is a bug to read in a log, ❌ never a sentence to
    // put in front of a person. The pane says the connection didn't happen.
    log.warn('Dialing the saved place {volumeId} was refused: {error}', { volumeId, error: String(e) })
    return { kind: 'refused', refusal: 'unreachable' }
  }
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
 * The dial's answer, in the flow's own words.
 *
 * ❗ Exhaustive over `ServerConnectOutcome`: a new outcome fails to compile here
 * rather than falling into a default arm that words it as something else.
 */
function readOutcome(outcome: ServerConnectOutcome): ConnectFlowResult {
  switch (outcome.outcome) {
    case 'connected':
      return { kind: 'connected', volumeId: outcome.volumeId }
    case 'cancelled':
      return { kind: 'cancelled' }
    case 'needs_host_key_approval':
      return { kind: 'refused', refusal: 'host_key_untrusted' }
    case 'host_key_revoked':
      return { kind: 'refused', refusal: 'host_key_revoked' }
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
