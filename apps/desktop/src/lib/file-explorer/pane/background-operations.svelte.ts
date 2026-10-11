/**
 * Operations this window runs in the BACKGROUND: started there straight from a
 * setup dialog (F2 or its Background button, #381), or sent there from the
 * progress dialog (its Queue button, its F2, auto-queue, or a close).
 *
 * A background start mounts NO modal, not even for a frame: it starts through
 * the same `startTransferOperation` the progress dialog uses, minus the
 * foreground claim, so the progress slot stays free and a second F5 opens at
 * once. Without a claim, a clash in the job is the main window's conflict host's
 * to ask (`$lib/file-operations/operation-conflict.svelte.ts`), exactly as for
 * any backgrounded job: F2 never means "overwrite silently".
 *
 * What it still owes the person, and nothing more (`transfer/DETAILS.md` §
 * "Starting in the background"):
 *
 * - **A refused start** (the backend said no before anything ran) goes to the
 *   owner, which opens the error dialog with a Retry. Nothing else would say it.
 * - **An archive password.** A copy out of an encrypted zip stops with
 *   `archive_needs_password`, which the backend does NOT retain as a failure: it
 *   is a question, and the progress dialog is normally who asks it. With no
 *   dialog, this module keeps the operation's session until its outcome lands
 *   and hands that one outcome to the owner, which raises the prompt.
 * - **A completion** goes to the owner too, which raises a trash's Undo toast
 *   (the reason trash is offered here at all) and stays quiet for the rest.
 * - **Everything else is ambient**, as for any backgrounded job: the queue
 *   window and the corner chip show it, a retained failure raises the failure
 *   toast, and the file watcher updates the panes. No rename editor for a
 *   duplicate, no selection restore on cancel.
 */

import { untrack } from 'svelte'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
import { startTransferOperation } from '$lib/file-operations/transfer/transfer-dispatch'
import { getOperationSessions } from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import type { OperationOutcome } from '$lib/file-operations/operation-session/operation-session.svelte'
import { createTransferPaneEffects, type TransferPaneEffectsDeps } from './transfer-pane-effects'
import { transferOpLabel } from './transfer-op-label'
import type { WriteOperationError } from '../types'
import type { TransferCompletePayload, TransferProgressPropsData } from './dialog-props'

const log = getAppLogger('fileExplorer')

/** The one `write-error` a background job hands back to the window. */
export type ArchiveNeedsPasswordError = Extract<WriteOperationError, { type: 'archive_needs_password' }>

export interface BackgroundOperationsDeps extends TransferPaneEffectsDeps {
  /** The backend refused the start before anything ran. */
  onStartRefused: (props: TransferProgressPropsData, error: WriteOperationError) => void
  /** A background job stopped to ask for its archive's password. */
  onNeedsPassword: (props: TransferProgressPropsData, error: ArchiveNeedsPasswordError, operationId: string) => void
  /** A background job finished. The owner decides what, if anything, it says. */
  onCompleted: (props: TransferProgressPropsData, payload: TransferCompletePayload, operationId: string) => void
}

export function createBackgroundOperations(deps: BackgroundOperationsDeps) {
  /** Operations this module is watching, until a beat after their outcome: long
   *  enough for any other view of the same operation (Show) reacting to that
   *  outcome to ask `watches` and leave the password stop to this module. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping read on demand by `watches`, never rendered or tracked
  const watched = new Set<string>()

  /**
   * Holds `operationId`'s session until its outcome lands, then lets go. Only an
   * archive-password stop is passed on; every other ending already has its
   * ambient surface.
   */
  function watch(props: TransferProgressPropsData, operationId: string): void {
    const registry = getOperationSessions()
    if (registry === null) {
      // The window's sessions start at init, before any dialog can confirm, so
      // this is a teardown race. The job runs on; only a password stop goes
      // unasked, and the queue window still lists it.
      log.warn('No operation sessions to watch the background op={operationId} with', { operationId })
      return
    }
    const session = registry.acquire(operationId)
    watched.add(operationId)
    let done = false

    const finish = (outcome: OperationOutcome): void => {
      done = true
      // Let the effect run finish, and every other view's reaction to the same
      // outcome, before tearing the root down and forgetting the id.
      queueMicrotask(() => {
        stopWatching()
        registry.release(operationId)
        watched.delete(operationId)
      })
      if (outcome.kind === 'error' && outcome.event.error.type === 'archive_needs_password') {
        deps.onNeedsPassword(props, outcome.event.error, operationId)
      } else if (outcome.kind === 'complete') {
        const event = outcome.event
        deps.onCompleted(
          props,
          {
            filesProcessed: event.filesProcessed,
            filesSkipped: event.filesSkipped,
            bytesProcessed: event.bytesProcessed,
            appearedDuringMove: event.appearedDuringMove ?? null,
            topLevelSkipped: event.topLevelSkipped ?? null,
            refused: event.refused ?? null,
          },
          operationId,
        )
      }
    }

    const stopWatching = $effect.root(() => {
      $effect(() => {
        const outcome = session.outcome
        if (outcome === null || done) return
        untrack(() => {
          finish(outcome)
        })
      })
    })
  }

  /**
   * Starts `props` in the background: no modal, no foreground claim. On success
   * the queue window shows WITHOUT taking focus, a quiet toast says where the
   * job went, and the source pane's selection is dropped, as a Queue press does.
   */
  async function start(props: TransferProgressPropsData): Promise<void> {
    // A permanent delete is the one operation nothing undoes, so it never starts
    // where nobody watches it. No caller sends one (the delete dialog's background
    // path builds a trash, Queue doesn't flag a delete); this holds the line.
    if (props.operationType === 'delete') {
      log.error('Refused to start a permanent delete in the background; it needs its progress dialog')
      return
    }
    log.info('{op} starting in the background', { op: transferOpLabel(props.operationType) })
    const result = await startTransferOperation(props)
    if (!result.started) {
      deps.onStartRefused(props, result.error)
      return
    }
    createTransferPaneEffects(deps, () => props).clearSourcePaneAfterTransfer()
    addToast(tString('fileOperations.backgroundStart.startedToast'), { level: 'info', toastGroup: 'transfer-queue' })
    void openQueueWindow({ focus: false })
    watch(props, result.operationId)
  }

  return {
    start,
    watch,
    /** Whether `operationId` is a background job this module asks for, so a view
     *  adopted onto it (Show) reports its password stop through the prompt
     *  rather than as a failure. */
    watches: (operationId: string): boolean => watched.has(operationId),
  }
}
