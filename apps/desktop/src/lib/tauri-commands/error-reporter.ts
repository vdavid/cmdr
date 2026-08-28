// Error reporter commands (Flow A: user-initiated) and the Flow B auto-send event

import { invoke } from '@tauri-apps/api/core'
import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type ErrorReportAutoSent, type SystemSnapshot } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export interface ActiveSettingsSnapshot {
  indexingEnabled: boolean | null
  aiProvider: string | null
  mcpEnabled: boolean | null
  verboseLogging: boolean | null
}

export interface BundleManifest {
  id: string
  kind: 'user' | 'auto'
  appVersion: string
  osVersion: string
  arch: string
  activeSettings: ActiveSettingsSnapshot
  userNote?: string
  /** The `diag_<uuid>` diagnostics id. Never the `anal_` analytics id. */
  diagId: string
  /** Contact email, set only when the user ticks the attach-email box (Flow A). */
  email?: string
  /** Machine snapshot (model, CPU, RAM, disk, drive-index sizes, Cmdr's RSS) for triage. PII-free. */
  system: SystemSnapshot
  generatedAt: string
}

export interface PreviewPayload {
  /**
   * The report's ID. Authoritative: the dialog shows it and passes it straight back to
   * `sendErrorReport`, so the badge, the Copy button, and the post-send toast all name
   * the same report.
   */
  id: string
  /** Size of the zip bytes that would be uploaded. */
  sizeBytes: number
  manifest: BundleManifest
  sampleFirst: string[]
  sampleLast: string[]
  totalRedactedLines: number
}

/**
 * Build the bundle in-memory and return preview metadata. No network.
 *
 * `email` is the beta contact email the user opted to attach (Flow A only). Pass it so
 * the previewed manifest reflects exactly what'll ship.
 */
export async function prepareErrorReportPreview(userNote?: string, email?: string): Promise<PreviewPayload> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- BundleManifest contains Breadcrumb.ctx: Option<serde_json::Value>, which specta can't represent; excluded from typed bindings
  return invoke<PreviewPayload>('prepare_error_report_preview', { userNote, email })
}

/**
 * What the most recent Flow B auto-send shipped, or `null` when nothing was auto-sent
 * this run (the backend stash dies with the process). Same preview fields as
 * `PreviewPayload` plus `canAmend`.
 */
export interface AutoSentReport extends PreviewPayload {
  /** Whether a note can still be added. Branch on THIS, never on an error message. */
  canAmend: boolean
}

/**
 * Re-build the bundle and upload it. Returns the report's ID.
 *
 * Pass the `id` the preview returned so the report ships under the id the dialog showed;
 * omit it and the backend mints a fresh one, which is how a dialog ends up naming a report
 * that doesn't exist.
 *
 * `email` is included only when the user ticked the attach-email box.
 */
export async function sendErrorReport(userNote?: string, email?: string, id?: string): Promise<{ id: string }> {
  const res = await commands.sendErrorReport(userNote ?? null, email ?? null, id ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * What Flow B auto-sent this run, for the dialog's amend mode: no bundle rebuild, and the
 * manifest and sample lines are the ones that actually shipped. `null` means nothing was
 * auto-sent, so there's nothing to add to.
 */
export async function getAutoSentReportPreview(): Promise<AutoSentReport | null> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- BundleManifest contains Breadcrumb.ctx: Option<serde_json::Value>, which specta can't represent; excluded from typed bindings
  return invoke<AutoSentReport | null>('get_auto_sent_report_preview')
}

/**
 * Add a note (and optionally a reply-to address) to the report Flow B already sent.
 * Returns that report's id. Callable more than once: amendments accumulate server-side,
 * so disable the button while the call is in flight rather than after it returns.
 *
 * Takes no id because there's only ever one stashed report. It throws when nothing was
 * auto-sent or the server never handed back an amend key; `canAmend` from
 * `getAutoSentReportPreview` is the flag to branch on beforehand.
 */
export async function amendErrorReport(userNote?: string, email?: string): Promise<{ id: string }> {
  const res = await commands.amendErrorReport(userNote ?? null, email ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Debug-only: write the bundle to the app data dir and return the path.
 * In production the command isn't registered, so calling it returns an error.
 *
 * Takes the same `id` as `sendErrorReport` so the zip on disk is the bundle the send
 * would have shipped, id included.
 */
export async function saveErrorReportToDisk(userNote?: string, email?: string, id?: string): Promise<string> {
  const res = await commands.saveErrorReportToDisk(userNote ?? null, email ?? null, id ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Flow B: subscribes to `error-report-auto-sent`, emitted after a successful
 * opt-in auto-send. The payload's `id` is the server-issued `ERR-XXXXX` report
 * id; the FE shows a confirmation toast.
 */
export function onErrorReportAutoSent(handler: (payload: ErrorReportAutoSent) => void): Promise<UnlistenFn> {
  return events.errorReportAutoSent.listen((event) => {
    handler(event.payload)
  })
}
