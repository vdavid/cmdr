/** Carrying a typed `ReconnectError` from the SMB reconnect commands to the log that records it. */
import type { ReconnectError } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/**
 * An `Error` that still carries the backend's typed reconnect refusal.
 *
 * Its `Error.message` is a compact diagnostic (`reconnect refused: volume`), so
 * the reconnect manager's existing log lines stay readable without anyone
 * reading a backend sentence.
 */
export class ReconnectFailure extends TypedFailure<ReconnectError> {
  constructor(failure: ReconnectError) {
    super(failure, `reconnect refused: ${failure.type}`)
    this.name = 'ReconnectFailure'
  }
}

/** Throws a wire `ReconnectError` as an `Error`, keeping the typed value. */
export function throwReconnectError(failure: ReconnectError): never {
  throw new ReconnectFailure(failure)
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asReconnectError(error: unknown): ReconnectError | null {
  return failureOf(ReconnectFailure, error)
}

/**
 * A one-line diagnostic for the log: the refusal's REASON, never a sentence.
 *
 * ❌ Not user-facing. The reconnect surfaces (the reconnecting view, the gave-up
 * banner, the sign-in form) carry their own translated copy; this exists so a
 * log line records what happened without anyone parsing prose to find out.
 */
export function describeReconnectRefusal(error: ReconnectError): string {
  return error.type === 'volumeNotFound' ? `volumeNotFound(${error.volumeId})` : `volume/${error.error.type}`
}
