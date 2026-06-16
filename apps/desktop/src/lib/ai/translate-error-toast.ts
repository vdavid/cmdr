/**
 * Maps a typed AI-translation failure to a friendly, actionable toast.
 *
 * Both the Search and Selection dialogs translate a natural-language prompt through the
 * backend (`translate_search_query` / `translate_selection_query`). When that call fails,
 * the backend returns a typed `AiTranslateError { kind, message }` (see
 * `src-tauri/src/ai/translate_error.rs`). The dialogs surface it through QueryDialog so the
 * user learns WHY nothing came back instead of staring at a silent no-op.
 *
 * We branch on `kind`, never on the message string (the `no-error-string-match` rule). Keep
 * the switch below in lockstep with the `AiTranslateErrorKind` enum on the Rust side.
 */

import type { AiTranslateErrorKind } from '$lib/ipc/bindings'
import { addToast, type ToastLevel } from '$lib/ui/toast/toast-store.svelte'
import { tString } from '$lib/intl/messages.svelte'

/** A thrown AI-translation failure: a real `Error` that also carries the typed `kind`. */
export type AiTranslateThrown = Error & { kind: AiTranslateErrorKind }

const ALL_KINDS: ReadonlySet<string> = new Set<AiTranslateErrorKind>([
  'off',
  'notConfigured',
  'authFailed',
  'rateLimited',
  'timeout',
  'unavailable',
  'emptyResponse',
  'serverError',
  'parseError',
  'unknownProvider',
])

/**
 * True when `e` is an AI-translation failure carrying a known `kind`. The IPC wrapper throws
 * `Object.assign(new Error(message), { kind, message })`, so the kind rides on the Error.
 */
export function isAiTranslateError(e: unknown): e is AiTranslateThrown {
  return (
    e instanceof Error &&
    'kind' in e &&
    typeof (e as { kind: unknown }).kind === 'string' &&
    ALL_KINDS.has((e as { kind: string }).kind)
  )
}

export interface AiTranslateToastCopy {
  title: string
  /** Second line: the suggested next step. */
  body: string
  level: ToastLevel
}

/**
 * Friendly, actionable copy for each failure kind. Pure, so it's unit-tested directly.
 * Follows the style guide: no "error"/"failed" in the user-facing strings, active voice,
 * sentence case, one concrete next step.
 */
export function aiTranslateErrorToast(kind: AiTranslateErrorKind): AiTranslateToastCopy {
  switch (kind) {
    case 'off':
      return {
        title: tString('ai.translateError.off.title'),
        body: tString('ai.translateError.off.body'),
        level: 'warn',
      }
    case 'notConfigured':
      return {
        title: tString('ai.translateError.notConfigured.title'),
        body: tString('ai.translateError.notConfigured.body'),
        level: 'warn',
      }
    case 'authFailed':
      return {
        title: tString('ai.translateError.authFailed.title'),
        body: tString('ai.translateError.authFailed.body'),
        level: 'error',
      }
    case 'rateLimited':
      return {
        title: tString('ai.translateError.rateLimited.title'),
        body: tString('ai.translateError.rateLimited.body'),
        level: 'warn',
      }
    case 'timeout':
      return {
        title: tString('ai.translateError.timeout.title'),
        body: tString('ai.translateError.timeout.body'),
        level: 'warn',
      }
    case 'unavailable':
      return {
        title: tString('ai.translateError.unavailable.title'),
        body: tString('ai.translateError.unavailable.body'),
        level: 'warn',
      }
    case 'emptyResponse':
      return {
        title: tString('ai.translateError.emptyResponse.title'),
        body: tString('ai.translateError.emptyResponse.body'),
        level: 'warn',
      }
    case 'serverError':
      return {
        title: tString('ai.translateError.serverError.title'),
        body: tString('ai.translateError.serverError.body'),
        level: 'warn',
      }
    case 'parseError':
      return {
        title: tString('ai.translateError.parseError.title'),
        body: tString('ai.translateError.parseError.body'),
        level: 'warn',
      }
    case 'unknownProvider':
      return {
        title: tString('ai.translateError.unknownProvider.title'),
        body: tString('ai.translateError.unknownProvider.body'),
        level: 'warn',
      }
  }
}

/** Stable id so repeated failures replace the toast in place instead of stacking. */
const AI_TRANSLATE_TOAST_ID = 'ai-translate-error'

/**
 * Surfaces an AI-translation failure as a toast. Returns `true` when `err` was a typed
 * translation failure we recognized and toasted; `false` otherwise (the caller decides
 * whether to show a generic fallback). Lives here, not inline in QueryDialog, so the mapping
 * stays pure and unit-tested while only this thin wrapper touches `addToast`.
 */
export function showAiTranslateErrorToast(err: unknown): boolean {
  if (!isAiTranslateError(err)) return false
  const copy = aiTranslateErrorToast(err.kind)
  addToast(`${copy.title}\n${copy.body}`, {
    level: copy.level,
    dismissal: 'transient',
    timeoutMs: 8000,
    id: AI_TRANSLATE_TOAST_ID,
  })
  return true
}
