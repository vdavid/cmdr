/** Carrying a typed `MountError` from `mountNetworkShare` to the pane that words it. */
import type { MountError } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** An `Error` that still carries the backend's typed mount refusal. */
export class MountFailure extends TypedFailure<MountError> {
  constructor(failure: MountError) {
    super(failure, `mount refused: ${failure.type}`)
    this.name = 'MountFailure'
  }
}

/** Throws a wire `MountError` as an `Error`, keeping the typed value. */
export function throwMountError(failure: MountError): never {
  throw new MountFailure(failure)
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asMountError(error: unknown): MountError | null {
  return failureOf(MountFailure, error)
}
