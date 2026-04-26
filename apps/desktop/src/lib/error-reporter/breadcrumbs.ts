import { invoke } from '@tauri-apps/api/core'

/** Free-form structured context attached to a breadcrumb. */
export type BreadcrumbCtx = Record<string, unknown>

/**
 * Record a triage breadcrumb. Fire-and-forget — failures (e.g. backend not ready
 * during early startup) are silently swallowed.
 *
 * Convention:
 *   kind:     short label like 'nav', 'command', 'dialog', 'transfer', 'error-shown'
 *   message:  short human-readable description ('to /Users/x', 'open settings')
 *   ctx:      optional structured fields for triage ({ from, to, paneId, ... })
 *
 * Wire this into FE event handlers, navigation transitions, and dialog open/close
 * sites so error report bundles carry context for "what led up to this."
 */
export function recordBreadcrumb(kind: string, message: string, ctx?: BreadcrumbCtx): void {
  void invoke('record_breadcrumb', { kind, message, ctx: ctx ?? null }).catch(() => {
    // Best-effort: a failing breadcrumb shouldn't break the UI flow.
  })
}
