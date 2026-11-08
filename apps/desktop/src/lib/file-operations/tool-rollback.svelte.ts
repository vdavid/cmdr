/**
 * A rollback question a TOOL asked for (`operations_rollback`), for a person to answer.
 *
 * ❌ Never stacks. A running operation with a progress dialog up asks through that dialog's own
 * question, which it may already be showing. Otherwise any modal in the way refuses, typed, and
 * the question stands alone in `ToolRollbackDialog.svelte`. `DETAILS.md` § "Rollback asks first".
 */

import type { Initiator, OpKind } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { asRollbackRefusal } from '#lib/operation-log/rollback-refusal.js'
import { rollbackRefusalNotice } from '#lib/operation-log/operation-log-labels.js'
import { rollbackOperation } from '#lib/tauri-commands/index.js'
import { blockingSoftDialogBesides } from '#lib/ui/open-dialogs.svelte.js'
import { startedByWhenGuarded, type ToolInitiator } from '#lib/ui/started-elsewhere-guard.js'
import type { SoftDialogId } from '#lib/ui/dialog-registry.js'
import { addToast } from '#lib/ui/toast/index.js'
import { opKindForWireType } from './op-kind'
import { getMainWindowOperationRows } from './queue/main-window-operations.svelte'
import type { OperationRow } from './queue/operations-store.svelte'
import {
  inFlightRollbackVariant,
  liveRollbackBlock,
  rollbackConfirmVariant,
  type LiveRollbackBlock,
  type RollbackConfirmVariant,
} from './reversal-wording'
import type { RollbackAnswer } from './rollback-confirmers'

const log = getAppLogger('fileOperations')

/** What the tool knows to ask about, off `mcp-rollback-dialog`. */
export type ToolRollbackFacts = { operationId: string; initiator: Initiator; guard: boolean } & (
  | { target: 'live' }
  | { target: 'logged'; kind: OpKind; rest: boolean }
)

/** The question up in `ToolRollbackDialog.svelte`. */
export type ToolRollbackQuestion = ToolRollbackFacts & { variant: RollbackConfirmVariant; rest: boolean }

/** What came of asking for the question, as `mcp-response` carries it. */
export type ToolRollbackRaise =
  | { ok: true }
  | { ok: false; blockedBy: SoftDialogId; error: string }
  | { ok: false; refusal: LiveRollbackBlock; error: string }

let question = $state<ToolRollbackQuestion | null>(null)

/** Who a tool's question names and arms for, `undefined` for a client a person let past them. */
type ProgressRollbackAsker = (startedBy: ToolInitiator | undefined) => LiveRollbackBlock | null

/** A mounted progress dialog's way to raise its own question, by the operation it shows. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- read once per request, and nothing renders from it
const progressAskers = new Map<string, ProgressRollbackAsker>()

/**
 * Lets the progress dialog showing `operationId` take this operation's question. The asker
 * raises it and answers `null`, or says why it can't now. Returns the unregister.
 */
export function registerProgressRollbackAsker(operationId: string, ask: ProgressRollbackAsker): () => void {
  progressAskers.set(operationId, ask)
  return () => {
    if (progressAskers.get(operationId) === ask) progressAskers.delete(operationId)
  }
}

export function getToolRollbackQuestion(): ToolRollbackQuestion | null {
  return question
}

export function dismissToolRollback(): void {
  question = null
}

/** Why this running operation can't roll back right now, or `null` while it can. */
export function liveRowBlock(row: OperationRow | undefined): LiveRollbackBlock | null {
  if (row === undefined) return 'notRunning'
  return liveRollbackBlock({
    status: row.snapshot.status,
    rollbackSupport: row.snapshot.rollbackSupport,
    phase: row.progress?.phase ?? null,
    kind: opKindForWireType(row.snapshot.operationType),
  })
}

export function liveRow(operationId: string): OperationRow | undefined {
  return getMainWindowOperationRows().find((row) => row.snapshot.operationId === operationId)
}

function refused(refusal: LiveRollbackBlock): ToolRollbackRaise {
  return { ok: false, refusal, error: `This operation can't roll back right now, so no dialog opened (${refusal}).` }
}

function blocked(blockedBy: SoftDialogId): ToolRollbackRaise {
  return {
    ok: false,
    blockedBy,
    error: `The ${blockedBy} dialog is open, so the rollback dialog can't open too. Close it first, then try again.`,
  }
}

/** Raises the question, or says typed what's in the way. */
export function raiseToolRollback(facts: ToolRollbackFacts): ToolRollbackRaise {
  if (facts.target === 'live') {
    const askProgress = progressAskers.get(facts.operationId)
    if (askProgress !== undefined) {
      // Its own dialog and the question over it are no obstacle: this IS that question.
      const inTheWay = blockingSoftDialogBesides(['transfer-progress', 'rollback-confirmation'])
      if (inTheWay !== null) return blocked(inTheWay)
      const refusal = askProgress(startedByWhenGuarded(facts.initiator, facts.guard))
      return refusal === null ? { ok: true } : refused(refusal)
    }
  }

  const inTheWay = blockingSoftDialogBesides([])
  if (inTheWay !== null) return blocked(inTheWay)

  if (facts.target === 'logged') {
    question = { ...facts, variant: rollbackConfirmVariant(facts.kind) }
    return { ok: true }
  }
  const row = liveRow(facts.operationId)
  const refusal = liveRowBlock(row)
  if (refusal !== null || row === undefined) return refused(refusal ?? 'notRunning')
  question = {
    ...facts,
    variant: inFlightRollbackVariant(opKindForWireType(row.snapshot.operationType)),
    rest: false,
  }
  return { ok: true }
}

/** The yes on a finished operation: the path the history dialog's takes, and a refusal gets said. */
export async function confirmLoggedToolRollback(operationId: string, initiator: Initiator): Promise<RollbackAnswer> {
  try {
    const { inverseOpId } = await rollbackOperation(operationId, initiator)
    return { status: 'dispatched', inverseOpId }
  } catch (e) {
    const refusal = asRollbackRefusal(e)
    addToast(tString(rollbackRefusalNotice(refusal)), { level: 'info' })
    if (refusal) log.info("Couldn't roll {operationId} back: {reason}", { operationId, reason: refusal.kind })
    else log.warn("Couldn't roll {operationId} back: {error}", { operationId, error: e })
    return { status: 'refused', refusal: refusal ?? { kind: 'unknownOperation' } }
  }
}
