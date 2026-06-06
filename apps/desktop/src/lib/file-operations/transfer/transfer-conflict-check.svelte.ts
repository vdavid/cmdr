/**
 * Reactive conflict-check state machine lifted out of `TransferDialog.svelte`.
 *
 * Owns the cheap top-level conflict check that runs in parallel with the
 * (potentially slow) deep scan preview: the conflict / merge-folder counts, the
 * type-mismatch flag, the bulk-skip name list, and the `check()` lifecycle. The
 * dialog assigns the returned promise to its `conflictCheckPromise` so the
 * confirm path can await it (a fast confirm must not dispatch with
 * `conflicts: []` when conflicts exist).
 *
 * The check is a single dest listing, NOT the recursive byte scan, so it stays
 * decoupled from the scan factory — that decoupling is what lets a same-volume
 * move cancel the deep preview while still surfacing "N folders will merge" and
 * the file-policy radios.
 *
 * The factory takes its reactive inputs via getter callbacks (matching the
 * codebase's factory pattern) and exposes state through getters the dialog reads
 * in its markup.
 */

import { scanVolumeForConflicts, type SourceItemInput } from '$lib/tauri-commands'
import { pluralize } from '$lib/utils/pluralize'
import type { Logger } from '$lib/logging/logger'

export interface TransferConflictCheckDeps {
  /** Destination volume id (the volume the dialog currently targets). */
  getSelectedVolumeId: () => string
  /** Source paths being transferred (used for name matching + backend type resolution). */
  getSourcePaths: () => string[]
  /** Current destination path (volume-relative). */
  getEditedPath: () => string
  /** Real source volume id, forwarded so the backend resolves real per-item types + sizes. */
  getSourceVolumeId: () => string
  /** Whether the dialog is being destroyed (the check no-ops once torn down). */
  getDestroyed: () => boolean
  /** Logger for the found-conflicts / failure diagnostics. */
  log: Logger
}

export function createTransferConflictCheck(deps: TransferConflictCheckDeps) {
  // Conflict detection state. `totalConflictCount` is the unbounded count of
  // real conflicts (file clashes + cross-type clashes) for the summary text —
  // must NOT be derived from a capped slice, or the summary misleads the user
  // about how many files will actually be skipped. Dir-vs-dir collisions are
  // NOT conflicts: they always merge silently, so they're surfaced as a
  // separate informational count (`mergeFolderCount`) and never counted here.
  // The conflict names (file + cross-type only, never dir-dir) are forwarded
  // to the backend on confirm so it can bulk-skip them upfront under
  // `Skip all`. We never render per-conflict rows in this dialog, so we don't
  // need to keep the full `VolumeConflictInfo[]` array around.
  let totalConflictCount = $state(0)
  // Count of source folders that will merge into an existing same-named dest
  // folder. Informational only — never a conflict, never a radio count.
  let mergeFolderCount = $state(0)
  // `true` when any real conflict is a cross-type clash (file-vs-folder either
  // direction). Drives the upfront "Overwrite all" red warning, mirroring the
  // per-file dialog's file→folder warning.
  let hasTypeMismatchConflict = $state(false)
  let conflictNames = $state<string[]>([])
  let isCheckingConflicts = $state(false)
  let conflictCheckComplete = $state(false)

  /** Checks for conflicts at the destination. */
  async function check(): Promise<void> {
    if (deps.getDestroyed() || isCheckingConflicts || conflictCheckComplete) return

    isCheckingConflicts = true
    try {
      // Build source item info from the source paths. We extract the
      // filename from each path for name matching. The real per-item
      // `is_directory` and size come from the backend, which resolves
      // them authoritatively from the source volume (one batched stat)
      // when we pass `sourceVolumeId` + `sourcePaths`. We still send
      // placeholders here so name matching works even if that resolution
      // is unavailable (e.g. the source volume vanished).
      const sourcePaths = deps.getSourcePaths()
      const sourceItems: SourceItemInput[] = sourcePaths.map((path) => {
        const name = path.split('/').pop() || path
        return {
          name,
          size: 0,
          modified: null,
          isDirectory: false,
        }
      })

      const foundConflicts = await scanVolumeForConflicts(
        deps.getSelectedVolumeId(),
        sourceItems,
        deps.getEditedPath(),
        deps.getSourceVolumeId(),
        sourcePaths,
      )

      // Classify each collision:
      //  - dir + dir  → a silent merge, not a conflict (informational).
      //  - everything else (file+file, file+dir, dir+file) → a real
      //    conflict the file policy governs.
      // Only real conflicts count toward `totalConflictCount` and feed
      // the bulk-skip name list; dir-dir merges must never enter the file
      // bulk-skip set ("Skip all" must not skip folders wholesale).
      const realConflicts = foundConflicts.filter((c) => !(c.sourceIsDirectory && c.destIsDirectory))
      mergeFolderCount = foundConflicts.length - realConflicts.length
      totalConflictCount = realConflicts.length
      hasTypeMismatchConflict = realConflicts.some((c) => c.sourceIsDirectory !== c.destIsDirectory)
      conflictNames = realConflicts.map((c) => c.sourcePath)
      conflictCheckComplete = true

      if (totalConflictCount > 0 || mergeFolderCount > 0) {
        deps.log.info('Found {count} {conflictsNoun} and {merges} folder merges at destination', {
          count: totalConflictCount,
          conflictsNoun: pluralize(totalConflictCount, 'conflict'),
          merges: mergeFolderCount,
        })
      }
    } catch (err) {
      deps.log.error('Failed to check for conflicts: {error}', { error: err })
      // Don't block the operation on conflict check failure
      conflictCheckComplete = true
    } finally {
      isCheckingConflicts = false
    }
  }

  return {
    check,
    get totalConflictCount() {
      return totalConflictCount
    },
    get mergeFolderCount() {
      return mergeFolderCount
    },
    get hasTypeMismatchConflict() {
      return hasTypeMismatchConflict
    },
    get conflictNames() {
      return conflictNames
    },
    get isCheckingConflicts() {
      return isCheckingConflicts
    },
    get conflictCheckComplete() {
      return conflictCheckComplete
    },
  }
}
