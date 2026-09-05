/**
 * The bulk-rename review: the state slice behind `BulkRenameReviewDialog.svelte`.
 *
 * A turn's `proposalReady` events stage plans here, the end of the turn opens ONE review over
 * all of them, every user decision and every relevant pane change revalidates it against the
 * backend, Apply starts one managed operation per staged plan, and each leaves an undo line in
 * the thread.
 *
 * The SERVER owns the outcome throughout: a name the user types goes over IPC and comes back
 * validated (never patched locally), preflight is what makes an edited name applicable at
 * all, and Apply sends opaque row ids, not paths.
 *
 * **Grouping is presentational, and every guardrail stays per row**: the evidence check, the
 * pane-scoped source validation, the fingerprinted preflight, and the fingerprint recheck at
 * write time all still run per row, inside the proposal that row belongs to.
 */

import { getAppLogger } from '$lib/logging/logger'
import { SvelteSet } from 'svelte/reactivity'
import type { RailMessage } from './ask-cmdr-messages'
import { askCmdrState, type BulkRenameReviewProposal, type BulkRenameReviewRow } from './ask-cmdr-state.svelte'
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

type ProposalSnapshot = Extract<AskCmdrStreamEvent, { type: 'proposalReady' }>['proposal']

/**
 * The plans this turn has staged but not shown yet.
 *
 * The review opens on the turn boundary, so the user meets a 500-file job once rather than
 * once per batch. Plain module state: nothing renders from it, and it lives exactly as long as
 * the turn does.
 */
let stagedProposals: ProposalSnapshot[] = []

/** Stage one batch. It joins the review the current turn is building, and shows on turn end. */
export function stageRenameProposal(proposal: ProposalSnapshot): void {
  stagedProposals.push(proposal)
}

/**
 * Show everything this turn staged, as one review.
 *
 * Called on every way a turn can end, including a failure and the user's own Stop: the plans
 * exist on the spine either way, and a staged plan the user never sees is a plan they can't
 * answer.
 */
export function openStagedRenameReview(): void {
  const staged = stagedProposals
  stagedProposals = []
  if (staged.length === 0) return
  const proposals = staged.map((proposal): BulkRenameReviewProposal => ({
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
  }))
  const review = askCmdrState.renameReview
  // A review already on screen GROWS rather than being replaced: replacing it cancelled a plan
  // the user was reading, and asked them the same question again.
  if (review) review.proposals.push(...proposals)
  else askCmdrState.renameReview = { proposals }
  for (const proposal of proposals) void refreshRenamePreflight(proposal.proposalId)
}

/** Drop what this turn staged without showing it: the thread it belonged to is gone. */
export function discardStagedRenameProposals(): void {
  const staged = stagedProposals
  stagedProposals = []
  for (const proposal of staged) void cancelBulkRenameProposal(proposal.proposalId)
}

/** The proposal with this id, as it stands NOW, or `null` if the review moved on without it. */
function liveProposal(proposalId: string): BulkRenameReviewProposal | null {
  return askCmdrState.renameReview?.proposals.find((candidate) => candidate.proposalId === proposalId) ?? null
}

/** The row as it stands NOW, or `null` if the review closed or was replaced meanwhile. */
function liveRenameRow(proposalId: string, rowId: string): BulkRenameReviewRow | null {
  return liveProposal(proposalId)?.rows.find((candidate) => candidate.rowId === rowId) ?? null
}

/** Every row on show, whichever proposal staged it. */
function allRenameRows(): BulkRenameReviewRow[] {
  return askCmdrState.renameReview?.proposals.flatMap((proposal) => proposal.rows) ?? []
}

/** Change one row's user decision, then revalidate its proposal's exact allowed subset. */
export function setRenameRowAllowed(proposalId: string, rowId: string, allowed: boolean): void {
  const row = liveRenameRow(proposalId, rowId)
  if (!row || (row.blockedReason && allowed)) return
  row.allowed = allowed
  void refreshRenamePreflight(proposalId)
}

