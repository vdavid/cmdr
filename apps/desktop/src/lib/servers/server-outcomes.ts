/**
 * The ONE place a backend `ServerConnectOutcome` or `SavedServerOutcome` becomes
 * the app's own words.
 *
 * ❗ Exhaustive over the wire enums, so a new outcome fails to COMPILE here rather
 * than falling into a default arm that words it as something else. Two readers of
 * this switch would be two chances to word one outcome differently, which is why
 * `connect-flow.ts` folds this result rather than re-reading the wire.
 *
 * The two host-key arms keep their payloads: the sheet's key step needs a
 * fingerprint to show, and a refusal sentence has nowhere to put one.
 */

import type { SavedServerOutcome } from '$lib/ipc/bindings'
import type { ServerConnectOutcome } from '$lib/tauri-commands'
import type { ConnectRefusalKind } from './connect-refusals'
import type { SignInAttemptOutcome } from './sign-in-contract'

/** How one dial ended. Everything a dial can answer, minus the caller-only hand-off. */
export type ServerDialOutcome = Exclude<SignInAttemptOutcome, { kind: 'handed_off' } | { kind: 'added' }>

/** The outcomes that are a bare REASON, carrying nothing a sentence needs beyond its kind. */
type PlainRefusalOutcome = Exclude<
  ServerConnectOutcome['outcome'],
  'connected' | 'cancelled' | 'needs_host_key_approval' | 'host_key_revoked' | 'region_mismatch'
>

/**
 * Each plain refusal's kind. ❗ A `Record`, so a new wire outcome that isn't handled
 * by the switch below fails to compile HERE rather than falling through to a default.
 */
const PLAIN_REFUSALS: Record<PlainRefusalOutcome, ConnectRefusalKind> = {
  authentication_rejected: 'authentication_rejected',
  needs_credentials: 'needs_credentials',
  auth_method_unsupported: 'auth_method_unsupported',
  certificate_untrusted: 'certificate_untrusted',
  not_a_webdav_server: 'not_a_webdav_server',
  invalid_url: 'invalid_url',
  start_folder_outside_root: 'start_folder_outside_root',
  timed_out: 'timed_out',
  unreachable: 'unreachable',
  access_denied: 'access_denied',
  bucket_list_refused: 'bucket_list_refused',
  bucket_not_found: 'bucket_not_found',
  clock_skewed: 'clock_skewed',
  not_an_s3_endpoint: 'not_an_s3_endpoint',
}

/** One dial's answer, in the app's own vocabulary. */
export function readConnectOutcome(outcome: ServerConnectOutcome): ServerDialOutcome {
  switch (outcome.outcome) {
    case 'connected':
      return { kind: 'connected', volumeId: outcome.volumeId }
    case 'cancelled':
      return { kind: 'cancelled' }
    case 'needs_host_key_approval':
      return { kind: 'needs_host_key', prompt: outcome }
    case 'host_key_revoked':
      return { kind: 'host_key_revoked', key: outcome }
    case 'region_mismatch':
      // ❗ The region rides along: "this bucket is in us-east-2" is the fix, and the
      // bare kind can only say "another region".
      return outcome.region
        ? { kind: 'refused', refusal: 'region_mismatch', region: outcome.region }
        : { kind: 'refused', refusal: 'region_mismatch' }
    default:
      return { kind: 'refused', refusal: PLAIN_REFUSALS[outcome.outcome] }
  }
}

/** How saving an edit ended: written, or refused with nothing written. */
export type SaveOutcome = { kind: 'saved' } | { kind: 'refused'; refusal: ConnectRefusalKind }

/**
 * One save's answer, in the app's own vocabulary.
 *
 * ❗ The backend's `unreachable` reads as `save_unconfirmed`, ❌ not the dial's
 * `unreachable`: nothing was saved, and the address that sentence points at is
 * locked in edit mode.
 */
export function readSavedServerOutcome(outcome: SavedServerOutcome): SaveOutcome {
  switch (outcome.outcome) {
    case 'saved':
      return { kind: 'saved' }
    case 'start_folder_outside_root':
      return { kind: 'refused', refusal: 'start_folder_outside_root' }
    case 'root_not_found':
      return { kind: 'refused', refusal: 'root_not_found' }
    case 'start_folder_not_found':
      return { kind: 'refused', refusal: 'start_folder_not_found' }
    case 'unreachable':
      return { kind: 'refused', refusal: 'save_unconfirmed' }
  }
}

/**
 * Whether the backend is waiting on a PERSON rather than reporting a dead end.
 *
 * ❗ `auth_method_unsupported` is deliberately out: the server challenged with a
 * scheme Cmdr doesn't speak, the secret never left, and no typing fixes it.
 * Opening a password box over it would ask for something that cannot help.
 *
 * S3's `access_denied` is IN: a bucket that turns a key away can mean a wrong
 * secret (Garage answers one that way), so the sheet, with the sentence asking
 * about both, is the one place a fix can be typed. `bucket_list_refused` is OUT:
 * the way past it is a bucket place, which no secret typed here creates.
 */
export function needsAHuman(outcome: ServerDialOutcome): boolean {
  return (
    outcome.kind === 'needs_host_key' ||
    (outcome.kind === 'refused' &&
      (outcome.refusal === 'needs_credentials' ||
        outcome.refusal === 'authentication_rejected' ||
        outcome.refusal === 'access_denied'))
  )
}
