/**
 * The Suggested ops dialog's backend surface.
 *
 * Reads only, plus the one decision the dialog can record by itself. Approving is a separate
 * path: it claims the group and hands it to the queue, so it lives with the executor bridge
 * rather than here.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type ApprovalResultView,
  type OpSelection,
  type RejectResultView,
  type RunsAs,
  type ServerApprovalResultView,
  type SuggestedOpLiveFact,
  type SuggestedOpPage,
  type SuggestedOpsLiveFactsError,
  type SuggestedSweepView,
  type SuggestionsChanged,
} from '#lib/ipc/bindings.js'
import { throwSuggestedOpsError } from '#lib/suggested-ops/suggested-ops-failure.js'

/** Every sweep with at least one group still waiting on the user, newest first. Counts only:
 *  not one op row is read, because a group of 60,000 ops is legitimate. */
export async function listSuggestedOps(): Promise<SuggestedSweepView[]> {
  const res = await commands.suggestedOpsList()
  if (res.status === 'error') throwSuggestedOpsError(res.error)
  return res.data
}

/** One window of a group's ops, in the order they were proposed, plus the group's total so the
 *  list can size itself without loading the rest. */
export async function pageSuggestedOps(groupId: number, offset: number, limit: number): Promise<SuggestedOpPage> {
  const res = await commands.suggestedOpsPage(groupId, offset, limit)
  if (res.status === 'error') throwSuggestedOpsError(res.error)
  return res.data
}

/** What one window of a group's ops is like on disk NOW, beside the creation snapshot the page
 *  carries. Per-file trouble is that file's `unknown`; a refusal means the source couldn't be asked
 *  at all (not connected, gone, no such group), which the dialog words once per group. */
export type SuggestedOpLiveFactsAnswer =
  | { kind: 'facts'; facts: SuggestedOpLiveFact[] }
  | { kind: 'refused'; reason: SuggestedOpsLiveFactsError }

export async function liveSuggestedOpFacts(
  groupId: number,
  offset: number,
  limit: number,
): Promise<SuggestedOpLiveFactsAnswer> {
  const res = await commands.suggestedOpsLiveFacts(groupId, offset, limit)
  return res.status === 'error' ? { kind: 'refused', reason: res.error } : { kind: 'facts', facts: res.data }
}

/** The user said no to a group. Answers what actually happened: a group somebody already
 *  decided keeps its answer, and the dialog re-reads rather than insisting. */
export async function rejectSuggestedGroup(groupId: number): Promise<RejectResultView> {
  const res = await commands.suggestedOpsReject(groupId)
  if (res.status === 'error') throwSuggestedOpsError(res.error)
  return res.data
}

/** Approve a group: claim it and hand its ops to the queue. Send whichever ids are fewer: the ones
 *  the user turned off (`allExcept`) or the ones they kept (`only`), so neither approving a
 *  60,000-op group whole nor ticking three of them carries 60,000 ids. An `only` that keeps
 *  nothing answers `nothingSelected` and runs nothing. `revision` is the group's `revision` the
 *  review showed: if the agent amended the group since, the approval answers `listChanged` and
 *  nothing runs. `runsAs` is the fallback the review showed (the group's `runsAs`): a trash group on
 *  a drive with no Trash deletes for good only when the user approved that delete. */
export async function approveSuggestedGroup(
  groupId: number,
  revision: number,
  selection: OpSelection,
  runsAs: RunsAs | null,
): Promise<ApprovalResultView> {
  const res = await commands.suggestedOpsApprove(groupId, revision, selection, runsAs)
  if (res.status === 'error') throwSuggestedOpsError(res.error)
  return res.data
}

/** Approve a server card (`addServer` / `editServer`, which have no ops): save it through the
 *  servers hub's own code, with no password and no contact with the server. `revision` is the one
 *  the review showed, as for a file group. Every refusal but `alreadyAnswered` and `unknown` gives
 *  the card back; a save that never answered throws `approvalDidntFinish`, and the card stays
 *  approved because it may have saved. */
export async function approveSuggestedServer(groupId: number, revision: number): Promise<ServerApprovalResultView> {
  const res = await commands.suggestedOpsApproveServer(groupId, revision)
  if (res.status === 'error') throwSuggestedOpsError(res.error)
  return res.data
}

/** Fires whenever the pending suggestion set changes: a sweep landed, a group was amended, or
 *  somebody approved or rejected one. Carries the counts an indicator renders from, so nothing
 *  has to query after being told. */
export function onSuggestionsChanged(callback: (payload: SuggestionsChanged) => void): Promise<UnlistenFn> {
  return events.suggestionsChanged.listen((event) => {
    callback(event.payload)
  })
}

export type {
  ApprovalResultView,
  DestinationState,
  LiveFileState,
  OpSelection,
  SuggestionChange,
  SuggestionsChanged,
  NewNameState,
  RejectResultView,
  RunsAs,
  ServerApprovalResultView,
  SuggestedGroupView,
  SuggestedOpLiveFact,
  SuggestedOpPage,
  SuggestedOpsLiveFactsError,
  SuggestedOpView,
  SuggestedSweepView,
} from '#lib/ipc/bindings.js'