/**
 * Replace one row's proposed name with the one the user typed, then revalidate.
 *
 * The backend owns everything about the new name: it validates it, swaps the row's evidence for
 * the "you typed this" marker (the model's quote described the model's name), and invalidates the
 * accepted preflight — so the fresh preflight below is what lets the edited name be applied at
 * all. A name it won't take leaves the row on the name it had, said plainly on the row.
 */
export async function reviseRenameRow(proposalId: string, rowId: string, destinationName: string): Promise<void> {
  const proposal = liveProposal(proposalId)
  const row = proposal?.rows.find((candidate) => candidate.rowId === rowId)
  if (!proposal || !row || proposal.expired) return
  // No IPC for a field the user left as it was (a blur after no edit, or Enter twice).
  if (destinationName === row.destinationName) {
    row.nameRejected = false
    return
  }
  try {
    const revised = await reviseBulkRenameRow(proposalId, rowId, destinationName)
    const current = liveRenameRow(proposalId, rowId)
    if (!current) return
    current.destinationName = revised.destinationName
    current.evidence = revised.evidence
    current.coverage = revised.coverage
    current.nameRejected = false
    await refreshRenamePreflight(proposalId)
  } catch (e) {
    log.warn('revising a proposed name failed: {error}', { error: String(e) })
    const current = liveRenameRow(proposalId, rowId)
    if (current) current.nameRejected = true
  }
}

/** Allow every row the latest preflight did not block, across every batch. */
export function allowAllRenameRows(): void {
  for (const proposal of askCmdrState.renameReview?.proposals ?? []) {
    for (const row of proposal.rows) {
      if (!row.blockedReason) row.allowed = true
    }
    void refreshRenamePreflight(proposal.proposalId)
  }
}

/** Deny every row. This sends no filesystem request and creates no operation. */
export function denyAllRenameRows(): void {
  for (const proposal of askCmdrState.renameReview?.proposals ?? []) {
    for (const row of proposal.rows) row.allowed = false
    void refreshRenamePreflight(proposal.proposalId)
  }
}

/** Revalidates the batches a review holds when the pane's existing file watcher reports a
 * name that participates in one of them. The backend remains authoritative; this name filter
 * only avoids unrelated watcher traffic causing extra IPC, and keeps a job spanning several
 * folders from re-preflighting every batch over one folder's change. */
export async function renameReviewListingChanged(
  changes: ReadonlyArray<{ type?: string; entry: { name: string } }>,
): Promise<void> {
  const changed = new SvelteSet(changes.map((change) => change.entry.name))
  const affected = (askCmdrState.renameReview?.proposals ?? []).filter((proposal) =>
    proposal.rows.some((row) => changed.has(row.sourceName) || changed.has(row.destinationName)),
  )
  await Promise.all(affected.map((proposal) => refreshRenamePreflight(proposal.proposalId)))
}

/** The destination names in this review the user did NOT take: denied rows, and (when the whole
 * review is cancelled) every row. Names only — what the model needs is the fact that a style was
 * rejected, and a reason would be its own words handed back to it.
 *
 * A job is answered once now, so there is no later batch in the same job to teach. What this
 * still feeds is the user's NEXT message ("not like that, try dates instead"), which is why it
 * survives the grouping. */
function rememberDeniedNames(rows: { allowed: boolean; destinationName: string }[]): void {
  const denied = rows.filter((row) => !row.allowed).map((row) => row.destinationName)
  if (denied.length === 0) return
  // Newest first, so the cap in the envelope keeps the most recent decision.
  askCmdrState.deniedNames = [...denied, ...askCmdrState.deniedNames]
}

/** Cancel closes the review and consumes every proposal it holds. */
export function cancelRenameReview(): void {
  if (!askCmdrState.renameReview) return
  // Cancelling turns down every row, and that is exactly the feedback the next try needs.
  rememberDeniedNames(allRenameRows().map((row) => ({ allowed: false, destinationName: row.destinationName })))
  closeRenameReview()
}

