/**
 * The `operation-conflict` preview: the gallery's one REAL-OPERATION row.
 *
 * `OperationConflictDialog` takes no props and shows whatever clash the main
 * window's conflict host is holding, and only a live operation parked on a real
 * `write-conflict` gives it one: the line naming which transfer is asking, the
 * Cancel / Rollback row, and every answer all read that operation. So the
 * preview starts one. It copies a fixture item onto a same-named entry in the
 * conflict fixture's destination under "Ask for each", through the app's own
 * `dispatchTransferOperation`, and claims no progress dialog, which is exactly
 * the shape of a copy sent to the queue: the host owns the clash and asks.
 *
 * What the reviewer answers really happens, inside the throwaway fixture tree.
 * The next trigger's fixture call puts the destination back (`dev_fixtures.rs`).
 */

import { dispatchTransferOperation } from '#lib/file-operations/transfer/transfer-dispatch.js'
import { DEFAULT_VOLUME_ID } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/toast-store.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { closeGalleryDialog } from './gallery-state.svelte'
import type { FixtureDirPayload } from './disk-fixture'
import { operationConflictFixtures } from './fixtures/operation-conflict'

const log = getAppLogger('dialogGallery')

/** What a trigger did, so the caller (and the test) can see which branch ran. */
export type OperationConflictPreviewOutcome =
  /** The copy is running and will park on the clash; the host opens the prompt. */
  | { kind: 'started'; operationId: string }
  /** No fixture tree came with the trigger, so nothing was started. */
  | { kind: 'no-fixtures' }
  /** The backend refused to start the copy; the reviewer was told. */
  | { kind: 'refused' }
  /** A state id the row doesn't advertise. Same "open nothing" rule as a missing fixture. */
  | { kind: 'unknown-state' }

/** Starts the real copy whose clash raises the conflict prompt. */
export async function openOperationConflictPreview(
  stateId: string,
  fixtures: FixtureDirPayload | null,
): Promise<OperationConflictPreviewOutcome> {
  const pick = operationConflictFixtures[stateId]
  if (pick === undefined) return { kind: 'unknown-state' }
  if (fixtures === null) return { kind: 'no-fixtures' }

  // A previewed dialog would sit underneath this one: the prompt isn't the
  // gallery's own preview and closes on its own terms.
  closeGalleryDialog()

  const { sourcePath, destinationDir } = pick(fixtures.conflictPreview)
  try {
    const { operationId } = await dispatchTransferOperation({
      operationType: 'copy',
      sourcePaths: [sourcePath],
      destinationPath: destinationDir,
      previewId: null,
      sourceVolumeId: DEFAULT_VOLUME_ID,
      destVolumeId: DEFAULT_VOLUME_ID,
      conflictResolution: 'stop',
      initiator: 'user',
    })
    return { kind: 'started', operationId }
  } catch (error) {
    log.warn('Dialog gallery: the conflict preview copy did not start: {error}', { error })
    addToast('The conflict preview couldn’t start its copy. The app log has the reason.', {
      level: 'info',
      timeoutMs: 8000,
    })
    return { kind: 'refused' }
  }
}
