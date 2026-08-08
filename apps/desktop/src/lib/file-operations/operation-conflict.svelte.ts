/**
 * The main window's host for conflict prompts nobody else can answer.
 *
 * A copy or move whose destination folder already exists MERGES, and the
 * dialog's upfront conflict check only lists the destination's top level — so a
 * clash on a file deep inside a merged folder can't be known before the
 * operation starts. Under "Ask for each" the backend emits `write-conflict` and
 * parks the operation on a oneshot until somebody answers. The progress dialog
 * answers its own; once the user presses Queue, that dialog is gone and its
 * listener with it, and the operation used to wait forever with no surface
 * saying so.
 *
 * ## The prompt belongs to the operation
 *
 * Not to a window. This host doesn't know which windows are open, never talks to
 * the queue window, and would work unchanged if the queue became a popover
 * inside the main window. The two questions it has to answer — who owns a
 * conflict, and how much stops while it waits — are pure functions in
 * `operation-conflict-rules.ts`, so widening ownership (a queue row handing an
 * operation back to the progress dialog) or narrowing the pause (letting
 * parallel lanes carry on) is a change there and nowhere else.
 *
 * ## What it does, in order
 *
 * 1. A conflict arrives. If a dialog is mid-dispatch, hold it: the foreground
 *    slot proves nothing until the claim settles.
 * 2. Otherwise, if the progress dialog owns that operation, drop it — the dialog
 *    is already showing it.
 * 3. Otherwise pause what's running, remember exactly which operations that was,
 *    raise the main window, and ask.
 * 4. The answer resolves the clash, then resumes exactly what was paused, and
 *    only once the last queued prompt is answered.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  cancelWriteOperation,
  onWriteConflict,
  pauseOperation,
  resolveWriteConflict,
  resumeOperation,
  type OperationSnapshot,
  type WriteConflictEvent,
} from '$lib/tauri-commands'
import type { ConflictResolution } from '$lib/file-explorer/types'
import { getAppLogger } from '$lib/logging/logger'
import { isE2eRun } from '$lib/app-mode'
import { getMainWindowOperationRows } from './queue/main-window-operations.svelte'
import { isTerminalStatus, type OperationRow } from './queue/operations-store.svelte'
import { getForegroundOperationId, isForegroundClaimPending } from './foreground-operation.svelte'
import { conflictOwner, operationsToPauseFor } from './operation-conflict-rules'

const log = getAppLogger('operation-conflict')

/** A conflict waiting for an answer. */
interface PromptEntry {
  event: WriteConflictEvent
  /** True once the operation has been seen live in a snapshot. Until then, its
   *  absence from the rows means "hasn't arrived yet", not "gone": the rows come
   *  on their own stream, and dropping the entry would re-wedge the operation. */
  confirmedLive: boolean
}

/** What the prompt renders. */
export interface ConflictPrompt {
  operationId: string
  event: WriteConflictEvent
  /** The operation's row, for naming which transfer is asking. `null` only in
   *  the sliver before its snapshot lands. */
  snapshot: OperationSnapshot | null
  /** Whether anything BESIDES the asking operation is on hold, so the prompt
   *  can't claim a hold it doesn't have. */
  pausedOthers: boolean
}

/** Unanswered prompts, oldest first. One is shown at a time: the buttons say
 *  "Skip" and "Overwrite all", and two sets of them on screen give no way to
 *  tell which operation each acts on. */
let promptQueue = $state<PromptEntry[]>([])

/** Exactly what this host paused, so the answer resumes that and nothing else.
 *  ❌ Never `resumeAll()`: it would restart an operation the USER paused. */
let pausedIds = $state<string[]>([])

/** Conflicts held because a dialog is mid-dispatch and might be about to own
 *  them. Plain, not `$state`: nothing renders from it, and the effect that
 *  drains it is driven by the claim settling, not by this array. */
let deferred: WriteConflictEvent[] = []

let resolving = $state(false)
let cancelling = $state(false)

let unlisten: UnlistenFn | null = null
let stopEffects: (() => void) | null = null

/** The prompt on screen, or `null`. Reactive. */
export function getConflictPrompt(): ConflictPrompt | null {
  const entry = promptQueue[0]
  if (!entry) return null
  const operationId = entry.event.operationId
  const snapshot = getMainWindowOperationRows().find((r) => r.snapshot.operationId === operationId)?.snapshot ?? null
  return { operationId, event: entry.event, snapshot, pausedOthers: pausedIds.length > 1 }
}

