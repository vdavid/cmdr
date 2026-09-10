/**
 * Reactive "does the destination folder exist yet, and does it take writes?" check
 * for `TransferDialog`.
 *
 * Drives the yellow "this folder will be created" warning and the red "nothing can
 * go here" notice. Structural validation (empty, absolute, length) stays
 * pure-frontend and per-keystroke; THIS check is the one async piece, so it's
 * debounced and decoupled: an `$effect` re-runs it whenever the destination path or
 * volume changes, but only after the user pauses typing. A monotonic sequence drops
 * a slow probe that lands after a newer keystroke, and a timeout (hung mount) is
 * treated as inconclusive — we stay quiet rather than promise a create we can't
 * confirm.
 *
 * The write-access answer shows only when the backend can say a folder takes no
 * writes (`destinationWriteAccess`): "can't tell" shows nothing, and the transfer
 * asks again before it writes, which is the answer that refuses.
 *
 * Mirrors the factory pattern of `transfer-conflict-check.svelte.ts`: reactive
 * inputs arrive via getter callbacks, state is read through a getter, and the
 * internal `$effect` lands in the effect-tracking context because the dialog
 * creates this synchronously at component init.
 */

import { destinationWriteAccess, pathExistsChecked } from '$lib/tauri-commands'
import type { UnwritableReason } from '$lib/file-explorer/types'
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

  // `true` when the resolved destination path DEFINITIVELY exists (not a timeout).
  // Compress uses this for its overwrite warning: the target is a new zip FILE, so
  // "already exists" is the noteworthy case, the inverse of copy/move's "will be
  // created". A timeout leaves both flags false (inconclusive).
  let targetExists = $state(false)

  // Why the destination folder takes no writes, or `null` when it takes them or
  // the backend can't tell. ❗ Only a definite "no" sets it.
  let refusal = $state<UnwritableReason | null>(null)

  // Monotonic guard: only the newest probe may write the state above.
  let checkSeq = 0

  async function run(): Promise<void> {
    const seq = ++checkSeq
    const path = deps.getEditedPath()
    const volumeId = deps.getSelectedVolumeId()
    // A structurally invalid path already shows the red error; don't probe it.
    if (validateDirectoryPath(path).severity === 'error') {
      targetMissing = false
      targetExists = false
      refusal = null
      return
    }
    try {
      // Both at once. A failed write-access probe says nothing (`null`), ❌ never
      // takes the existence answer down with it.
      const [result, access] = await Promise.all([
        pathExistsChecked(path, volumeId),
        destinationWriteAccess(volumeId, path).catch((err: unknown) => {
          deps.log.debug('Destination write-access check failed: {error}', { error: err })
          return null
        }),
      ])
      // Drop a stale result (a newer keystroke superseded this probe) or one that
      // landed after the dialog closed.
      if (seq !== checkSeq || deps.getDestroyed()) return
      // Warn only on a definitive answer. A timeout is inconclusive (both false).
      targetMissing = !result.timedOut && !result.data
      targetExists = !result.timedOut && result.data
      refusal = access?.kind === 'unwritable' ? access.reason : null
    } catch (err) {
      if (seq !== checkSeq) return
      targetMissing = false
      targetExists = false
      refusal = null
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
    get targetExists() {
      return targetExists
    },
    /** Why the destination folder takes no writes, or `null` (takes them, or can't tell). */
    get refusal() {
      return refusal
    },
    /**
     * One-shot, non-debounced existence probe for the compress auto-confirm gate:
     * MCP auto-confirm must not silently overwrite an existing archive, so it needs
     * a deterministic answer at confirm time rather than the debounced reactive
     * state. Returns `true` when the target exists OR the answer is indeterminate
     * (timeout / error) — the conservative choice, since "proceed only when it
     * definitely doesn't exist" must NOT proceed on uncertainty.
     */
    async probeExists(): Promise<boolean> {
      const path = deps.getEditedPath()
      if (validateDirectoryPath(path).severity === 'error') return false
      try {
        const result = await pathExistsChecked(path, deps.getSelectedVolumeId())
        return result.timedOut || result.data
      } catch (err) {
        deps.log.debug('Destination existence probe failed: {error}', { error: err })
        return true
      }
    },
    /** Cancels a pending debounced probe (call on dialog destroy). */
    cancel() {
      debounced.cancel()
    },
  }
}
