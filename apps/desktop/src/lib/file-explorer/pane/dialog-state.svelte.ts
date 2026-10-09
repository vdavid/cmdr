/**
 * Every dialog `DualPaneExplorer` can put on screen, and what each one does when
 * it settles.
 *
 * This module owns BIRTH CONTEXT: the operation this window started, the paths
 * it started with, and the pane work that follows from them. The three pieces it
 * composes each own one thing it deliberately cannot:
 *
 * - `transfer-pane-effects.ts`: everything a settled transfer does to the panes.
 * - `adopted-operation.svelte.ts`: the progress dialog's other arm, an operation
 *   this window only watches. It is built WITHOUT the pane effects and without
 *   any way to write birth context — see its module doc for why that is the one
 *   hazard in this feature.
 * - `archive-password-flow.svelte.ts`: the password prompt and its two flows.
 *
 * Shapes live in `dialog-props.ts`; the architecture is `DETAILS.md` § "Birth
 * context".
 */

import { onDirectoryDiff, findFileIndex } from '$lib/tauri-commands'
import { dismissFailedOperation } from '$lib/tauri-commands'
import {
  getForegroundFailureId,
  getForegroundOperationId,
  setForegroundFailureId,
} from '$lib/file-operations/foreground-operation.svelte'
import { addToast, type ToastLevel } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import {
  composeTransferCompleteToast,
  composeTrashRefusedToast,
} from '$lib/file-operations/transfer/transfer-complete-toast'
import { getTechnicalDetails } from '$lib/file-operations/transfer/transfer-error-messages'
import TrashCompleteToastContent from '$lib/file-operations/delete/TrashCompleteToastContent.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import { pathCrossesArchiveBoundary } from './archive-paths'
import { transferOpLabel } from './transfer-op-label'
import { createTransferPaneEffects } from './transfer-pane-effects'
import { createAdoptedOperation } from './adopted-operation.svelte'
import { createArchivePasswordFlow } from './archive-password-flow.svelte'
import { createProgrammaticConfirm } from './programmatic-confirm'
import { skippedDeleteConfirmation, skippedTransferConfirmation } from './confirmation-skip'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import { openRenameOnDuplicate } from './duplicate-rename'
import type { TransferDialogPropsData } from './transfer-operations'
import type { TransferOperationType, WriteOperationError } from '../types'
import type {
  AdoptedOperationData,
  AlertDialogPropsData,
  DeleteDialogPropsData,
  DialogStateDeps,
  ForegroundOperationVerdict,
  NewFileDialogPropsData,
  NewFolderDialogPropsData,
  OperationStartVerdict,
  TransferCompletePayload,
  TransferConfirmPayload,
  TransferErrorPropsData,
  TransferProgressPropsData,
} from './dialog-props'
import type { ProgressAtStop } from '$lib/tauri-commands'
import { isAnySoftDialogOpen } from '$lib/ui/open-dialogs.svelte'
import { announceOperationBlocked } from './operation-start-gate'
import type { SoftDialogId } from '$lib/ui/dialog-registry'
import { formatByteSize } from '$lib/units'

const log = getAppLogger('fileExplorer')

/**
 * The birth context a Retry dispatches: the same sources, destination, and
 * conflict policy, as a NEW operation.
 *
 * Three fields can't carry over. `previewId`: the backend refuses a second claim
 * on one preview. `preKnownConflicts`: the failed run may have landed files, so
 * the old pre-flight's list is stale, and the backend finds conflicts itself
 * without one. `mcpRequestId` and `initiator`: the click is the user's, and an
 * MCP round-trip that already got its answer mustn't get a second one.
 */
export function retryPropsFrom(props: TransferProgressPropsData): TransferProgressPropsData {
  return { ...props, previewId: null, preKnownConflicts: undefined, mcpRequestId: undefined, initiator: 'user' }
}