/** A resolution is in flight; the resolution buttons disable. Reactive. */
export function isResolvingConflictPrompt(): boolean {
  return resolving
}

/** A cancel is in flight; the Cancel / Rollback row disables. Reactive. */
export function isCancellingConflictPrompt(): boolean {
  return cancelling
}

function handleConflict(event: WriteConflictEvent): void {
  log.info('Conflict on an operation with no dialog in front of it: {operationId} at {destinationPath}', {
    operationId: event.operationId,
    destinationPath: event.destinationPath,
  })
  deferred.push(event)
  drainDeferred()
}

/** Decides every held conflict that can be decided now, and keeps the rest. */
function drainDeferred(): void {
  if (deferred.length === 0) return
  const foreground = {
    foregroundOperationId: getForegroundOperationId(),
    claimPending: isForegroundClaimPending(),
  }
  const held: WriteConflictEvent[] = []
  for (const event of deferred) {
    switch (conflictOwner(event.operationId, foreground)) {
      case 'unknown':
        held.push(event)
        break
      case 'foreground':
        // The progress dialog is showing this clash in its own body.
        break
      case 'here':
        takePrompt(event)
        break
    }
  }
  deferred = held
}

function takePrompt(event: WriteConflictEvent): void {
  const rows = getMainWindowOperationRows()
  const existing = promptQueue.findIndex((entry) => entry.event.operationId === event.operationId)
  if (existing >= 0) {
    // The backend serializes prompts per operation, so this shouldn't happen. If
    // it ever did, the newer clash is the live one: `resolveWriteConflict` is
    // keyed by operation id alone, so an answer lands on whatever that operation
    // is parked on right now.
    log.warn('A second conflict for {operationId} while one was still unanswered; taking the newer one', {
      operationId: event.operationId,
    })
    promptQueue = promptQueue.map((entry, i) => (i === existing ? { ...entry, event } : entry))
    return
  }

  const first = promptQueue.length === 0
  promptQueue = [
    ...promptQueue,
    { event, confirmedLive: rows.some((r) => r.snapshot.operationId === event.operationId) },
  ]
  void hold(event.operationId, rows)
  // Only for the first of a run: the person is already looking at the prompt for
  // the rest of it, and re-raising under their hands would be rude.
  if (first) void raiseMainWindow()
}

/** Pauses what this conflict stops, skipping anything already on hold. */
async function hold(conflictOperationId: string, rows: OperationRow[]): Promise<void> {
  const ids = operationsToPauseFor(conflictOperationId, rows).filter((id) => !pausedIds.includes(id))
  if (ids.length === 0) return
  // Recorded BEFORE the IPC: the prompt renders off this, and a failed pause
  // still has to be resumed (it may have landed on the backend regardless).
  pausedIds = [...pausedIds, ...ids]
  await Promise.all(
    ids.map(async (id) => {
      try {
        await pauseOperation(id)
      } catch (error) {
        log.warn('Failed to pause {operationId} for a conflict prompt: {error}', { operationId: id, error })
      }
    }),
  )
}

/** Resumes what was paused, once nothing is left to answer. */
async function release(): Promise<void> {
  if (promptQueue.length > 0) return
  const ids = pausedIds
  if (ids.length === 0) return
  pausedIds = []
  for (const id of ids) {
    try {
      await resumeOperation(id)
    } catch (error) {
      log.warn('Failed to resume {operationId} after a conflict prompt: {error}', { operationId: id, error })
    }
  }
}

function dropPrompt(operationId: string): void {
  promptQueue = promptQueue.filter((entry) => entry.event.operationId !== operationId)
}

/**
 * Answers the prompt on screen: the same command, with the same arguments, that
 * the progress dialog sends.
 *
 * Resolve first, resume second. If the resolve doesn't land, the prompt stays up
 * and everything stays paused, which is the honest state for a question nobody
 * has answered. The resolved operation may park at its next between-files
 * boundary in the moment before the resume reaches it; that's what pause is
 * built for, and a cancel still wins over both.
 */
