/**
 * The typed error `start_ai_server` and `start_ai_download` reject with (Rust `LocalAiError`). Both
 * commands are invoked raw (they're generic, so specta has no typed wrapper), so a rejection arrives
 * as `unknown`; these turn it back into the type and say how loud to log it.
 */

import type { LocalAiError } from '$lib/ipc/bindings'

const KNOWN_TYPES: ReadonlySet<string> = new Set<LocalAiError['type']>([
  'managed',
  'unsupported',
  'cancelled',
  'failed',
])

/** The backend's typed error, or `failed` for anything else (an IPC failure, a stray throw). */
export function toLocalAiError(error: unknown): LocalAiError {
  if (typeof error === 'object' && error !== null && 'type' in error && typeof error.type === 'string') {
    if (KNOWN_TYPES.has(error.type)) return error as LocalAiError
  }
  return { type: 'failed', detail: String(error) }
}

/**
 * How loud a local-AI error is. The organization's refusal and a cancel (the person's, or a policy
 * change's) aren't failures: an error-level log can send an automatic error report.
 */
export function localAiErrorLogLevel(error: LocalAiError): 'info' | 'error' {
  return error.type === 'managed' || error.type === 'cancelled' ? 'info' : 'error'
}
