/** Carrying a typed `MutationError` from an IPC call to the surface that words it. */
import type { MutationError } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** An `Error` that still carries the backend's typed refusal. */
export class MutationFailure extends TypedFailure<MutationError> {
  constructor(failure: MutationError) {
    super(failure, `mutation refused: ${failure.type}`)
    this.name = 'MutationFailure'
  }
}

/** Throws a wire `MutationError` as an `Error`, keeping the typed value. */
export function throwMutationError(failure: MutationError): never {
  throw new MutationFailure(failure)
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asMutationError(error: unknown): MutationError | null {
  return failureOf(MutationFailure, error)
}

/** Whether a caught value is the backend saying "I haven't answered yet". */
export function isMutationTimeout(error: unknown): boolean {
  return asMutationError(error)?.type === 'timedOut'
}
