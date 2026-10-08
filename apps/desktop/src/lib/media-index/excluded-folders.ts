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

import { mediaIndexSetExcludedFolder } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { persistThenApply } from './network-volume-prefs'

const log = getAppLogger('media-index')

/**
 * Exclude (or re-include) a folder from image indexing. Persists the array AND
 * live-applies via IPC. Excluding also purges the folder's already-indexed rows
 * backend-side; a purge that doesn't land (a full disk, a locked database) stays owed and
 * retries quietly on the volume's next pass, so nothing here waits on it or reports it.
 * Un-excluding just clears the veto (no re-delete, no auto re-enrich).
 *
 * The persisted value is NEVER rolled back, even when the call rejects. The command sets
 * the live veto before anything else and has no error of its own, so a rejection only
 * means the call didn't arrive, and every launch seeds the veto from this value. A
 * rollback would un-exclude a folder the running app may still be treating as excluded.
 */
export async function setFolderExcluded(folder: string, excluded: boolean): Promise<void> {
  try {
    await persistThenApply({ setting: 'mediaIndex.excludedFolders', id: folder, on: excluded, rollback: false }, () =>
      mediaIndexSetExcludedFolder(folder, excluded),
    )
  } catch (err) {
    log.warn(
      'Folder exclusion for {folder} is saved but did not reach the backend; the next launch applies it: {err}',
      {
        folder,
        err: String(err),
      },
    )
    throw err
  }
}
