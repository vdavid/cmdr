/** Carrying a typed `EjectError` from `ejectVolume` / `disconnectSmbVolume` to the toast that words it. */
import type { EjectError } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** An `Error` that still carries the backend's typed eject refusal. */
export class EjectFailure extends TypedFailure<EjectError> {
  constructor(failure: EjectError) {
    super(failure, `eject refused: ${failure.type}`)
    this.name = 'EjectFailure'
  }
}

/** Throws a wire `EjectError` as an `Error`, keeping the typed value. */
export function throwEjectError(failure: EjectError): never {
  throw new EjectFailure(failure)
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asEjectError(error: unknown): EjectError | null {
  return failureOf(EjectFailure, error)
}
