/**
 * Reactive "does the destination folder exist yet?" check for `TransferDialog`.
 *
 * Drives the yellow "this folder will be created" warning. Structural validation
 * (empty, absolute, length) stays pure-frontend and per-keystroke; THIS check is
 * the one async piece, so it's debounced and decoupled: an `$effect` re-runs it
 * whenever the destination path or volume changes, but only after the user pauses
 * typing. A monotonic sequence drops a slow probe that lands after a newer
 * keystroke, and a timeout (hung mount) is treated as inconclusive — we stay quiet
 * rather than promise a create we can't confirm.
 *
 * Mirrors the factory pattern of `transfer-conflict-check.svelte.ts`: reactive
 * inputs arrive via getter callbacks, state is read through a getter, and the
 * internal `$effect` lands in the effect-tracking context because the dialog
 * creates this synchronously at component init.
 */

import { pathExistsChecked } from '$lib/tauri-commands'
import { validateDirectoryPath } from '$lib/utils/filename-validation'
import { createDebounce } from '$lib/utils/timing'
import type { Logger } from '$lib/logging/logger'

export interface TransferDestExistsCheckDeps {
  /** Current destination path (volume-relative, or `~`-rooted for the home volume). */
  getEditedPath: () => string
  /** Destination volume id (the backend expands `~` for the local `root` volume). */
  getSelectedVolumeId: () => string
  /** Whether the dialog is being destroyed (a late probe no-ops once torn down). */
  getDestroyed: () => boolean
  /** Logger for the probe-failure diagnostic. */
  log: Logger
}

export function createTransferDestExistsCheck(deps: TransferDestExistsCheckDeps) {
  // `true` when the resolved destination folder doesn't exist yet. The dialog
  // gates the yellow warning on this AND on there being no red structural error,
  // so the error always wins.
  let targetMissing = $state(false)

  // Monotonic guard: only the newest probe may write `targetMissing`.
  let checkSeq = 0

  async function run(): Promise<void> {
    const seq = ++checkSeq
    const path = deps.getEditedPath()
    const volumeId = deps.getSelectedVolumeId()
    // A structurally invalid path already shows the red error; don't probe it.
    if (validateDirectoryPath(path).severity === 'error') {
      targetMissing = false
      return
    }
    try {
      const result = await pathExistsChecked(path, volumeId)
      // Drop a stale result (a newer keystroke superseded this probe) or one that
      // landed after the dialog closed.
      if (seq !== checkSeq || deps.getDestroyed()) return
      // Warn only on a definitive "missing". A timeout is inconclusive.
      targetMissing = !result.timedOut && !result.data
    } catch (err) {
      if (seq !== checkSeq) return
      targetMissing = false
      deps.log.debug('Destination existence check failed: {error}', { error: err })
    }
  }

  const debounced = createDebounce(() => void run(), 300)

  // Re-run the check whenever the destination path or volume changes.
  $effect(() => {
    void deps.getEditedPath()
    void deps.getSelectedVolumeId()
    debounced.call()
  })

  return {
    get targetMissing() {
      return targetMissing
    },
    /** Cancels a pending debounced probe (call on dialog destroy). */
    cancel() {
      debounced.cancel()
    },
  }
}