export async function resolveConflictPrompt(resolution: ConflictResolution, applyToAll: boolean): Promise<void> {
  const entry = promptQueue[0]
  if (!entry || resolving) return
  const operationId = entry.event.operationId

  resolving = true
  let resolved = false
  try {
    await resolveWriteConflict(operationId, resolution, applyToAll)
    resolved = true
  } catch (error) {
    log.error('Failed to resolve the conflict on {operationId}: {error}', { operationId, error })
  } finally {
    resolving = false
  }
  if (!resolved) return

  dropPrompt(operationId)
  await release()
}

/** Backs out of the operation the prompt is asking about. `rollback` reverses
 *  what it already wrote. The backend drops the conflict's oneshot sender, which
 *  is what unblocks the parked operation. */
export async function cancelConflictPrompt(rollback: boolean): Promise<void> {
  const entry = promptQueue[0]
  if (!entry || cancelling) return
  const operationId = entry.event.operationId

  cancelling = true
  let sent = false
  try {
    await cancelWriteOperation(operationId, rollback)
    sent = true
  } catch (error) {
    log.error('Failed to cancel {operationId} from its conflict prompt: {error}', { operationId, error })
  } finally {
    cancelling = false
  }
  if (!sent) return

  dropPrompt(operationId)
  await release()
}

/**
 * Drops prompts whose operation is no longer live, and resumes if that empties
 * the queue.
 *
 * The operation can end without this host asking: Cancel in the queue window, a
 * failure, or the app tearing the operation down. Leaving the prompt up would
 * park the user on a question with no answer left to give, and leave the rest of
 * the queue paused behind it.
 *
 * Exported so the pass is callable straight from a test; the effect only decides
 * when it runs.
 */
export function reconcileConflictPrompts(rows: OperationRow[]): void {
  if (promptQueue.length === 0) return
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local for one pass; nothing renders from it
  const live = new Set(rows.filter((r) => !isTerminalStatus(r.snapshot.status)).map((r) => r.snapshot.operationId))

  let changed = false
  const next: PromptEntry[] = []
  for (const entry of promptQueue) {
    if (live.has(entry.event.operationId)) {
      if (entry.confirmedLive) {
        next.push(entry)
      } else {
        next.push({ ...entry, confirmedLive: true })
        changed = true
      }
      continue
    }
    if (entry.confirmedLive) {
      log.info('Dropping the conflict prompt for {operationId}: the operation is gone', {
        operationId: entry.event.operationId,
      })
      changed = true
      continue
    }
    next.push(entry)
  }
  if (!changed) return

  const dropped = next.length !== promptQueue.length
  promptQueue = next
  if (dropped) void release()
}

/** Brings the main window forward. The path into this bug ends with the queue
 *  window in front, and a prompt nobody can see is the same wedge with more
 *  code. Self-focus, because cross-window `setFocus()` doesn't reliably raise on
 *  macOS. */
async function raiseMainWindow(): Promise<void> {
  // E2E drives the webview over a socket and doesn't need OS focus; stealing it
  // per run makes the host machine unusable.
  if (isE2eRun()) return
  try {
    await getCurrentWindow().setFocus()
  } catch (error) {
    log.warn('Failed to bring the main window forward for a conflict prompt: {error}', { error })
  }
}

/**
 * Starts listening. Idempotent; call {@link stopOperationConflictHost} on
 * teardown.
 *
 * Two effects, both outside a component: this belongs to the window, not to
 * whatever happens to render the prompt, and an `$effect.root` keeps each pass
 * callable from a test.
 */
export async function startOperationConflictHost(): Promise<void> {
  if (unlisten ?? stopEffects) return

  stopEffects = $effect.root(() => {
    $effect(() => {
      // Read both unconditionally so a claim settling always re-runs the pass,
      // even when nothing is held right now.
      void isForegroundClaimPending()
      void getForegroundOperationId()
      drainDeferred()
    })
    $effect(() => {
      reconcileConflictPrompts(getMainWindowOperationRows())
    })
  })

  unlisten = await onWriteConflict(handleConflict)
}

/** Drops the listener, the effects, and every prompt. */
export function stopOperationConflictHost(): void {
  unlisten?.()
  unlisten = null
  stopEffects?.()
  stopEffects = null
  promptQueue = []
  pausedIds = []
  deferred = []
  resolving = false
  cancelling = false
}
