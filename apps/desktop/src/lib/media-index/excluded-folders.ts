// FE-owned media-index folder exclusion (the privacy veto): the set of absolute OS
// folder paths the user excluded from image indexing. Persisted as a real JSON array in
// the sparse settings store (the Rust loader reads `mediaIndex.excludedFolders` as
// `Vec<String>`) AND live-applied through `media_index_set_excluded_folder`, both in one
// place so the persisted array and the running scheduler config never drift.
//
// Mirrors `network-volume-prefs.ts`: co-locating persist + IPC (rather than routing
// through `settings-applier.ts`) because the setter takes a per-item delta (`folder`,
// `excluded`), not a whole-array push, so it doesn't fit the applier's passthrough
// table. The trigger is the folder context-menu exclude/un-exclude item, whose click
// arrives as the `media-index-folder-exclusion` event (wired in the main route's
// `setupMenuListeners`).

import { getSetting, setSetting } from '$lib/settings'
import { mediaIndexSetExcludedFolder } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { toggleInArray } from './network-volume-prefs'

const log = getAppLogger('media-index')

/** Absolute OS folder paths excluded from image indexing. */
function getExcludedFolders(): string[] {
  return getSetting('mediaIndex.excludedFolders')
}

/**
 * Exclude (or re-include) a folder from image indexing. Persists the array AND
 * live-applies via IPC. Excluding also retro-deletes the folder's already-indexed rows
 * backend-side (the privacy veto is immediate, not "eventually"); un-excluding just
 * clears the veto (no re-delete, no auto re-enrich). On IPC failure the persisted value
 * rolls back so the setting and backend stay in agreement.
 */
export async function setFolderExcluded(folder: string, excluded: boolean): Promise<void> {
  const previous = getExcludedFolders()
  setSetting('mediaIndex.excludedFolders', toggleInArray(previous, folder, excluded))
  try {
    await mediaIndexSetExcludedFolder(folder, excluded)
  } catch (err) {
    setSetting('mediaIndex.excludedFolders', previous)
    log.warn('Failed to apply folder exclusion for {folder}: {err}', { folder, err: String(err) })
    throw err
  }
}
