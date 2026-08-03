/**
 * The bulk-rename review: the state slice behind `BulkRenameReviewDialog.svelte`.
 *
 * A `proposalReady` streaming event opens it, every user decision and every relevant pane
 * change revalidates it against the backend, Apply starts one managed operation for the rows
 * the user allowed, and a finished batch leaves an undo line in the thread.
 *
 * The SERVER owns the outcome throughout: a name the user types goes over IPC and comes back
 * validated (never patched locally), preflight is what makes an edited name applicable at
 * all, and Apply sends opaque row ids, not paths.
 */

import { getAppLogger } from '$lib/logging/logger'
import { SvelteSet } from 'svelte/reactivity'
import type { RailMessage } from './ask-cmdr-messages'
import { askCmdrState, type BulkRenameReviewRow } from './ask-cmdr-state.svelte'
import { undoStateFromReport } from './rename-undo'
import {
  applyBulkRename,
  cancelBulkRenameProposal,
  preflightBulkRename,
  reviseBulkRenameRow,
  undoOperations,
  type AskCmdrStreamEvent,
} from '$lib/tauri-commands'

const log = getAppLogger('askCmdr')

export function openRenameReview(proposal: Extract<AskCmdrStreamEvent, { type: 'proposalReady' }>['proposal']): void {
  discardRenameReview()
  askCmdrState.renameReview = {
    proposalId: proposal.proposalId,
    rows: proposal.rows.map((row) => ({
      ...row,
      allowed: true,
      blockedReason: null,
      warnings: [],
      nameRejected: false,
    })),
    preflighting: false,
    expired: false,
    requestVersion: 0,
  }
  void refreshRenamePreflight()
}

/** Change one row's user decision, then revalidate the exact allowed subset. */
export function setRenameRowAllowed(rowId: string, allowed: boolean): void {
  const review = askCmdrState.renameReview
  const row = review?.rows.find((candidate) => candidate.rowId === rowId)
  if (!review || !row || (row.blockedReason && allowed)) return
  row.allowed = allowed
  void refreshRenamePreflight()
}

/**
 * Replace one row's proposed name with the one the user typed, then revalidate.
 *
 * The backend owns everything about the new name: it validates it, swaps the row's evidence for
 * the "you typed this" marker (the model's quote described the model's name), and invalidates the
 * accepted preflight — so the fresh preflight below is what lets the edited name be applied at
 * all. A name it won't take leaves the row on the name it had, said plainly on the row.
 */
export async function reviseRenameRow(rowId: string, destinationName: string): Promise<void> {
  const review = askCmdrState.renameReview
  const row = review?.rows.find((candidate) => candidate.rowId === rowId)
  if (!review || !row || review.expired) return
  // No IPC for a field the user left as it was (a blur after no edit, or Enter twice).
  if (destinationName === row.destinationName) {
    row.nameRejected = false
    return
  }
  try {
    const revised = await reviseBulkRenameRow(review.proposalId, rowId, destinationName)
    const current = liveRenameRow(review.proposalId, rowId)
    if (!current) return
    current.destinationName = revised.destinationName
    current.evidence = revised.evidence
    current.coverage = revised.coverage
    current.nameRejected = false
    await refreshRenamePreflight()
  } catch (e) {
    log.warn('revising a proposed name failed: {error}', { error: String(e) })
    const current = liveRenameRow(review.proposalId, rowId)
    if (current) current.nameRejected = true
  }
}

/** The row as it stands NOW, or `null` if the review closed or was replaced meanwhile. */
function liveRenameRow(proposalId: string, rowId: string): BulkRenameReviewRow | null {
  const current = askCmdrState.renameReview
  if (!current || current.proposalId !== proposalId) return null
  return current.rows.find((candidate) => candidate.rowId === rowId) ?? null
}

/** Allow every row the latest preflight did not block. */
export function allowAllRenameRows(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  for (const row of review.rows) {
    if (!row.blockedReason) row.allowed = true
  }
  void refreshRenamePreflight()
}

