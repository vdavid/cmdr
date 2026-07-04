// macOS custom-updater commands: check / download / install, preserving TCC /
// Full Disk Access by syncing files into the existing `.app` bundle (see
// `$lib/updates/updater.svelte.ts` for the full flow, including the non-macOS
// Tauri-plugin fallback).

import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

/** Metadata for an available update. */
export interface UpdateCheckResult {
  version: string
  url: string
  signature: string
}

/** Fetches `latest.json` and returns update info if a newer version is available, else `null`. */
export async function checkForUpdate(): Promise<UpdateCheckResult | null> {
  const res = await commands.checkForUpdate()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Downloads the update tarball and verifies its minisign signature. */
export async function downloadUpdate(url: string, signature: string): Promise<void> {
  const res = await commands.downloadUpdate(url, signature)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Installs a previously downloaded update by syncing files into the running `.app` bundle. */
export async function installUpdate(): Promise<void> {
  const res = await commands.installUpdate()
  if (res.status === 'error') throwIpcError(res.error)
}
