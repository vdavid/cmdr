// Error reporter commands (Flow A: user-initiated) and the Flow B auto-send event

import { invoke } from '@tauri-apps/api/core'
import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type BreadcrumbEvent,
  type ErrorReportAutoSent,
  type OpenErrorReport,
  type ReportPreviewFile,
  type SystemSnapshot,
} from '#lib/ipc/bindings.js'
import { throwErrorReportSendError } from '#lib/error-reporter/error-report-send-error.js'
import { throwReportPreviewError } from '#lib/error-reporter/report-preview-error.js'
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
  /** Hand-picked behavior settings by id, at their effective values (`error_reporter/triage_settings.rs`). */
  triageSettings: Record<string, boolean | number | string>
  breadcrumbs: Array<{ at: string; event: BreadcrumbEvent }>
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
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- BundleManifest has serde-elided optional fields, which specta can't export as one command return type
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
 * `email` is included only when the user ticked the attach-email box. A send that doesn't land
 * throws an `ErrorReportSendFailure` carrying the typed reason.
 */
export async function sendErrorReport(userNote?: string, email?: string, id?: string): Promise<{ id: string }> {
  const res = await commands.sendErrorReport(userNote ?? null, email ?? null, id ?? null)
  if (res.status === 'error') throwErrorReportSendError(res.error)
  return res.data
}

/**
 * Send the log from the session a crash report describes, as its own error report.
 *
 * Reached only from the "crash report sent" toast's action, which IS the consent: crash reports are
 * on by default and carry no log, while `updates.errorReports` is opt-in, so nothing here may be
 * called without a person pressing the button. The bundle is scoped around `crashTimestamp` rather
 * than around now, because the lines worth reading are in the previous session.
 *
 * A send that doesn't land throws an `ErrorReportSendFailure` carrying the typed reason.
 */
export async function sendCrashLogReport(crashShortId: string, crashTimestamp: string): Promise<{ id: string }> {
  const res = await commands.sendCrashLogReport(crashShortId, crashTimestamp)
  if (res.status === 'error') throwErrorReportSendError(res.error)
  return res.data
}

/**
 * What Flow B auto-sent this run, for the dialog's amend mode: no bundle rebuild, and the
 * manifest and sample lines are the ones that actually shipped. `null` means nothing was
 * auto-sent, so there's nothing to add to.
 */
export async function getAutoSentReportPreview(): Promise<AutoSentReport | null> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- BundleManifest has serde-elided optional fields, which specta can't export as one command return type
  return invoke<AutoSentReport | null>('get_auto_sent_report_preview')
}

/**
 * Add a note (and optionally a reply-to address) to the report Flow B already sent.
 * Returns that report's id. Callable more than once: amendments accumulate server-side,
 * so disable the button while the call is in flight rather than after it returns.
 *
 * Takes no id because there's only ever one stashed report. It throws an
 * `ErrorReportSendFailure` (`notAmendable` when nothing was auto-sent or the server never handed
 * back an amend key); `canAmend` from `getAutoSentReportPreview` is the flag to branch on
 * beforehand.
 */
export async function amendErrorReport(userNote?: string, email?: string): Promise<{ id: string }> {
  const res = await commands.amendErrorReport(userNote ?? null, email ?? null)
  if (res.status === 'error') throwErrorReportSendError(res.error)
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
 * Build the bundle the send would ship, with the note and address as they stand, and write it as
 * one text file for the File viewer. Returns its path and the id it was built under.
 *
 * Pass the preview's `id`, so the file's redaction (keyed by id) matches the send's.
 */
export async function writeErrorReportPreviewFile(
  userNote?: string,
  email?: string,
  id?: string,
): Promise<ReportPreviewFile> {
  const res = await commands.writeErrorReportPreviewFile(userNote ?? null, email ?? null, id ?? null)
  if (res.status === 'error') throwReportPreviewError(res.error)
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

/**
 * Ask the main window to open the compose dialog, pre-filled. Emitted by a window that
 * doesn't host the dialog (the queue window, Settings); `#lib/error-reporter/error-report-flow`
 * decides when.
 */
export function requestErrorReportInMainWindow(request: OpenErrorReport): Promise<void> {
  return events.openErrorReport.emit(request)
}

/** The main window's half of {@link requestErrorReportInMainWindow}. */
export function onErrorReportRequested(handler: (request: OpenErrorReport) => void): Promise<UnlistenFn> {
  return events.openErrorReport.listen((event) => {
    handler(event.payload)
  })
}