/**
 * Start one managed operation per batch, for the rows the user currently allows.
 *
 * Sequential, in staging order, because that is the order undo has to reverse: a later batch
 * can have taken a name an earlier one freed. A batch that fails leaves itself and everything
 * after it in the review, revalidated, so the user can answer what is left.
 */
export async function applyRenameReview(): Promise<void> {
  const review = askCmdrState.renameReview
  if (!review || review.proposals.some((proposal) => proposal.preflighting)) return
  const pending = review.proposals
    .filter((proposal) => !proposal.expired)
    .map((proposal) => ({
      proposalId: proposal.proposalId,
      allowedRowIds: proposal.rows.filter((row) => row.allowed && !row.blockedReason).map((row) => row.rowId),
    }))
    .filter((batch) => batch.allowedRowIds.length > 0)
  if (pending.length === 0) return
  // Read the decisions before the first batch leaves the review, so a run that stops partway
  // still reports what the user turned down in the batches that did start.
  const decisions = allRenameRows().map((row) => ({ allowed: row.allowed, destinationName: row.destinationName }))
  for (const proposal of review.proposals) proposal.preflighting = true
  let applied = 0
  for (const batch of pending) {
    try {
      const started = await applyBulkRename(batch.proposalId, batch.allowedRowIds)
      applied += 1
      noteRenameApplied(started.operationId, batch.allowedRowIds.length)
      dropAppliedProposal(batch.proposalId)
    } catch (e) {
      log.warn('starting the rename plan failed: {error}', { error: String(e) })
      break
    }
  }
  // Applying a subset is a decision about the rest: carry those names into the next message.
  if (applied > 0) rememberDeniedNames(decisions)
  if (applied === pending.length) {
    // Every batch the user allowed something in has started. A batch still here is one they
    // turned down whole, which is an answer too, so it is consumed rather than left on screen.
    closeRenameReview()
    return
  }
  // A batch refused to start, so the rest of the job is still the user's to answer and goes
  // back to a checked state.
  for (const proposal of askCmdrState.renameReview?.proposals ?? []) {
    proposal.preflighting = false
    void refreshRenamePreflight(proposal.proposalId)
  }
}

/** Close the review, consuming every proposal still in it. */
function closeRenameReview(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  askCmdrState.renameReview = null
  for (const proposal of review.proposals) void cancelBulkRenameProposal(proposal.proposalId)
}

/** Take a started batch out of the review; the rest of the job is still on screen. */
function dropAppliedProposal(proposalId: string): void {
  const review = askCmdrState.renameReview
  if (!review) return
  review.proposals = review.proposals.filter((proposal) => proposal.proposalId !== proposalId)
}

/**
 * Record a finished batch in the thread, with its undo.
 *
 * The line goes in the thread rather than in the (now closed) review dialog,
 * because that's where the user is looking and because a run of batches then reads
 * as a run. **Only the newest line carries the job-wide undo**, and only once a run
 * has more than one batch: the previous lines hand their ids over and keep just
 * their own Undo, so "undo everything" appears once, at the bottom. That holds
 * whether the ids arrive one turn at a time or together from one Apply.
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

/** Close the review and consume every proposal it holds, plus anything staged but unshown. */
export function discardRenameReview(): void {
  discardStagedRenameProposals()
  closeRenameReview()
}

async function refreshRenamePreflight(proposalId: string): Promise<void> {
  const proposal = liveProposal(proposalId)
  if (!proposal) return
  const version = proposal.requestVersion + 1
  proposal.requestVersion = version
  proposal.preflighting = true
  // Validate every displayed row, including denied and previously blocked rows.
  // Otherwise a target that disappears after blocking its row could never make
  // that row reviewable again. Apply still submits only the user's allowed ids.
  const allowedRowIds = proposal.rows.map((row) => row.rowId)
  try {
    const result = await preflightBulkRename(proposalId, allowedRowIds)
    const current = liveProposal(proposalId)
    if (!current || current.requestVersion !== version) return
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
    const current = liveProposal(proposalId)
    if (!current || current.requestVersion !== version) return
    current.preflighting = false
    log.warn('checking the rename plan failed: {error}', { error: String(e) })
  }
}
