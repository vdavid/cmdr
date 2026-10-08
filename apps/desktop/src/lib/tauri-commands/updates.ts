// macOS custom-updater commands: check / download / install, preserving TCC /
// Full Disk Access by syncing files into the existing `.app` bundle (see
// `$lib/updates/updater.svelte.ts` for the full flow, including the non-macOS
// Tauri-plugin fallback). Plus the cross-platform background-check schedule.

import { commands, type BundleWriteBlocker, type UpdateCheckOutcome, type UpdateCheckTrigger } from '$lib/ipc/bindings'
import { throwServerRequestError } from '$lib/error-messages/server-request'
import { UpdateDownloadFailure } from '$lib/updates/update-download-failure'
import { UpdateInstallFailure } from '$lib/updates/update-install-failure'
import { throwIpcError } from './ipc-types'

export type { BundleWriteBlocker, UpdateCheckOutcome, UpdateCheckTrigger }

/**
 * Whether the running bundle sits somewhere an update can be written into, or `null` when nothing
 * is in the way. Asked once an update is found and before the download starts: an install that
 * can't write its own bundle would otherwise pull ~63 MB and rewrite nothing, every poll interval.
 */
export async function updateWriteBlocker(): Promise<BundleWriteBlocker | null> {
  const res = await commands.updateWriteBlocker()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Fetches `latest.json` and answers what it found, with the organization's policy applied (the backend decides; a
 * managed outcome is an answer, not a failure). `trigger` tells the backend whether this is a background check. A check
 * that doesn't land throws a `ServerRequestFailure`, which the updater words and logs at the level it earns.
 */
export async function checkForUpdate(trigger: UpdateCheckTrigger): Promise<UpdateCheckOutcome> {
  const res = await commands.checkForUpdate(trigger)
  if (res.status === 'error') throwServerRequestError(res.error)
  return res.data
}

/**
 * Downloads the update the last check offered and verifies its minisign signature. A download that doesn't land throws
 * an `UpdateDownloadFailure`, which the updater logs at the level it earns.
 */
export async function downloadUpdate(): Promise<void> {
  const res = await commands.downloadUpdate()
  if (res.status === 'error') throw new UpdateDownloadFailure(res.error)
}

/**
 * Installs the downloaded update by syncing files into the running `.app` bundle. A refusal or failure throws an
 * `UpdateInstallFailure`.
 */
export async function installUpdate(): Promise<void> {
  const res = await commands.installUpdate()
  if (res.status === 'error') throw new UpdateInstallFailure(res.error)
}

/**
 * Milliseconds until the background update check is due for an interval of `intervalMs` (0 = now), or `null` when
 * the backend couldn't say. The backend remembers the last answered check across relaunches, so a relaunch within the
 * interval doesn't check again. Every platform, unlike the three commands above.
 */
export async function updateCheckDueIn(intervalMs: number): Promise<number | null> {
  try {
    return await commands.updateCheckDueIn(intervalMs)
  } catch {
    return null
  }
}

/**
 * Tells the backend a check finished: `answered` when the update server replied, whatever it said. Best-effort: a
 * failed record only means the next wake asks a stale schedule.
 */
export async function recordUpdateCheck(answered: boolean): Promise<void> {
  try {
    await commands.recordUpdateCheck(answered)
  } catch {
    // Nothing to do: the schedule is a courtesy to the network, never a gate on updating.
  }
}
