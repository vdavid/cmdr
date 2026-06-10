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
        title: 'AI is turned off',
        body: 'Turn on a provider in Settings > AI to use AI search.',
        level: 'warn',
      }
    case 'notConfigured':
      return {
        title: 'AI needs a bit more setup',
        body: 'Finish your provider setup in Settings > AI, then try again.',
        level: 'warn',
      }
    case 'authFailed':
      return {
        title: 'That API key got turned down',
        body: 'Check your key in Settings > AI - it might be wrong or revoked.',
        level: 'error',
      }
    case 'rateLimited':
      return {
        title: 'Your AI provider is out of room',
        body: "It's rate-limiting requests or your plan is out of quota. Check your plan and billing, then try again.",
        level: 'warn',
      }
    case 'timeout':
      return {
        title: 'The AI took too long',
        body: 'The request timed out. Give it another go in a moment.',
        level: 'warn',
      }
    case 'unavailable':
      return {
        title: "Can't reach your AI provider",
        body: "Check your internet or the provider's status, then try again.",
        level: 'warn',
      }
    case 'emptyResponse':
      return {
        title: 'The AI came back empty',
        body: 'Your model may need a bigger budget for this. Try a faster model like gpt-4.1-mini in Settings > AI.',
        level: 'warn',
      }
    case 'serverError':
      return {
        title: 'Your AI provider hit a snag',
        body: 'It returned something unexpected. Try again in a moment.',
        level: 'warn',
      }
    case 'parseError':
      return {
        title: "Couldn't read the AI's answer",
        body: 'Try again, or pick a different model in Settings > AI.',
        level: 'warn',
      }
    case 'unknownProvider':
      return {
        title: "That AI provider isn't recognized",
        body: 'Pick a provider in Settings > AI.',
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
