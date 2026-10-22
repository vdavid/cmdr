/**
 * The done state's Undo: the operation log's finished-operation rollback
 * (`#lib/operation-log/CLAUDE.md`), offered from a progress dialog that stayed
 * open on what it did. ❌ Not a second reversal path: it reads the same journal
 * row the history dialog reads, offers exactly what that row admits
 * (`rowRollbackAction`), asks through the same `RollbackConfirmDialog` variant,
 * and dispatches through the same `rollbackOperation`.
 *
 * The journal row is readable only once `write-settled` arrives (the buffered
 * tail flushes in the finalize barrier, `../CLAUDE.md`), so until then Undo
 * waits, focusable and explained, rather than reading an empty page.
 */

import type { OpKind } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { getOperationLogDetail, rollbackOperation } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { asRollbackRefusal } from '#lib/operation-log/rollback-refusal.js'
import { rollbackRefusalNotice, rowRollbackAction, rowStandingNotice } from '#lib/operation-log/operation-log-labels.js'

const log = getAppLogger('transferProgress')

/** Where Undo stands for one finished operation. */
type DoneUndoState =
  /** Nothing to offer: this kind of operation has no Undo here, or the journal
   *  never recorded it (the operation log was off). */
  | { kind: 'none' }
  /** The journal isn't readable yet. */
  | { kind: 'waiting' }
  /** The journal admits a rollback. `opKind` words the question; `finishing`
   *  picks up a reversal someone stopped partway. */
  | { kind: 'ready'; opKind: OpKind; finishing: boolean }
  /** The journal says no, and why when it says so. */
  | { kind: 'blocked'; reason: MessageKey | null }

interface DoneUndoInputs {
  operationId: () => string | null
  /** The dialog is on its done state, for an operation kind it offers Undo on. */
  offered: () => boolean
  /** `write-settled` has arrived, so the journal row is readable. */
  settled: () => boolean
  /** The reversal is on its way, as its own operation: the dialog closes. */
  onDispatched: () => void
}

export function createDoneUndo(inputs: DoneUndoInputs) {
  /** What the journal said, once it was asked. */
  let read = $state<DoneUndoState | null>(null)
  /** Latches the one journal read. A plain `let`: the effect must not re-run on it. */
  let readStarted = false
  let confirming = $state(false)
  let dispatching = $state(false)
  let refusal = $state<MessageKey | null>(null)

  async function readJournal(operationId: string): Promise<void> {
    try {
      const detail = await getOperationLogDetail(operationId, 0, 0)
      if (detail === null) {
        read = { kind: 'none' }
        return
      }
      const row = detail.operation
      const action = rowRollbackAction(row.rollbackState)
      read =
        action === null
          ? { kind: 'blocked', reason: rowStandingNotice(row.rollbackState, row.notRollbackableReason) }
          : { kind: 'ready', opKind: row.kind, finishing: action === 'finish' }
    } catch (e) {
      log.warn("Couldn't read op={operationId} from the operation log for Undo: {error}", { operationId, error: e })
      read = { kind: 'none' }
    }
  }

  $effect(() => {
    if (!inputs.offered() || !inputs.settled() || readStarted) return
    const operationId = inputs.operationId()
    if (operationId === null) return
    readStarted = true
    void readJournal(operationId)
  })

  function state(): DoneUndoState {
    if (!inputs.offered()) return { kind: 'none' }
    return read ?? { kind: 'waiting' }
  }

  /** Asks first, like every rollback (`../CLAUDE.md` § "Rollback asks first"). */
  function request(): void {
    if (state().kind !== 'ready' || dispatching) return
    refusal = null
    confirming = true
  }

  function dismiss(): void {
    confirming = false
  }

  /** The answer was yes: hand the reversal to the queue, or say why not. */
  async function confirm(): Promise<void> {
    confirming = false
    const operationId = inputs.operationId()
    if (operationId === null || dispatching) return
    dispatching = true
    try {
      await rollbackOperation(operationId)
      log.info('Undo of op={operationId} handed to the queue', { operationId })
      inputs.onDispatched()
    } catch (e) {
      const typed = asRollbackRefusal(e)
      refusal = rollbackRefusalNotice(typed)
      if (typed) log.info("Couldn't undo op={operationId}: {reason}", { operationId, reason: typed.kind })
      else log.warn("Couldn't undo op={operationId}: {error}", { operationId, error: e })
    } finally {
      dispatching = false
    }
  }

  return {
    request,
    dismiss,
    confirm,
    get state(): DoneUndoState {
      return state()
    },
    /** The question is up. */
    get confirming(): boolean {
      return confirming && state().kind === 'ready'
    },
    get dispatching(): boolean {
      return dispatching
    },
    /** Why the last confirmed Undo was turned down, or `null`. */
    get refusal(): MessageKey | null {
      return refusal
    },
  }
}
