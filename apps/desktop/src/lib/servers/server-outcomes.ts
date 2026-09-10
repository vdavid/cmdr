/**
 * The ONE place a backend `ServerConnectOutcome` becomes the app's own words.
 *
 * ❗ Exhaustive over the wire enum, so a new outcome fails to COMPILE here rather
 * than falling into a default arm that words it as something else. Two readers of
 * this switch would be two chances to word one outcome differently, which is why
 * `connect-flow.ts` folds this result rather than re-reading the wire.
 *
 * The two host-key arms keep their payloads: the sheet's key step needs a
 * fingerprint to show, and a refusal sentence has nowhere to put one.
 */

import type { ServerConnectOutcome } from '$lib/tauri-commands'
import type { SignInAttemptOutcome } from './sign-in-contract'

/** How one dial ended. Everything a dial can answer, minus the caller-only hand-off. */
export type ServerDialOutcome = Exclude<SignInAttemptOutcome, { kind: 'handed_off' }>

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
    case 'start_folder_outside_root':
      // ❗ A stand-in: the add form has no start-folder field yet, so a dial never carries one the backend could
      // refuse. It borrows the address refusal until the field lands with a sentence of its own.
      return { kind: 'refused', refusal: 'invalid_url' }
    case 'timed_out':
      return { kind: 'refused', refusal: 'timed_out' }
    case 'unreachable':
      return { kind: 'refused', refusal: 'unreachable' }
  }
}

/**
 * Whether the backend is waiting on a PERSON rather than reporting a dead end.
 *
 * ❗ `auth_method_unsupported` is deliberately out: the server challenged with a
 * scheme Cmdr doesn't speak, the secret never left, and no typing fixes it.
 * Opening a password box over it would ask for something that cannot help.
 */
export function needsAHuman(outcome: ServerDialOutcome): boolean {
  return (
    outcome.kind === 'needs_host_key' ||
    (outcome.kind === 'refused' &&
      (outcome.refusal === 'needs_credentials' || outcome.refusal === 'authentication_rejected'))
  )
}
