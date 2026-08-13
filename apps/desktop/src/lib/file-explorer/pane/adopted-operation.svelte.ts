/**
 * The progress dialog's ADOPTED arm: an operation this window never started,
 * shown because the user pressed Show on its queue row
 * (`$lib/file-operations/queue/DETAILS.md` § Show).
 *
 * This module owns the adopted slot outright, and it is a different module from
 * the one holding birth context ON PURPOSE. `handleTransferError`'s archive
 * branch takes the progress dialog down while keeping birth context alive, and
 * the password submit re-dispatches from it; an adoption that could write those
 * props would send the ADOPTED operation's sources to the ADOPTED operation's
 * destination. Here the birth slot is simply not in scope: this factory is
 * handed a read-only `hasBirthContext()` and nothing else, so the wrong write is
 * unreachable rather than merely unwritten.
 *
 * For the same reason the factory is built without `transfer-pane-effects.ts`.
 * An adopted view has no birth context to act on — no `sourcePaths`, no
 * `sourcePaneSide`, no per-type counts — so its outcomes touch NO pane: no
 * refresh, no selection change, no operation snapshot. The counts still reach
 * the toast (those are facts about the OPERATION, off the completion event); it
 * falls back to the file count instead of the per-type split.
 *
 * `DETAILS.md` § "Birth context".
 */

import { getForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { composeTransferCompleteToast } from '$lib/file-operations/transfer/transfer-complete-toast'
import { getAppLogger } from '$lib/logging/logger'
import { formatByteSize } from '$lib/units'
import { transferOpLabel } from './transfer-op-label'
import type { TransferOperationType, WriteOperationError } from '../types'
import type { AdoptedOperationData, ForegroundOperationVerdict } from './dialog-props'

const log = getAppLogger('fileExplorer')

export interface AdoptedOperationDeps {
  /** Whether this window is already committed to an operation it STARTED.
   *  Read-only by design: see the module doc. */
  hasBirthContext: () => boolean
  /** Whether any dialog is on screen. Half of the occupancy test; the other half
   *  is the two slots, because an archive-password prompt keeps birth context
   *  alive with the progress dialog unmounted. */
  anyDialogOpen: () => boolean
  /** Mounts or unmounts the shared progress dialog. */
  setProgressDialogShown: (shown: boolean) => void
  /** Opens the error dialog, claiming `failedOperationId` for the failure slot. */
  openTransferError: (
    operationType: TransferOperationType,
    error: WriteOperationError,
    failedOperationId: string | null,
  ) => void
  onRefocus: () => void
}

export function createAdoptedOperation(deps: AdoptedOperationDeps) {
  let adoptedProps = $state<AdoptedOperationData | null>(null)

  /** Closes the dialog and frees the slot, the tail all four outcomes share. */
  function settle(): void {
    deps.setProgressDialogShown(false)
    adoptedProps = null
  }

  return {
    get props(): AdoptedOperationData | null {
      return adoptedProps
    },

    /** Whether an adopted operation occupies the progress dialog. */
    isShowing(): boolean {
      return adoptedProps !== null
    },

    /**
     * Shows an operation that is already running (the queue row's Show button).
     *
     * The dialog slot is single-occupancy and refusing is the honest answer when
     * it is taken: swapping would either drop a transfer's dialog out from under
     * the user or, worse, land next to a live birth context. "Taken" includes
     * the case where nothing is on screen — an archive-password prompt keeps
     * birth context alive with the progress dialog unmounted — which is why
     * `anyDialogOpen()` is only half the test.
     */
    foregroundOperation(operation: AdoptedOperationData): ForegroundOperationVerdict {
      if (adoptedProps?.operationId === operation.operationId) {
        // Pressing Show on the operation already up. Deliberately not a
        // re-adoption: replacing the props remounts the dialog, which disposes
        // its session and builds a second one, whose ETA smoother would start
        // over from nothing halfway through the transfer.
        return 'alreadyShowing'
      }
      if (deps.hasBirthContext() || adoptedProps !== null || deps.anyDialogOpen()) {
        log.info('Not showing op={operationId}: this window is busy with another dialog', {
          operationId: operation.operationId,
        })
        addToast(tString('fileOperations.transferProgress.foregroundBusyToast'), { level: 'info' })
        return 'busy'
      }
      log.info('Showing op={operationId} in the progress dialog', { operationId: operation.operationId })
      adoptedProps = operation
      deps.setProgressDialogShown(true)
      return 'adopted'
    },

    /** Hands an adopted operation back to the queue window, if one is being shown.
     *  Birth wins over adoption: a new operation is the user's fresh intent, and
     *  the adopted one is still running and still listed in the queue, which is
     *  where it came from. Without this the two would stack, since a dialog for an
     *  operation this window STARTED renders from the other slot. */
    release(): void {
      if (!adoptedProps) return
      log.info('Handing op={operationId} back to the queue window: this window is starting another operation', {
        operationId: adoptedProps.operationId,
      })
      adoptedProps = null
    },

    /** Clears the slot with no logging, for the render-failure sweep. */
    forget(): void {
      adoptedProps = null
    },

    /** An adopted operation finished. It says what the operation did and stops
     *  there: what a pane should do about a transfer belongs to the view that
     *  started it. */
    handleComplete(filesProcessed: number, filesSkipped: number, bytesProcessed: number): void {
      const op = adoptedProps?.operationType ?? 'copy'
      log.info(
        `${transferOpLabel(op)} complete (adopted): ${String(filesProcessed)} files (${String(filesSkipped)} skipped, ${formatByteSize(bytesProcessed)})`,
      )
      const toastMessage = composeTransferCompleteToast({ operationType: op, filesProcessed, filesSkipped })
      const allSkipped = filesSkipped > 0 && filesSkipped === filesProcessed
      addToast(toastMessage, { level: allSkipped ? 'info' : 'success', timeoutMs: 7000 })

      settle()
      deps.onRefocus()
    },

    /** An adopted operation was cancelled. No pane work, for the same reason
     *  completion has none: the selection to adjust was never taken here. */
    handleCancelled(filesProcessed: number): void {
      const op = adoptedProps?.operationType ?? 'copy'
      log.info(`${transferOpLabel(op)} cancelled (adopted) after ${String(filesProcessed)} files`)

      settle()
      deps.onRefocus()
    },

    /** An adopted operation couldn't finish. The reason is worth showing exactly
     *  as it is for an operation this window started; only the pane tail is
     *  missing. The failure handover is the same too, so the corner chip and the
     *  toast stay quiet about what the user is already reading. */
    handleError(error: WriteOperationError): void {
      const op = adoptedProps?.operationType ?? 'copy'
      const failedOperationId = getForegroundOperationId()
      log.error('{op} failed (adopted): {errorType}', { op: transferOpLabel(op), errorType: error.type, error })

      settle()
      deps.openTransferError(op, error, failedOperationId)
    },

    /** The user sent an adopted operation back to the queue window (Background,
     *  F2, or a close). Stop showing it; it keeps running. */
    handleQueue(): void {
      if (!adoptedProps) return
      log.info('{op} handed back to the queue window', { op: transferOpLabel(adoptedProps.operationType) })

      settle()
      deps.onRefocus()
    },
  }
}
