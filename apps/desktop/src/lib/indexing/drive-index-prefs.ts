// FE-owned drive-indexing preferences that the backend doesn't read: the
// per-drive "don't ask again" silences (D6) and the one-time stale-dialog
// one-shot (D2). Persisted as settings (the one-shot is a row in Advanced's
// "Hints and warnings" card), so they survive restarts and sync across windows. Pure-ish wrappers over the
// settings store keep the JSON-array plumbing in one place.

import { getSetting, setSetting } from '#lib/settings/index.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('indexing')

/** A stored value as the drive IDs it validly holds, and whether it held anything else. */
function parseSilencedDrives(raw: string): { drives: string[]; clean: boolean } {
  try {
    const parsed: unknown = JSON.parse(raw)
    if (Array.isArray(parsed)) {
      const drives = parsed.filter((v): v is string => typeof v === 'string')
      return { drives, clean: drives.length === parsed.length }
    }
  } catch {
    // Not JSON at all: repaired like any other value that isn't an array.
  }
  return { drives: [], clean: false }
}

/** Whether the repair of a corrupt value is already on its way. */
let repairScheduled = false

/**
 * Parse the silenced-drives JSON array, repairing a corrupt value.
 *
 * A corrupt value is rewritten with the valid IDs it held, so it warns once
 * rather than on every read. ❗ The write waits for a microtask: a reader can be a
 * `$derived` (`search/coverage-cta.svelte.ts`), where a synchronous settings write
 * is an unsafe state mutation.
 */
export function getSilencedDrives(): string[] {
  const { drives, clean } = parseSilencedDrives(getSetting('indexing.silencedDrives'))
  if (!clean && !repairScheduled) {
    repairScheduled = true
    log.warn('Corrupt indexing.silencedDrives value; rewriting it with only its valid drive IDs')
    queueMicrotask(() => {
      repairScheduled = false
      const current = parseSilencedDrives(getSetting('indexing.silencedDrives'))
      if (!current.clean) setSetting('indexing.silencedDrives', JSON.stringify(current.drives))
    })
  }
  return drives
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

/**
 * Clear the one-shot so the dialog can fire again.
 *
 * DEV-ONLY, and the app itself never calls it: in the product, only the flag's
 * row in Settings > Advanced clears it (through the store, not here). The dialog
 * gallery's `drive-index-stale` row calls it before every trigger, since the
 * dialog stamps the flag the moment it shows and would otherwise be a
 * single-use preview.
 */
export function resetFirstStaleDialogShown(): void {
  setSetting('indexing.firstStaleDialogShown', false)
}
