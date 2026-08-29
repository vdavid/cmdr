/** Carrying a typed `RollbackRefusal` from `rollbackOperation` to the row that words it. */
import type { RollbackRefusal } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** An `Error` that still carries the backend's typed rollback refusal. */
export class RollbackRefusalFailure extends TypedFailure<RollbackRefusal> {
  constructor(failure: RollbackRefusal) {
    super(failure, `rollback refused: ${failure.kind}`)
    this.name = 'RollbackRefusalFailure'
  }
}

/** Throws a wire `RollbackRefusal` as an `Error`, keeping the typed value. */
export function throwRollbackRefusal(failure: RollbackRefusal): never {
  throw new RollbackRefusalFailure(failure)
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asRollbackRefusal(error: unknown): RollbackRefusal | null {
  return failureOf(RollbackRefusalFailure, error)
}
