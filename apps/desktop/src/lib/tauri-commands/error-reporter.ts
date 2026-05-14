// Error reporter commands (Flow A: user-initiated)

import { invoke } from '@tauri-apps/api/core'
import { commands } from '$lib/ipc/bindings'
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
 */
export async function prepareErrorReportPreview(userNote?: string): Promise<PreviewPayload> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- BundleManifest contains Breadcrumb.ctx: Option<serde_json::Value>, which specta can't represent; excluded from typed bindings
  return invoke<PreviewPayload>('prepare_error_report_preview', { userNote })
}

/**
 * Re-build the bundle and upload it. Returns the server-issued ID.
 * Display the returned `id` to the user, not the one from `prepareErrorReportPreview`.
 */
export async function sendErrorReport(userNote?: string): Promise<{ id: string }> {
  const res = await commands.sendErrorReport(userNote ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Debug-only: write the bundle to the app data dir and return the path.
 * In production the command isn't registered, so calling it returns an error.
 */
export async function saveErrorReportToDisk(userNote?: string): Promise<string> {
  const res = await commands.saveErrorReportToDisk(userNote ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
