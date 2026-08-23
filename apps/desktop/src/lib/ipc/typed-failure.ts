/**
 * Carrying a backend's TYPED refusal from an IPC call to the surface that words it.
 *
 * `throwIpcError` flattens anything without a `.message` into
 * `new Error(JSON.stringify(...))`, which turns a typed refusal back into the
 * string the typed-error work exists to get rid of. `TypedFailure` keeps the
 * value intact instead: the command wrapper throws a subclass, and the catch
 * site asks {@link failureOf} for the typed value back.
 *
 * One subclass per wire type, so `instanceof` narrows to the right payload and
 * no family can accidentally read another's. Adding a family is three lines
 * (see `$lib/file-operations/mutation-error.ts` for the shape).
 */

/** An `Error` that still carries the backend's typed refusal. */
export abstract class TypedFailure<T> extends Error {
  /** The typed refusal, for the factory that renders its words. */
  readonly failure: T

  /**
   * `diagnostic` becomes `Error.message`: a best-effort line for logs and
   * generic consumers. ❌ Nothing a user reads comes from it.
   */
  protected constructor(failure: T, diagnostic: string) {
    super(diagnostic)
    this.failure = failure
  }
}

/**
 * The typed refusal behind a caught value, or `null` when it isn't one of
 * `ctor`'s.
 *
 * ```ts
 * const refusal = failureOf(EjectFailure, e)
 * ```
 */
export function failureOf<T>(ctor: abstract new (...args: never[]) => TypedFailure<T>, error: unknown): T | null {
  return error instanceof ctor ? error.failure : null
}
