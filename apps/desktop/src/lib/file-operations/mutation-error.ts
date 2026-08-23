/**
 * Carrying a typed `MutationError` from an IPC call to the surface that words it.
 *
 * `throwIpcError` flattens anything without a `.message` into
 * `new Error(JSON.stringify(...))`, which would turn a typed refusal back into
 * the string this whole path exists to get rid of. These two keep the value
 * intact instead: the wrapper throws a `MutationFailure`, and the catch site asks
 * `asMutationError` for the typed value back.
 */
import type { MutationError } from '$lib/ipc/bindings'

/** An `Error` that still carries the backend's typed refusal. */
export class MutationFailure extends Error {
  /** The typed refusal, for the factory that renders its words. */
  readonly failure: MutationError

  constructor(failure: MutationError) {
    // `Error.message` is a best-effort diagnostic for logs and generic
    // consumers; nothing a user reads comes from it.
    super(`mutation refused: ${failure.type}`)
    this.name = 'MutationFailure'
    this.failure = failure
  }
}

/** Throws a wire `MutationError` as an `Error`, keeping the typed value. */
export function throwMutationError(failure: MutationError): never {
  throw new MutationFailure(failure)
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asMutationError(error: unknown): MutationError | null {
  return error instanceof MutationFailure ? error.failure : null
}

/** Whether a caught value is the backend saying "I haven't answered yet". */
export function isMutationTimeout(error: unknown): boolean {
  return asMutationError(error)?.type === 'timedOut'
}