/** Deny every row. This sends no filesystem request and creates no operation. */
export function denyAllRenameRows(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  for (const row of review.rows) row.allowed = false
  void refreshRenamePreflight()
}

/** Revalidates a review when the pane's existing file watcher reports a name
 * that participates in the proposal. The backend remains authoritative; this
 * name filter only avoids unrelated watcher traffic causing extra IPC. */
export async function renameReviewListingChanged(
  changes: ReadonlyArray<{ type?: string; entry: { name: string } }>,
): Promise<void> {
  const review = askCmdrState.renameReview
  if (!review) return
  const reviewedNames = new SvelteSet(review.rows.flatMap((row) => [row.sourceName, row.destinationName]))
  if (changes.some((change) => reviewedNames.has(change.entry.name))) await refreshRenamePreflight()
}

/** The destination names in this review the user did NOT take: denied rows, and (when the whole
 * review is cancelled) every row. Names only — what the model needs is the fact that a style was
 * rejected, and a reason would be its own words handed back to it. */
function rememberDeniedNames(rows: { allowed: boolean; destinationName: string }[]): void {
  const denied = rows.filter((row) => !row.allowed).map((row) => row.destinationName)
  if (denied.length === 0) return
  // Newest first, so the cap in the envelope keeps the most recent decision.
  askCmdrState.deniedNames = [...denied, ...askCmdrState.deniedNames]
}

/** Cancel closes the review and consumes its server-owned proposal. */
export function cancelRenameReview(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  // Cancelling turns down every row, and that is exactly the feedback the next batch needs.
  rememberDeniedNames(review.rows.map((row) => ({ allowed: false, destinationName: row.destinationName })))
  askCmdrState.renameReview = null
  void cancelBulkRenameProposal(review.proposalId)
}

/** Starts the one managed operation for the rows the user currently allows. */
export async function applyRenameReview(): Promise<void> {
  const review = askCmdrState.renameReview
  if (!review || review.preflighting || review.expired) return
  const allowedRowIds = review.rows.filter((row) => row.allowed && !row.blockedReason).map((row) => row.rowId)
  if (allowedRowIds.length === 0) return
  review.preflighting = true
  try {
    const started = await applyBulkRename(review.proposalId, allowedRowIds)
    // Applying a subset is a decision about the rest: carry those names into the next batch.
    rememberDeniedNames(review.rows)
    noteRenameApplied(started.operationId, allowedRowIds.length)
    if (askCmdrState.renameReview?.proposalId === review.proposalId) askCmdrState.renameReview = null
  } catch (e) {
    const current = askCmdrState.renameReview
    if (!current || current.proposalId !== review.proposalId) return
    current.preflighting = false
    log.warn('starting the rename plan failed: {error}', { error: String(e) })
    void refreshRenamePreflight()
  }
}

/**
 * Record a finished batch in the thread, with its undo.
 *
 * The line goes in the thread rather than in the (now closed) review dialog,
 * because that's where the user is looking and because a run of batches then reads
 * as a run. **Only the newest line carries the job-wide undo**, and only once a run
 * has more than one batch: the previous lines hand their ids over and keep just
 * their own Undo, so "undo everything" appears once, at the bottom.
 */
export function noteRenameApplied(operationId: string, fileCount: number): void {
  const run = renameRunLines()
  // Built from the lines themselves, never from a previous line's stored job set:
  // that set already includes its predecessors, so folding it in would repeat ids.
  const jobOperationIds = [...run.map((line) => line.operationId), operationId]
  const jobFileCount = run.reduce((total, line) => total + line.fileCount, 0) + fileCount
  // The older lines are no longer the newest, so they give up the job-wide action.
  for (const line of run) {
    line.jobOperationIds = []
    line.jobFileCount = 0
  }
  askCmdrState.messages.push({
    kind: 'renameApplied',
    operationId,
    fileCount,
    jobOperationIds: jobOperationIds.length > 1 ? jobOperationIds : [],
    jobFileCount: jobOperationIds.length > 1 ? jobFileCount : 0,
    undo: { status: 'undoable' },
  })
}

