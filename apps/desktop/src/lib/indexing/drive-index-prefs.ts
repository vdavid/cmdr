// FE-owned drive-indexing preferences that the backend doesn't read: the
// per-drive "don't ask again" silences (D6) and the one-time stale-dialog
// one-shot (D2). Persisted as hidden settings (like `network.firstTriggerDone`),
// so they survive restarts and sync across windows. Pure-ish wrappers over the
// settings store keep the JSON-array plumbing in one place.

import { getSetting, setSetting } from '$lib/settings'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('indexing')

/** Parse the silenced-drives JSON array, tolerating a corrupt value. */
export function getSilencedDrives(): string[] {
  const raw = getSetting('indexing.silencedDrives')
  try {
    const parsed: unknown = JSON.parse(raw)
    if (Array.isArray(parsed)) return parsed.filter((v): v is string => typeof v === 'string')
  } catch {
    log.warn('Corrupt indexing.silencedDrives value; resetting to empty')
  }
  return []
}

/** Whether the user silenced the first-connect prompt for this drive. */
export function isDriveSilenced(volumeId: string): boolean {
  return getSilencedDrives().includes(volumeId)
}

/** Remember "don't ask again for this drive". Idempotent. */
export function silenceDrive(volumeId: string): void {
  const current = getSilencedDrives()
  if (current.includes(volumeId)) return
  setSetting('indexing.silencedDrives', JSON.stringify([...current, volumeId]))
}

/** Clear every per-drive silence (the "Re-enable notifications for all drives" button). */
export function clearSilencedDrives(): void {
  setSetting('indexing.silencedDrives', '[]')
}

/** Whether at least one drive has been silenced (gates the re-enable button). */
export function hasSilencedDrives(): boolean {
  return getSilencedDrives().length > 0
}

/** Whether the one-time stale dialog (D2) has already fired. */
export function hasShownFirstStaleDialog(): boolean {
  return getSetting('indexing.firstStaleDialogShown')
}

/** Mark the one-time stale dialog as shown so it never fires again. */
export function markFirstStaleDialogShown(): void {
  setSetting('indexing.firstStaleDialogShown', true)
}
