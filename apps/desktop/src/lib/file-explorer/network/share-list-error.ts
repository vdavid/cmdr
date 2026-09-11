/** Carrying a typed `ShareListError` from a share listing to the pane that words it. */
import type { ShareListError } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** An `Error` that still carries the backend's typed listing refusal. */
export class ShareListFailure extends TypedFailure<ShareListError> {
  constructor(failure: ShareListError) {
    super(failure, `share list refused: ${failure.type}`)
    this.name = 'ShareListFailure'
  }
}

/** Throws a wire `ShareListError` as an `Error`, keeping the typed value. */
export function throwShareListError(failure: ShareListError): never {
  throw new ShareListFailure(failure)
}

/**
 * The typed refusal behind a caught value.
 *
 * Anything else that reached a listing's catch (a runtime exception, an IPC call
 * that never got an answer) reads as `protocol_error`, its text kept as the log
 * detail: every catch site gets a value `renderShareListError` can word, ❌ never
 * a cast that hands it an `Error` with no `type`.
 */
export function shareListErrorOf(error: unknown): ShareListError {
  return failureOf(ShareListFailure, error) ?? { type: 'protocol_error', message: String(error) }
}