/** Every rename line in this thread that can still be undone, oldest first. */
function renameRunLines(): Extract<RailMessage, { kind: 'renameApplied' }>[] {
  return askCmdrState.messages.filter(
    (message): message is Extract<RailMessage, { kind: 'renameApplied' }> =>
      message.kind === 'renameApplied' && message.undo.status === 'undoable',
  )
}

/**
 * Put the old names back for one batch, or (`scope: 'job'`) for every batch of the
 * run this line closes.
 *
 * The ids go over in APPLY order; the backend reverses them newest first, which is
 * the only order that works when a later batch took a name an earlier one freed. It
 * resolves when the reversal has actually finished, so the line can report what
 * came back rather than claiming success on dispatch.
 */
export async function undoRename(
  line: Extract<RailMessage, { kind: 'renameApplied' }>,
  scope: 'batch' | 'job' = 'batch',
): Promise<void> {
  if (line.undo.status !== 'undoable') return
  const operationIds = scope === 'job' && line.jobOperationIds.length > 0 ? line.jobOperationIds : [line.operationId]
  const covered = renameRunLines().filter((candidate) => operationIds.includes(candidate.operationId))
  // Remember each covered line's state so a call that never reached the backend can
  // hand its Undo back. A plain array of pairs, since this map is local and never
  // read reactively (`SvelteMap` would be reactivity nobody observes).
  const previous = covered.map((candidate) => [candidate, candidate.undo] as const)
  for (const candidate of covered) candidate.undo = { status: 'undoing' }
  try {
    const report = await undoOperations(operationIds)
    const state = undoStateFromReport(report)
    // The line the user clicked reports the whole tally; the others it covered are
    // done with, so they stop offering an Undo that would now be refused.
    line.undo = state
    for (const candidate of covered) {
      if (candidate !== line) candidate.undo = { status: 'unavailable' }
      candidate.jobOperationIds = []
      candidate.jobFileCount = 0
    }
  } catch (e) {
    log.warn('undoing the rename failed: {error}', { error: String(e) })
    // Nothing is known to have moved, so hand every line its Undo back.
    for (const [candidate, state] of previous) candidate.undo = state
  }
}

export function discardRenameReview(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  askCmdrState.renameReview = null
  void cancelBulkRenameProposal(review.proposalId)
}

async function refreshRenamePreflight(): Promise<void> {
  const review = askCmdrState.renameReview
  if (!review) return
  const version = review.requestVersion + 1
  review.requestVersion = version
  review.preflighting = true
  // Validate every displayed row, including denied and previously blocked rows.
  // Otherwise a target that disappears after blocking its row could never make
  // that row reviewable again. Apply still submits only the user's allowed ids.
  const allowedRowIds = review.rows.map((row) => row.rowId)
  try {
    const result = await preflightBulkRename(review.proposalId, allowedRowIds)
    const current = askCmdrState.renameReview
    if (!current || current.proposalId !== review.proposalId || current.requestVersion !== version) return
    current.preflighting = false
    current.expired = result.status === 'expired'
    if (current.expired) return
    for (const row of current.rows) {
      const backend = result.rows.find((candidate) => candidate.rowId === row.rowId)
      row.blockedReason = backend?.status === 'blocked' ? backend.reason : null
      if (backend) row.warnings = backend.warnings
      if (row.blockedReason) row.allowed = false
    }
  } catch (e) {
    const current = askCmdrState.renameReview
    if (!current || current.proposalId !== review.proposalId || current.requestVersion !== version) return
    current.preflighting = false
    log.warn('checking the rename plan failed: {error}', { error: String(e) })
  }
}
