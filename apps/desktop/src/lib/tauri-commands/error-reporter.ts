// Error reporter commands (Flow A: user-initiated) and the Flow B auto-send event

import { invoke } from '@tauri-apps/api/core'
import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type ErrorReportAutoSent } from '$lib/ipc/bindings'
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
  generatedAt: string
}

export interface PreviewPayload {
  /** Local ID. The server may issue a different one on send. Treat as a hint, not authoritative. */
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
 * Re-build the bundle and upload it. Returns the server-issued ID.
 * Display the returned `id` to the user, not the one from `prepareErrorReportPreview`.
 *
 * `email` is included only when the user ticked the attach-email box.
 */
export async function sendErrorReport(userNote?: string, email?: string): Promise<{ id: string }> {
  const res = await commands.sendErrorReport(userNote ?? null, email ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Debug-only: write the bundle to the app data dir and return the path.
 * In production the command isn't registered, so calling it returns an error.
 */
export async function saveErrorReportToDisk(userNote?: string, email?: string): Promise<string> {
  const res = await commands.saveErrorReportToDisk(userNote ?? null, email ?? null)
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
