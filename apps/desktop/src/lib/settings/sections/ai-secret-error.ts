import { tString } from '$lib/intl/messages.svelte'
import { isMacOS } from '$lib/shortcuts/key-capture'
import type { AiApiKeyError } from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'

/** An `Error` that still carries the secret store's typed refusal. */
export class AiSecretFailure extends TypedFailure<AiApiKeyError> {
  constructor(failure: AiApiKeyError) {
    super(failure, `secret store refused: ${failure.type}`)
    this.name = 'AiSecretFailure'
  }
}

/** Throws a wire `AiApiKeyError` as an `Error`, keeping the typed value. */
export function throwAiSecretError(failure: AiApiKeyError): never {
  throw new AiSecretFailure(failure)
}

/**
 * The typed refusal behind a caught value, or `null` when it isn't one.
 *
 * `configure_ai` hands its `secretStoreError` back as a bare wire value rather
 * than a throw, so that shape is accepted here too; both roads reach the same
 * variant.
 */
export function asAiSecretError(error: unknown): AiApiKeyError | null {
  const thrown = failureOf(AiSecretFailure, error)
  if (thrown !== null) return thrown
  if (typeof error !== 'object' || error === null || !('type' in error)) return null
  const tag = error.type
  return tag === 'not_found' || tag === 'access_denied' || tag === 'other' ? (error as AiApiKeyError) : null
}

export interface SecretErrorMessage {
  /** Short, fits in a toast title or inline status line. */
  title: string
  /** Optional second sentence with actionable guidance (open Keychain Access, unlock keyring, etc.). */
  body?: string
  /** The store's own words, for a "details" affordance. ❌ Never the message itself. */
  detail?: string
  /** Toast level the caller should use when surfacing this. */
  level: 'warn' | 'error'
}

/**
 * Translate a save/read refusal from the secret store into user-facing copy.
 *
 * The backend's `AiApiKeyError` is typed, so the branch is a VARIANT: `access_denied`
 * is the one the OS-specific guidance exists for (a Keychain ACL on macOS, a locked
 * keyring on Linux), and everything else reads as the generic notice. ❌ Nothing here
 * inspects the store's message: `detail` carries it for a details affordance and
 * nothing else.
 */
export function describeSecretError(e: unknown, operation: 'save' | 'read'): SecretErrorMessage {
  const failure = asAiSecretError(e)
  const detail = failure?.message ?? (e instanceof Error ? e.message : typeof e === 'string' ? e : undefined)

  if (failure?.type === 'access_denied') {
    if (isMacOS()) {
      return {
        title: tString('ai.secretError.keychainTitle', { op: operation }),
        body: tString('ai.secretError.keychainBody', { op: operation }),
        detail,
        level: 'error',
      }
    }
    return {
      title: tString('ai.secretError.keyringTitle', { op: operation }),
      body: tString('ai.secretError.keyringBody'),
      detail,
      level: 'error',
    }
  }

  return {
    title: tString('ai.secretError.genericTitle', { op: operation }),
    body: tString('ai.secretError.genericBody'),
    detail,
    level: 'error',
  }
}
