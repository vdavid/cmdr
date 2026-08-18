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
  type RejectResultView,
  type SuggestedOpPage,
  type SuggestedSweepView,
  type SuggestionsChanged,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

/** Every sweep with at least one group still waiting on the user, newest first. Counts only:
 *  not one op row is read, because a group of 60,000 ops is legitimate. */
export async function listSuggestedOps(): Promise<SuggestedSweepView[]> {
  const res = await commands.suggestedOpsList()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** One window of a group's ops, in the order they were proposed, plus the group's total so the
 *  list can size itself without loading the rest. */
export async function pageSuggestedOps(groupId: number, offset: number, limit: number): Promise<SuggestedOpPage> {
  const res = await commands.suggestedOpsPage(groupId, offset, limit)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** The user said no to a group. Answers what actually happened: a group somebody already
 *  decided keeps its answer, and the dialog re-reads rather than insisting. */
export async function rejectSuggestedGroup(groupId: number): Promise<RejectResultView> {
  const res = await commands.suggestedOpsReject(groupId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Approve a group: claim it and hand its ops to the queue. Sends the ids the user turned OFF,
 *  never the ones they kept, so approving a 60,000-op group whole carries an empty list. */
export async function approveSuggestedGroup(groupId: number, deselectedOpIds: number[]): Promise<ApprovalResultView> {
  const res = await commands.suggestedOpsApprove(groupId, deselectedOpIds)
  if (res.status === 'error') throwIpcError(res.error)
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
  SuggestionChange,
  SuggestionsChanged,
  RejectResultView,
  SuggestedGroupView,
  SuggestedOpPage,
  SuggestedOpView,
  SuggestedSweepView,
} from '$lib/ipc/bindings'
