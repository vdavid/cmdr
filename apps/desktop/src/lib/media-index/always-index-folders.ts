// FE-owned media-index CHOSEN folders: the absolute OS folder paths the user picked for
// image indexing. Persisted as a real JSON array in the sparse settings store (the Rust
// loader reads `mediaIndex.alwaysIndexFolders` as `Vec<String>`) AND live-applied through
// `media_index_set_always_index_folder`, both in one place so the persisted array and the
// running scheduler config never drift.
//
// Mirrors `network-volume-prefs.ts` / `excluded-folders.ts`: co-locating persist + IPC
// (rather than routing through `settings-applier.ts`) because the setter takes a per-item
// delta (`folder`, `always`), not a whole-array push, so it doesn't fit the applier's
// passthrough table.
//
// In the "only folders I choose" scope these folders ARE the coverage; in the automatic
// scope they're the escape hatch for a folder importance ranks too low to reach. Adding
// one kicks an immediate indexing pass backend-side.

import { getSetting, setSetting } from '$lib/settings'
import { mediaIndexSetAlwaysIndexFolder } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { toggleInArray } from './network-volume-prefs'

const log = getAppLogger('media-index')

/** The absolute OS folder paths chosen for image indexing, in the order they were added. */
export function getChosenFolders(): string[] {
  return getSetting('mediaIndex.alwaysIndexFolders')
}

/** Whether this exact folder path is already chosen. */
export function isFolderChosen(folder: string): boolean {
  return getChosenFolders().includes(folder)
}

/**
 * Add or remove a chosen folder. Persists the array AND live-applies via IPC (adding
 * kicks an immediate pass backend-side). Removing stops future indexing but deletes
 * nothing: the folder's existing rows stay searchable until the user reclaims the space.
 * On IPC failure the persisted value rolls back so the setting and backend stay in
 * agreement.
 */
export async function setFolderChosen(folder: string, chosen: boolean): Promise<void> {
  const previous = getChosenFolders()
  setSetting('mediaIndex.alwaysIndexFolders', toggleInArray(previous, folder, chosen))
  try {
    await mediaIndexSetAlwaysIndexFolder(folder, chosen)
  } catch (err) {
    setSetting('mediaIndex.alwaysIndexFolders', previous)
    log.warn('Failed to apply chosen folder {folder}: {err}', { folder, err: String(err) })
    throw err
  }
}
