import { tString } from '$lib/intl/messages.svelte'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { isIpcError } from '$lib/tauri-commands/ipc-types'

/** Possible variants of the Rust `AiApiKeyError` enum we surface to the UI. */
type SecretErrorKind = 'access_denied' | 'other' | 'unknown'

export interface SecretErrorMessage {
  /** Short, fits in a toast title or inline status line. */
  title: string
  /** Optional second sentence with actionable guidance (open Keychain Access, unlock keyring, etc.). */
  body?: string
  /** Underlying error message from the OS, for a "details" affordance. */
  detail?: string
  /** Toast level the caller should use when surfacing this. */
  level: 'warn' | 'error'
}

/** Pulls a `kind` + `message` out of whatever the Tauri command rejected with. The error shape
 *  varies: `IpcError` from `throwIpcError`, a bare `AiApiKeyError` object, or a generic JS Error. */
function extractErrorShape(e: unknown): { kind: SecretErrorKind; message: string } {
  // IpcError shape: { message, timedOut }. Wraps the underlying serialized error.
  if (isIpcError(e)) {
    const msg = e.message
    const kind = inferKindFromMessage(msg)
    return { kind, message: msg }
  }

  // Raw `AiApiKeyError` serialized over IPC: { type: 'access_denied' | 'other' | 'not_found', message }
  if (typeof e === 'object' && e !== null && 'type' in e) {
    const obj = e as Record<string, unknown>
    const tag = typeof obj.type === 'string' ? obj.type : ''
    const message = typeof obj.message === 'string' ? obj.message : 'Unknown error'
    if (tag === 'access_denied') return { kind: 'access_denied', message }
    if (tag) return { kind: 'other', message }
  }

  if (e instanceof Error) {
    return { kind: inferKindFromMessage(e.message), message: e.message }
  }

  return { kind: 'unknown', message: typeof e === 'string' ? e : 'Unknown error' }
}

/** Heuristic for stringly-typed errors. Prefer the typed `AiApiKeyError` path when possible. */
function inferKindFromMessage(msg: string): SecretErrorKind {
  const lower = msg.toLowerCase()

  if (lower.includes('denied') || lower.includes('cancelled') || lower.includes('canceled')) {
    return 'access_denied'
  }
  return 'other'
}

/** Translate a save/read failure from the secret store into user-facing copy. Platform-aware
 *  guidance helps users actually fix the underlying issue (Keychain ACL on macOS, locked keyring
 *  on Linux, etc.) instead of just seeing a raw error message. */
export function describeSecretError(e: unknown, operation: 'save' | 'read'): SecretErrorMessage {
  const { kind, message } = extractErrorShape(e)

  if (kind === 'access_denied') {
    if (isMacOS()) {
      return {
        title: tString('ai.secretError.keychainTitle', { op: operation }),
        body: tString('ai.secretError.keychainBody', { op: operation }),
        detail: message,
        level: 'error',
      }
    }
    return {
      title: tString('ai.secretError.keyringTitle', { op: operation }),
      body: tString('ai.secretError.keyringBody'),
      detail: message,
      level: 'error',
    }
  }

  return {
    title: tString('ai.secretError.genericTitle', { op: operation }),
    body: tString('ai.secretError.genericBody'),
    detail: message,
    level: 'error',
  }
}
