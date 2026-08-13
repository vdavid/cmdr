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
 *
 * ## A prompt is a view, so it commands through a session
 *
 * It holds the asking operation's session for exactly as long as its question is
 * on screen, and answers, cancels, and rolls back through it. Which surface may
 * SHOW a clash is still the ownership rule above, a UX preference; which answer
 * the operation acts on is the backend's call, and it reports its verdict. So
 * every verdict takes this prompt down, and only a call that never landed leaves
 * it up. ❌ Don't rebuild an ownership rule that makes correctness depend on one
 * surface answering.
 *
 * The one thing that stays off sessions is the fleet pause in `hold()`; the
 * reason is written there.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { SvelteMap } from 'svelte/reactivity'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  onWriteConflict,
  pauseOperation,
  resumeOperation,
  type ConflictId,
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
import { getOperationSessions } from './operation-session/window-operation-sessions.svelte'
import type { OperationSession } from './operation-session/operation-session.svelte'

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

/** The session for each operation this host is asking about, held for exactly
 *  as long as its prompt is up. A prompt IS a view of an operation, so it
 *  commands through the session like any other: the answer, the cancel, and the
 *  rollback all carry guards that whatever else is watching can see.
 *
 *  A `SvelteMap` because the disable-state getters read THROUGH it: a session
 *  acquired late (the registry wasn't up when the prompt was raised) has to make
 *  those getters re-run, and a plain map's `get` registers no dependency. It
 *  holds session references, which `SvelteMap` stores as they are rather than
 *  proxying — right, since a session is a getter-bearing object. */
const promptSessions = new SvelteMap<string, OperationSession>()

let unlisten: UnlistenFn | null = null
let stopEffects: (() => void) | null = null

/** The session for a prompted operation, built on first need and kept until the
 *  prompt goes. Lazy rather than eager only so a prompt raised before the
 *  window's registry exists can still find one later. */
function sessionFor(operationId: string): OperationSession | null {
  const held = promptSessions.get(operationId)
  if (held) return held
  const registry = getOperationSessions()
  if (!registry) {
    log.warn('No session registry yet for the conflict on {operationId}', { operationId })
    return null
  }
  const session = registry.acquire(operationId)
  promptSessions.set(operationId, session)
  return session
}

/** Lets go of a prompt's session. ❌ Never skip this on a path that drops a
 *  prompt: an unreleased session keeps listening for an operation that ended. */
function releaseSession(operationId: string): void {
  if (!promptSessions.delete(operationId)) return
  getOperationSessions()?.release(operationId)
}

/** The prompt on screen, or `null`. Reactive. */
export function getConflictPrompt(): ConflictPrompt | null {
  if (promptQueue.length === 0) return null
  const entry = promptQueue[0]
  const operationId = entry.event.operationId
  const snapshot = getMainWindowOperationRows().find((r) => r.snapshot.operationId === operationId)?.snapshot ?? null
  return { operationId, event: entry.event, snapshot, pausedOthers: pausedIds.length > 1 }
}

/** A resolution is in flight; the resolution buttons disable. Reactive, and it
 *  reads the OPERATION's own flag: a second surface answering the same clash
 *  disables these buttons too. */
export function isResolvingConflictPrompt(): boolean {
  if (promptQueue.length === 0) return false
  return promptSessions.get(promptQueue[0].event.operationId)?.resolvingConflict ?? false
}

/** A cancel is in flight; the Cancel / Rollback row disables. Reactive. */
export function isCancellingConflictPrompt(): boolean {
  if (promptQueue.length === 0) return false
  const session = promptSessions.get(promptQueue[0].event.operationId)
  return (session?.cancelling ?? false) || (session?.rollingBack ?? false)
}