export function createDialogState(deps: DialogStateDeps) {
  // Transfer dialog state (copy/move)
  let showTransferDialog = $state(false)
  let transferDialogProps = $state<TransferDialogPropsData | null>(null)

  // The progress dialog's BIRTH slot: what this window started, and what it may
  // therefore do to its panes afterwards. The adopted slot is a different
  // module's `$state` (`adopted-operation.svelte.ts`), which is what makes it
  // impossible for an adoption to overwrite the input the archive-password
  // submit re-dispatches from — a wrong write against the user's files.
  // `DETAILS.md` § "Birth context".
  let showTransferProgressDialog = $state(false)
  let transferProgressProps = $state<TransferProgressPropsData | null>(null)

  // New folder dialog state
  let showNewFolderDialog = $state(false)
  let newFolderDialogProps = $state<NewFolderDialogPropsData | null>(null)

  // New file dialog state
  let showNewFileDialog = $state(false)
  let newFileDialogProps = $state<NewFileDialogPropsData | null>(null)

  // Alert dialog state
  let showAlertDialog = $state(false)
  let alertDialogProps = $state<AlertDialogPropsData | null>(null)

  // Transfer error dialog state
  let showTransferErrorDialog = $state(false)
  let transferErrorProps = $state<TransferErrorPropsData | null>(null)

  // Delete dialog state
  let showDeleteDialog = $state(false)
  let deleteDialogProps = $state<DeleteDialogPropsData | null>(null)

  const paneEffects = createTransferPaneEffects(deps, () => transferProgressProps)

  /**
   * Whether anything is on screen. The main window shows one dialog at a time, so
   * this is what an adoption has to find empty.
   *
   * The INVENTORY is `$lib/ui/open-dialogs.svelte`, which `ModalDialog` maintains
   * from its own mount/destroy pair: exhaustive by construction, so a dialog added
   * anywhere in this window counts here without anyone remembering to say so. The
   * local flags below are not a second inventory — they close the same-tick window
   * between `show* = true` and the mount that registers it, which is when a
   * back-to-back MCP dispatch would otherwise see an empty set.
   */
  function anyDialogOpen(): boolean {
    return (
      isAnySoftDialogOpen() ||
      showTransferDialog ||
      showTransferProgressDialog ||
      showNewFolderDialog ||
      showNewFileDialog ||
      showAlertDialog ||
      showTransferErrorDialog ||
      archivePassword.showDialog ||
      showDeleteDialog
    )
  }

  /**
   * The dialog holding the progress slot, or `null` when it's free.
   *
   * BIRTH CONTEXT is the whole test, read off the props and ❌ never off
   * `showTransferProgressDialog`. Two consequences worth stating:
   *
   * - An ADOPTED operation doesn't hold the slot. Watching someone else's
   *   operation isn't owning one: birth still wins, the adopted view goes back to
   *   the queue window it came from, and it keeps running. Pinned in
   *   `dialog-state.foreground.svelte.test.ts`.
   * - A password prompt DOES, with nothing on screen but itself: it keeps birth
   *   context alive for the submit to re-dispatch from. It gets named as the
   *   blocker, so an agent isn't told to close a dialog that isn't up.
   */
  function progressSlotHolder(): SoftDialogId | null {
    if (transferProgressProps === null) return null
    return archivePassword.showDialog ? 'archive-password' : 'transfer-progress'
  }

  /** Refuses a start, saying so to whoever asked. The wording and the round-trip
   *  failure live in `operation-start-gate.ts`, shared with the entry-point gate. */
  function refuseOperationStart(blockedBy: SoftDialogId, mcpRequestId: string | undefined): OperationStartVerdict {
    announceOperationBlocked(blockedBy, mcpRequestId)
    return { blockedBy }
  }

  /**
   * What a finished operation leaves on screen.
   *
   * The completion sentence always, and — for a batch trash the OS refused part
   * of — a second, `warn`-level toast naming what stayed behind. Two toasts
   * rather than one clause, because the completion carries Undo and this one
   * carries a warning, and they're read (and dismissed) separately.
   *
   * A trash gets the Undo / "Go to trash" toast and a longer beat to reach them
   * (hovering pauses the timer, so the number is only the window in which the
   * pointer has to arrive). Both actions need the journaled operation, so a trash
   * that somehow has no id falls back to the plain toast rather than offering
   * buttons that would do nothing. 7s elsewhere: the default 4s reads as a
   * flicker for anyone still parsing a mixed sentence's second clause.
   */
  function raiseCompletionToasts(
    op: TransferOperationType,
    settledOperationId: string | null,
    message: string,
    level: ToastLevel,
    refusedMessage: string | null,
  ): void {
    if (op === 'trash' && settledOperationId) {
      addToast(TrashCompleteToastContent, {
        level,
        timeoutMs: 10000,
        props: {
          message,
          operationId: settledOperationId,
          // Only used when the journal recorded no in-trash location, to pick
          // WHICH volume's trash to open. The focused pane is the honest
          // stand-in for a birth context that went missing: it's the pane the
          // trash was started from.
          sourceFolderPath: transferProgressProps?.sourceFolderPath ?? deps.getFocusedPaneRef()?.getCurrentPath() ?? '',
          explorer: deps.getExplorer(),
        },
      })
    } else {
      addToast(message, { level, timeoutMs: 7000 })
    }
    // Last, so it lands newest. 10s to match the trash toast it stands beside.
    if (refusedMessage !== null) {
      addToast(refusedMessage, { level: 'warn', timeoutMs: 10000 })
    }
  }

  /** Opens the error dialog, claiming the failure so the corner chip and the
   *  failure toast stay quiet about what the user is already reading. */
  function openTransferError(
    operationType: TransferOperationType,
    error: WriteOperationError,
    failedOperationId: string | null,
    progressAtStop: ProgressAtStop | null,
    retry: TransferProgressPropsData | null = null,
  ): void {
    setForegroundFailureId(failedOperationId)
    transferErrorProps = { operationType, error, progressAtStop, retry }
    showTransferErrorDialog = true
  }

  /** Hides the error dialog and drops its retained failure from the queue: the
   *  user has read it, whether they then close it or retry. */
  function settleTransferError(): void {
    // The backend retains every failure unconditionally (it can't know a dialog
    // was up), and this is the only place that knows one was. Everything else
    // waits for an explicit Dismiss.
    const failedOperationId = getForegroundFailureId()
    if (failedOperationId !== null) {
      setForegroundFailureId(null)
      void dismissFailedOperation(failedOperationId).catch((err: unknown) => {
        // Nothing to recover: the row simply stays in the queue window, which
        // is a safe place for it to be.
        log.warn('Failed to dismiss the failed operation {operationId}: {error}', {
          operationId: failedOperationId,
          error: err,
        })
      })
    }
    showTransferErrorDialog = false
    transferErrorProps = null
  }

  /** Starts `retry` as a NEW operation through the same start every operation
   *  takes, settling the failed one like a close first. */
  function startAgainFromErrorDialog(retry: TransferProgressPropsData | null, how: string): void {
    settleTransferError()
    if (retry === null) {
      deps.onRefocus()
      return
    }
    const op = retry.operationType
    if (startBirthOperation(retry) === 'started') {
      log.info('{op} {how} from the error dialog', { op: transferOpLabel(op), how })
    } else {
      deps.onRefocus()
    }
  }

  const archivePassword = createArchivePasswordFlow({
    hasBirthContext: () => transferProgressProps !== null,
    redispatchBirthOperation: () => {
      const props = transferProgressProps
      if (!props) return
      // A fresh scan runs, so `previewId` is cleared. ⚠️ It MUST be: the retry is
      // a NEW operation, and the backend refuses a second claim on one preview,
      // so a carried-over id would silently fall back to a full re-walk.
      transferProgressProps = { ...props, previewId: null }
      paneEffects.snapshotSourcePaneSelection()
      showTransferProgressDialog = true
    },
    settleBirthOperation: () => {
      const op = transferProgressProps?.operationType ?? 'copy'
      // Both answers that end WITHOUT a retry land here: a cancel, and an MCP
      // unlock (which stores the password but deliberately starts nothing).
      log.info('{op} archive-password prompt closed with no retry', { op: transferOpLabel(op) })

      paneEffects.refreshPanesAfterTransfer()
      paneEffects.clearSourcePaneAfterTransfer()

      showTransferProgressDialog = false
      transferProgressProps = null
    },
    setProgressDialogShown: (shown) => {
      showTransferProgressDialog = shown
    },
    onRefocus: deps.onRefocus,
  })

  const adopted = createAdoptedOperation({
    hasBirthContext: () => transferProgressProps !== null,
    anyDialogOpen,
    setProgressDialogShown: (shown) => {
      showTransferProgressDialog = shown
    },
    openTransferError,
    onRefocus: deps.onRefocus,
  })

  /**
   * Opens the progress dialog on an operation this window is starting. Hands any
   * adopted operation back to the queue first: birth wins over adoption, and the
   * two arms would otherwise stack.
   *
   * Refuses outright when the slot is already taken. ⚠️ This refusal has to stand
   * on its own, whatever the entry points do: the native menu is OS-side and MCP
   * is a separate actor, and neither is gated on this window's modal state. A
   * second start used to overwrite the first operation's props, so the mounted
   * dialog re-rendered against something it had never dispatched and the user got
   * no operation and no explanation.
   */
  function startBirthOperation(props: TransferProgressPropsData): OperationStartVerdict {
    const blockedBy = progressSlotHolder()
    if (blockedBy) return refuseOperationStart(blockedBy, props.mcpRequestId)

    adopted.release()
    transferProgressProps = props
    paneEffects.snapshotSourcePaneSelection()
    showTransferProgressDialog = true
    return 'started'
  }

  const programmaticConfirm = createProgrammaticConfirm({
    isTransferDialogOpen: () => showTransferDialog && transferDialogProps !== null,
    isDeleteDialogOpen: () => showDeleteDialog && deleteDialogProps !== null,
    isArchivePasswordOpen: () => archivePassword.showDialog,
    supplyStoredPassword: () => {
      archivePassword.supplyStoredPassword()
    },
  })

  const state = {
    // --- Reactive getters for template binding ---
    get showTransferDialog() {
      return showTransferDialog
    },
    get transferDialogProps() {
      return transferDialogProps
    },
    get showTransferProgressDialog() {
      return showTransferProgressDialog
    },
    get transferProgressProps() {
      return transferProgressProps
    },
    get adoptedProgressProps() {
      return adopted.props
    },
    get showNewFolderDialog() {
      return showNewFolderDialog
    },
    get newFolderDialogProps() {
      return newFolderDialogProps
    },
    get showNewFileDialog() {
      return showNewFileDialog
    },
    get newFileDialogProps() {
      return newFileDialogProps
    },
    get showAlertDialog() {
      return showAlertDialog
    },
    get alertDialogProps() {
      return alertDialogProps
    },
    get showTransferErrorDialog() {
      return showTransferErrorDialog
    },
    get transferErrorProps() {
      return transferErrorProps
    },
    get showArchivePasswordDialog() {
      return archivePassword.showDialog
    },
    get archivePasswordProps() {
      return archivePassword.props
    },
    get showDeleteDialog() {
      return showDeleteDialog
    },
    get deleteDialogProps() {
      return deleteDialogProps
    },

    // --- Methods to open dialogs (called from DualPaneExplorer) ---

    showAlert(title: string, message: string, path?: string) {
      alertDialogProps = { title, message, path }
      showAlertDialog = true
    },

    /** Opens the copy/move/compress confirmation, or, with confirmations skipped
     *  (`confirmation-skip.ts`), answers it the way the dialog would have. */
    showTransfer(props: TransferDialogPropsData) {
      transferDialogProps = props
      const skipped = deps.skipsConfirmations() ? skippedTransferConfirmation(props, getVolumes()) : null
      if (skipped) state.handleTransferConfirm(skipped)
      else showTransferDialog = true
    },

    /** Opens the progress dialog directly, skipping the destination picker (used by
     *  clipboard paste). Refuses, and says so, when the slot is taken. */
    startTransferProgress(props: TransferProgressPropsData): OperationStartVerdict {
      return startBirthOperation(props)
    },

    /** Shows an operation that is already running (the queue row's Show button).
     *  Refuses an occupied slot; see `adopted-operation.svelte.ts`. */
    foregroundOperation(operation: AdoptedOperationData): ForegroundOperationVerdict {
      return adopted.foregroundOperation(operation)
    },

    showNewFolder(props: NewFolderDialogPropsData) {
      newFolderDialogProps = props
      showNewFolderDialog = true
    },

    showNewFile(props: NewFileDialogPropsData) {
      newFileDialogProps = props
      showNewFileDialog = true
    },

    /** Opens the delete/trash confirmation, or, with confirmations skipped
     *  (`confirmation-skip.ts`), trashes right away. A permanent delete always asks. */
    showDeleteConfirmation(props: DeleteDialogPropsData) {
      deleteDialogProps = props
      const skipped = deps.skipsConfirmations() ? skippedDeleteConfirmation(props) : null
      if (skipped) state.handleDeleteConfirm(skipped.previewId, skipped.isPermanent)
      else showDeleteDialog = true
    },

    /** Raises the browse-time archive-password prompt (a listing of a
     *  header-encrypted archive). No transfer operation is involved. */
    showArchivePasswordForBrowse(info: {
      volumeId: string
      archivePath: string
      wrongAttempt: boolean
      retry: () => void
    }) {
      archivePassword.promptForBrowse(info)
    },

    // --- Handler functions (passed to DialogManager) ---

    handleTransferConfirm({
      destination,
      volumeId,
      previewId,
      conflictResolution,
      operationType,
      preKnownConflicts,
      newName,
    }: TransferConfirmPayload) {
      if (!transferDialogProps) return

      // A refusal still takes this dialog down (below): the user answered it, and
      // leaving it stacked over the operation it can't join would say nothing. The
      // refusal itself does the talking.
      startBirthOperation({
        operationType,
        sourcePaths: transferDialogProps.sourcePaths,
        sourceFolderPath: transferDialogProps.sourceFolderPath,
        sourcePaneSide: transferDialogProps.direction === 'right' ? 'left' : 'right',
        destinationPath: destination,
        direction: transferDialogProps.direction,
        sortColumn: transferDialogProps.sortColumn,
        sortOrder: transferDialogProps.sortOrder,
        previewId,
        sourceVolumeId: transferDialogProps.sourceVolumeId,
        destVolumeId: volumeId,
        conflictResolution,
        preKnownConflicts,
        fileCount: transferDialogProps.fileCount,
        folderCount: transferDialogProps.folderCount,
        mcpRequestId: transferDialogProps.mcpRequestId,
        initiator: transferDialogProps.initiator,
        duplicateFollowUp: transferDialogProps.duplicateFollowUp,
        newName,
      })

      showTransferDialog = false
      transferDialogProps = null
    },

    handleTransferCancel() {
      showTransferDialog = false
      transferDialogProps = null
      deps.onRefocus()
    },

    handleDeleteConfirm(previewId: string | null, isPermanent: boolean) {
      if (!deleteDialogProps) return

      const opType: TransferOperationType = isPermanent ? 'delete' : 'trash'

      // Collect per-item sizes for trash progress if available.
      // Group A wire-format: IPC sends `null` for absent sizes, so reject both null and undefined.
      const sizes = deleteDialogProps.sourceItems
        .map((item) => (item.isDirectory ? item.recursiveSize : item.size))
        .filter((s): s is number => s != null)
      const itemSizes = sizes.length === deleteDialogProps.sourceItems.length ? sizes : undefined

      startBirthOperation({
        operationType: opType,
        sourcePaths: deleteDialogProps.sourcePaths,
        sourceFolderPath: deleteDialogProps.sourceFolderPath,
        sourcePaneSide: deps.getFocusedPaneSide(),
        sortColumn: deleteDialogProps.sortColumn,
        sortOrder: deleteDialogProps.sortOrder,
        previewId,
        sourceVolumeId: deleteDialogProps.sourceVolumeId,
        itemSizes,
        mcpRequestId: deleteDialogProps.mcpRequestId,
        initiator: deleteDialogProps.initiator,
        // A delete creates nothing, so there is never a copy to name.
        duplicateFollowUp: 'nothing',
      })

      showDeleteDialog = false
      deleteDialogProps = null
    },

    handleDeleteCancel() {
      showDeleteDialog = false
      deleteDialogProps = null
      deps.onRefocus()
    },

    handleTransferComplete({
      filesProcessed,
      filesSkipped,
      bytesProcessed,
      appearedDuringMove,
      topLevelSkipped,
      refused,
    }: TransferCompletePayload) {
      const props = transferProgressProps
      const op = props?.operationType ?? 'copy'
      const opLabel = transferOpLabel(op)
      // Read the foreground slot NOW, while the progress dialog still holds it:
      // it releases the slot as it unmounts, below. The operation's journal is
      // where the duplicate's generated name comes from, so an id read too late
      // costs the rename editor.
      const settledOperationId = getForegroundOperationId()

      // ❌ No search-snapshot purge here. A dialog knows what the operation was
      // ASKED to do, which is the wrong input: it misses a skip, misses a cancel,
      // and isn't available at all to a window watching an operation it never
      // started. `$lib/search/snapshot-purge.ts` reads the per-path outcome
      // stream instead, for every window and every ending.
      log.info(
        `${opLabel} complete: ${String(filesProcessed)} files (${String(filesSkipped)} skipped, ${String(refused?.itemCount ?? 0)} refused, ${formatByteSize(bytesProcessed)})`,
      )
      // Top-level counts for the per-type split ("Moved 1 file and 3 folders").
      // F5/F6, drag-and-drop, and clipboard paste all supply these now; absent
      // only when a kind probe came back partial → composer falls back.
      const toastMessage = composeTransferCompleteToast({
        operationType: op,
        filesProcessed,
        filesSkipped,
        fileCount: props?.fileCount,
        folderCount: props?.folderCount,
        appearedDuringMove,
        topLevelSkipped,
      })
      // A trash the OS refused part of still ends as a completion, so without
      // this it said "Moved 1 file to trash" in success green and the item still
      // in the pane went unmentioned. It takes the level down with it: nothing
      // about a partly refused batch may read as a clean success.
      //
      // `info` for the all-skipped case (nothing actually moved/copied — neutral
      // outcome, not a success). `success` everywhere else, including mixed: the
      // user's intent landed at the target.
      const refusedMessage = composeTrashRefusedToast(refused)
      const allSkipped = filesSkipped > 0 && filesSkipped === filesProcessed
      const completeLevel: ToastLevel = refusedMessage !== null ? 'warn' : allSkipped ? 'info' : 'success'
      raiseCompletionToasts(op, settledOperationId, toastMessage, completeLevel, refusedMessage)

      paneEffects.refreshPanesAfterTransfer()
      paneEffects.clearSourcePaneAfterTransfer()

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()

      // A duplicate the trigger asked to name ends in the inline rename editor.
      // Last, and unawaited: it reads the journal, so it settles a round trip
      // after everything above, and it must not hold the dialog open meanwhile.
      void openRenameOnDuplicate({
        context: props,
        operationId: settledOperationId,
        paneRef: deps.getFocusedPaneRef(),
        showHiddenFiles: deps.getShowHiddenFiles(),
      })
    },

    /** The four outcomes of a dialog that ADOPTED its operation. Separate from
     *  the birth handlers, in a separate module, so their pane work is not
     *  reachable from a view that has no birth context to act on. */
    handleAdoptedComplete(payload: TransferCompletePayload) {
      adopted.handleComplete(payload)
    },

    handleAdoptedCancelled(filesProcessed: number) {
      adopted.handleCancelled(filesProcessed)
    },

    handleAdoptedError(error: WriteOperationError, progressAtStop: ProgressAtStop | null) {
      adopted.handleError(error, progressAtStop)
    },

    handleAdoptedQueue() {
      adopted.handleQueue()
    },

    /** The operation was sent to the background (Queue button / F2 / auto-queue):
     *  keep it running, just close its modal. The op is now managed in the queue
     *  window. We do NOT cancel it and do NOT refresh panes here — the op is still
     *  in flight; the file watcher and the queue window cover its lifecycle. We DO
     *  drop the source-pane operation snapshot and selection, since this dialog
     *  has handed the op off and won't fire `handleTransferComplete` for it. */
    handleTransferQueue() {
      const op = transferProgressProps?.operationType ?? 'copy'
      log.info('{op} sent to the background (managed in the queue window)', { op: transferOpLabel(op) })

      paneEffects.clearSourcePaneAfterTransfer()

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    handleTransferCancelled(filesProcessed: number) {
      const op = transferProgressProps?.operationType ?? 'copy'
      log.info(`${transferOpLabel(op)} cancelled after ${String(filesProcessed)} files`)

      paneEffects.refreshPanesAfterTransfer()
      paneEffects.adjustSelectionAfterCancel(op)

      showTransferProgressDialog = false
      transferProgressProps = null
      deps.onRefocus()
    },

    handleTransferError(error: WriteOperationError, progressAtStop: ProgressAtStop | null) {
      const op = transferProgressProps?.operationType ?? 'copy'
      // Read the foreground slot NOW, while the progress dialog still holds it:
      // it releases the slot as it unmounts, a few lines down, and the backend's
      // retained failure row only reaches the snapshot after that. Without this
      // handover the corner chip and the failure toast would both announce a
      // failure the user is looking at in this very dialog.
      const failedOperationId = getForegroundOperationId()

      // An encrypted-archive source needs a password: intercept BEFORE the generic
      // error dialog and prompt instead. Birth context stays alive (only the
      // progress dialog unmounts) so an unlock can re-dispatch the same op and a
      // cancel can settle through the normal refresh/selection paths.
      if (error.type === 'archive_needs_password' && transferProgressProps) {
        archivePassword.promptForTransfer({
          operationType: op,
          parentVolumeId: transferProgressProps.sourceVolumeId,
          archivePath: error.path,
          wrongAttempt: error.wrongAttempt,
          operationId: failedOperationId,
        })
        return
      }

      // The same technical lines the error dialog shows, ending with the variant.
      // The bridge forwards only the rendered message, so a payload passed as a
      // bare property would never reach the log or a bundle.
      log.error('{op} failed: {detail}', {
        op: transferOpLabel(op),
        detail: getTechnicalDetails(error).replaceAll('\n', '; '),
      })

      paneEffects.refreshPanesAfterTransfer()
      paneEffects.clearSourcePaneAfterTransfer()

      const retry = transferProgressProps ? retryPropsFrom(transferProgressProps) : null
      showTransferProgressDialog = false
      transferProgressProps = null

      openTransferError(op, error, failedOperationId, progressAtStop, retry)
    },

    handleArchivePasswordSubmit(password: string) {
      archivePassword.handleSubmit(password)
    },

    handleArchivePasswordCancel() {
      archivePassword.handleCancel()
    },

    handleTransferErrorClose() {
      // The user has read this one, so the copy in the operation queue has done
      // its job: drop it.
      settleTransferError()
      deps.onRefocus()
    },

    /** The error dialog's Retry: the same operation again, through the same start
     *  every new operation takes. A new operation, so the failed one is settled
     *  like a close. */
    handleTransferErrorRetry() {
      startAgainFromErrorDialog(transferErrorProps?.retry ?? null, 'retried')
    },

    /** The error dialog's "Copy anyway" after a space shortfall: the same copy
     *  again, told to skip the free-space check. The shortfall figure is an upper
     *  bound (files already there can make the copy need less), so it's the
     *  person's call; a destination that really fills up still stops the copy. */
    handleTransferErrorCopyAnyway() {
      const retry = transferErrorProps?.retry ?? null
      startAgainFromErrorDialog(retry && { ...retry, spaceShortfall: 'proceed' }, 'started anyway')
    },

    handleNewFolderCreated(folderName: string) {
      const paneRef = deps.getFocusedPaneRef()
      const paneListingId = paneRef?.getListingId()
      const hasParent = paneRef?.hasParentEntry()

      showNewFolderDialog = false
      newFolderDialogProps = null
      deps.onRefocus()

      if (!paneListingId) return
      void moveCursorToNewFolder(
        paneListingId,
        folderName,
        paneRef,
        hasParent ?? false,
        deps.getShowHiddenFiles(),
        onDirectoryDiff,
        (args) => findFileIndex(args.listingId, args.filename, args.showHiddenFiles),
      )
    },

    handleNewFolderCancel() {
      showNewFolderDialog = false
      newFolderDialogProps = null
      deps.onRefocus()
    },

    handleNewFileCreated(fileName: string) {
      const paneRef = deps.getFocusedPaneRef()
      const paneListingId = paneRef?.getListingId()
      const hasParent = paneRef?.hasParentEntry()
      const currentPath = newFileDialogProps?.currentPath ?? ''

      showNewFileDialog = false
      newFileDialogProps = null
      deps.onRefocus()

      if (paneListingId) {
        void moveCursorToNewFolder(
          paneListingId,
          fileName,
          paneRef,
          hasParent ?? false,
          deps.getShowHiddenFiles(),
          onDirectoryDiff,
          (args) => findFileIndex(args.listingId, args.filename, args.showHiddenFiles),
        )
      }

      // Open the newly created file in the default editor — but NOT for a file
      // created inside a zip: that's an async managed archive-edit, so the entry
      // doesn't exist yet when this runs (the create returns an op id, not a
      // landed path), and an archive-inner path isn't openable in an external
      // editor anyway. The cursor still lands on it after the edit's refresh.
      if (pathCrossesArchiveBoundary(currentPath)) return
      const fullPath = currentPath === '/' ? `/${fileName}` : `${currentPath}/${fileName}`
      deps.onOpenInEditor(fullPath)
    },

    handleNewFileCancel() {
      showNewFileDialog = false
      newFileDialogProps = null
      deps.onRefocus()
    },

    handleAlertClose() {
      showAlertDialog = false
      alertDialogProps = null
      deps.onRefocus()
    },

    // --- Query methods ---

    /** Closes any confirmation dialog (new folder, new file, transfer, or delete) if open (for MCP). */
    closeConfirmationDialog() {
      if (showNewFolderDialog) {
        showNewFolderDialog = false
        newFolderDialogProps = null
        deps.onRefocus()
      }
      if (showNewFileDialog) {
        showNewFileDialog = false
        newFileDialogProps = null
        deps.onRefocus()
      }
      if (showTransferDialog) {
        showTransferDialog = false
        transferDialogProps = null
        deps.onRefocus()
      }
      if (showDeleteDialog) {
        showDeleteDialog = false
        deleteDialogProps = null
        deps.onRefocus()
      }
    },

    /**
     * Dismisses every dialog after one of them threw while rendering.
     *
     * A dialog that throws mid-render leaves nothing on screen, but the `show*`
     * flag that opened it is already true, so `isConfirmationDialogOpen()` keeps
     * suppressing the pane's keyboard: the user is stuck with no dialog to
     * escape from. Clearing every flag (not only the confirmation ones
     * `closeConfirmationDialog` covers) and refocusing the pane is the one exit.
     * Called from `DialogManager`'s error boundary.
     */
    handleDialogRenderFailure(error: unknown) {
      log.error('A dialog threw while rendering; dismissing every dialog and refocusing the pane: {error}', {
        error: error instanceof Error ? `${error.name}: ${error.message}` : String(error),
      })
      addToast(tString('fileExplorer.pane.dialogRenderFailedToast'), { level: 'error' })
      this.dismissAllAfterRenderFailure()
    },

    dismissAllAfterRenderFailure() {
      showTransferDialog = false
      transferDialogProps = null
      showTransferProgressDialog = false
      transferProgressProps = null
      adopted.forget()
      showNewFolderDialog = false
      newFolderDialogProps = null
      showNewFileDialog = false
      newFileDialogProps = null
      showAlertDialog = false
      alertDialogProps = null
      showTransferErrorDialog = false
      transferErrorProps = null
      archivePassword.forget()
      showDeleteDialog = false
      deleteDialogProps = null
      deps.onRefocus()
    },

    isConfirmationDialogOpen(): boolean {
      return showNewFolderDialog || showNewFileDialog || showTransferDialog || showDeleteDialog
    },

    /** Whether any transfer/delete-related dialog is open (used by canSwapPanes). */
    isAnyTransferDialogOpen(): boolean {
      return showTransferDialog || showTransferProgressDialog || showDeleteDialog
    },

    // The MCP `dialog confirm` (`programmatic-confirm.ts`).
    ...programmaticConfirm,
  }
  return state
}
