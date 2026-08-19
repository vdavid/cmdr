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
 * It matches by NAME, so a copy into the folder the sources already live in
 * would report every one of them as its own conflict. The backend drops those
 * before answering (`commands/file_system/volume_copy.rs`, and
 * `src-tauri/src/file_system/write_operations/transfer/DETAILS.md`
 * § "Self-collision (duplicating in place)"), so a same-folder copy correctly
 * arrives here as zero conflicts. Nothing to do on this side EXCEPT forward what
 * the backend needs to decide it: `sourceVolumeId` and `sourcePaths`. Without
 * them the filter goes inert, the count comes back, and the radios with it —
 * pinned by `transfer-conflict-check.svelte.test.ts`.
 *
 * The factory takes its reactive inputs via getter callbacks (matching the
 * codebase's factory pattern) and exposes state through getters the dialog reads
 * in its markup.
 */

import { scanVolumeForConflicts, type SourceItemInput } from '$lib/tauri-commands'
import { pluralize } from '$lib/utils/pluralize'
import { withTimeout } from '$lib/utils/timing'
import type { Logger } from '$lib/logging/logger'

/** The frontend's own bound on the check, in ms.
 *
 *  The backend already answers or gives up within 30 s (`scan_volume_for_conflicts`),
 *  so this is the second layer the codebase asks for on any path that can reach
 *  a wedged volume: it catches an IPC call that never comes back at all. Sized
 *  just above the backend's budget, so the backend's better-informed answer
 *  wins every time it has one. */
const CONFLICT_CHECK_TIMEOUT_MS = 35_000

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
  // Where the check has got to. `unknown` is the state that must never be
  // collapsed into `answered`: this feeds a data-destroying decision, and an
  // empty conflict list from a check that never ran looks exactly like a clean
  // destination.
  let status = $state<'idle' | 'checking' | 'answered' | 'unknown'>('idle')

  /** Checks for conflicts at the destination. */
  async function check(): Promise<void> {
    if (deps.getDestroyed() || status !== 'idle') return

    status = 'checking'
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

      // `null` is the fallback, and no real answer can be null, so it reads
      // unambiguously as "the call never came back".
      const foundConflicts = await withTimeout(
        scanVolumeForConflicts(
          deps.getSelectedVolumeId(),
          sourceItems,
          deps.getEditedPath(),
          deps.getSourceVolumeId(),
          sourcePaths,
        ),
        CONFLICT_CHECK_TIMEOUT_MS,
        null,
      )
      if (foundConflicts === null) {
        deps.log.warn('The conflict check did not come back within {ms}ms', { ms: CONFLICT_CHECK_TIMEOUT_MS })
        status = 'unknown'
        return
      }

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

      if (totalConflictCount > 0 || mergeFolderCount > 0) {
        deps.log.info('Found {count} {conflictsNoun} and {merges} folder merges at destination', {
          count: totalConflictCount,
          conflictsNoun: pluralize(totalConflictCount, 'conflict'),
          merges: mergeFolderCount,
        })
      }
      // Last, so nothing between here and the answer can throw us into `unknown`
      // after we already have one.
      status = 'answered'
    } catch (err) {
      // The check couldn't run. It doesn't block the transfer — the backend
      // arbitrates every clash it meets at write time — but it must NOT be
      // recorded as an answer: "no conflicts found" and "nobody looked" are
      // different things to show someone about to overwrite their files.
      deps.log.error('Could not check for conflicts: {error}', { error: err })
      status = 'unknown'
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
      return status === 'checking'
    },
    /** The check ran and this is what it found. */
    get conflictCheckComplete() {
      return status === 'answered'
    },
    /** The check couldn't run, so nothing is known about the destination. */
    get conflictCheckUnknown() {
      return status === 'unknown'
    },
  }
}