function handleConflict(event: WriteConflictEvent): void {
  // Arrival only. Who owns the clash isn't decided until `drainDeferred` asks
  // `conflictOwner`, and a line here claiming this host has it would be wrong
  // for every conflict the progress dialog is already showing.
  log.debug('A write conflict arrived for {operationId} at {destinationPath}', {
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
    // The operation raises its next clash the moment it takes an answer, so this
    // is the ordinary shape of a run of them: the answer for the one on screen is
    // still in flight and the next question has already arrived. The newer clash
    // is the live one, and it takes the slot; the answer in flight names the
    // older one, so it can't touch it (`dropPrompt`).
    log.debug('A second conflict for {operationId} while one was still unanswered; showing the newer one', {
      operationId: event.operationId,
    })
    promptQueue = promptQueue.map((entry, i) => (i === existing ? { ...entry, event } : entry))
    return
  }

  log.info('No dialog is showing {operationId}, so the main window asks about {destinationPath}', {
    operationId: event.operationId,
    destinationPath: event.destinationPath,
  })

  const first = promptQueue.length === 0
  promptQueue = [
    ...promptQueue,
    { event, confirmedLive: rows.some((r) => r.snapshot.operationId === event.operationId) },
  ]
  // Claimed now rather than at the first click: the fan-out has been holding
  // this operation's events for whoever asks, and the session picks them up.
  sessionFor(event.operationId)
  void hold(event.operationId, rows)
  // Only for the first of a run: the person is already looking at the prompt for
  // the rest of it, and re-raising under their hands would be rude.
  if (first) void raiseMainWindow()
}

/** Pauses what this conflict stops, skipping anything already on hold.
 *
 *  ❌ Not through sessions, and that's the line: this is a FLEET action over
 *  every executing operation, the same class as the queue window's Pause all,
 *  and most of what it stops has no view here at all. A session's guards are
 *  about one operation's buttons; these ids are chosen by a rule
 *  (`operationsToPauseFor`) that is free to narrow to one operation later. */
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

/**
 * Takes an operation's prompt off the queue and lets go of its session.
 *
 * `answeredConflictId` scopes that to the clash that was actually answered. The
 * operation raises its next clash the moment it takes an answer, and
 * {@link takePrompt} swaps that newer event into this same entry, so an answer
 * returning a beat later can find a different question in it. Dropping the entry
 * then would throw the new question away and park the transfer with nothing on
 * screen.
 *
 * `null` is for the paths where the OPERATION is going away (cancel, rollback,
 * or it ended): every question it might be asking is moot.
 */
function dropPrompt(operationId: string, answeredConflictId: ConflictId | null): void {
  const entry = promptQueue.find((e) => e.event.operationId === operationId)
  if (!entry) return
  if (answeredConflictId !== null && entry.event.conflictId !== answeredConflictId) {
    log.info('Keeping the prompt for {operationId}: it has since raised another clash at {destinationPath}', {
      operationId,
      destinationPath: entry.event.destinationPath,
    })
    return
  }
  promptQueue = promptQueue.filter((e) => e.event.operationId !== operationId)
  releaseSession(operationId)
}

/**
 * Answers the prompt on screen: the operation's own resolve command, the same
 * one every other surface issues.
 *
 * Resolve first, resume second. If the call itself doesn't land, the prompt
 * stays up and everything stays paused, which is the honest state for a question
 * nobody has answered. The resolved operation may park at its next between-files
 * boundary in the moment before the resume reaches it; that's what pause is
 * built for, and a cancel still wins over both.
 *
 * Losing the race is NOT that case. Any verdict at all means the backend has
 * arbitrated this clash: the prompt comes down and what it paused resumes,
 * exactly as if this answer had won. Leaving it up would ask the user a question
 * that no longer has an answer to give.
 */
export async function resolveConflictPrompt(resolution: ConflictResolution, applyToAll: boolean): Promise<void> {
  if (promptQueue.length === 0) return
  const answered = promptQueue[0].event
  const operationId = answered.operationId
  const session = sessionFor(operationId)
  if (!session) return

  const outcome = await session.resolveConflict(answered.conflictId, resolution, applyToAll)
  if (outcome === null) return

  dropPrompt(operationId, answered.conflictId)
  await release()
}

/** Backs out of the operation the prompt is asking about. `rollback` reverses
 *  what it already wrote. The backend drops the conflict's parked answer slot,
 *  which is what unblocks the operation. */
export async function cancelConflictPrompt(rollback: boolean): Promise<void> {
  if (promptQueue.length === 0) return
  const operationId = promptQueue[0].event.operationId
  const session = sessionFor(operationId)
  if (!session) return

  const sent = rollback ? await session.rollback() : await session.cancel()
  if (!sent) return

  dropPrompt(operationId, null)
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
      releaseSession(entry.event.operationId)
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

/** Drops the listener, the effects, every prompt, and every session those
 *  prompts were holding. */
export function stopOperationConflictHost(): void {
  unlisten?.()
  unlisten = null
  stopEffects?.()
  stopEffects = null
  for (const operationId of [...promptSessions.keys()]) releaseSession(operationId)
  promptQueue = []
  pausedIds = []
  deferred = []
}
