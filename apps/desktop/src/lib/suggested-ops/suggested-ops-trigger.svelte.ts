/**
 * Reactive state and the open/close seam for the Suggested ops dialog.
 *
 * The dialog's job is to lay a suggestion out so the user can decide, which shapes three
 * things here more than any convenience would:
 *
 * - **Ops load a WINDOW at a time.** A group of 60,000 is legitimate, so nothing loads a
 *   group to show it: the list knows its `total` from a `COUNT(*)` and fetches the rows the
 *   viewport actually reaches.
 * - **Deselection is a set of op ids, and there is deliberately no "deselect all".**
 *   Approving sends the ids the user turned OFF, so "all of them" is the empty set and the
 *   wire stays small at any group size. Turning down a whole group is what Reject is for; a
 *   deselect-all would have to enumerate 60,000 ids to say the same thing.
 * - **A group that changed under an open review is ANNOUNCED, never swapped.** The rows the
 *   user is reading stay put and a notice offers the reload, because re-ordering a list
 *   somebody is halfway through deciding on is how a wrong row gets approved.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { getAppLogger } from '$lib/logging/logger'
import type { MessageKey } from '$lib/intl/keys.gen'
import {
  approveSuggestedGroup,
  listSuggestedOps,
  onSuggestionsChanged,
  pageSuggestedOps,
  rejectSuggestedGroup,
  type ApprovalResultView,
  type SuggestedOpView,
  type SuggestedSweepView,
} from '$lib/tauri-commands'
import { asSuggestedOpsError } from './suggested-ops-failure'

const log = getAppLogger('suggestedOps')

/** How many op rows one fetch pulls. Comfortably more than a viewport, so ordinary scrolling
 *  reads from memory and only a jump costs a round trip. */
export const OP_WINDOW_SIZE = 200

/** The rows currently held for the open group. */
interface OpWindow {
  groupId: number
  /** Index of `ops[0]` within the whole group. */
  offset: number
  ops: SuggestedOpView[]
  /** Every op row the group has, from `COUNT(*)`. The list sizes itself from this. */
  total: number
}

/** Why the last Approve or Reject didn't happen, as the notice the dialog shows. */
export type DecisionNotice = Extract<MessageKey, 'suggestedOps.decisionNotRecorded' | 'suggestedOps.approvalUnsure'>

/** Why an approval didn't start. The group is pending again, and the dialog shows this under it. */
export type GroupRefusal = Extract<ApprovalResultView, { kind: 'refused' | 'nothingToRun' | 'couldNotStart' }>

interface SuggestedOpsState {
  open: boolean
  /** True while the sweep list is loading (the dialog shows a spinner). */
  loading: boolean
  /** True when the read threw: the dialog says so rather than showing an empty list, which
   *  would read as "the agent has suggested nothing". */
  loadError: boolean
  sweeps: SuggestedSweepView[]
  /** The group whose ops are expanded, if any. */
  openGroupId: number | null
  window: OpWindow | null
  windowLoading: boolean
  /** True when the open group's last page read threw. The list says so and offers a retry,
   *  rather than leaving its rows on a loading placeholder nothing will ever fill. */
  windowError: boolean
  /** Op ids the user turned off in the open group. Survives scrolling: it is keyed by op id,
   *  not by row position. */
  deselected: SvelteSet<number>
  /** Set when a re-read found the open group's op set changed under the user. */
  changedUnderReview: boolean
  /** The group whose decision is in flight, so its buttons can disable without freezing the
   *  rest of the dialog. */
  busyGroupId: number | null
  /** Why the last decision didn't happen, or `null`. Set AFTER the re-read, so it speaks about
   *  the list the user is now looking at, and cleared when the next decision starts. */
  decisionNotice: DecisionNotice | null
  /** Groups whose last approval refused to start, keyed by group id. Each one is pending again
   *  and keeps its reason under it until the user answers it again or closes the dialog. */
  refusals: SvelteMap<number, GroupRefusal>
}

export const suggestedOpsState = $state<SuggestedOpsState>({
  open: false,
  loading: false,
  loadError: false,
  sweeps: [],
  openGroupId: null,
  window: null,
  windowLoading: false,
  windowError: false,
  deselected: new SvelteSet<number>(),
  changedUnderReview: false,
  busyGroupId: null,
  decisionNotice: null,
  refusals: new SvelteMap<number, GroupRefusal>(),
})

/** Every group still waiting, flattened out of its sweep. */
export function pendingGroups(): SuggestedSweepView['groups'] {
  return suggestedOpsState.sweeps.flatMap((sweep) => sweep.groups)
}

/** The open group's header, or `null`. */
export function openGroup(): SuggestedSweepView['groups'][number] | null {
  const id = suggestedOpsState.openGroupId
  if (id === null) return null
  return pendingGroups().find((group) => group.groupId === id) ?? null
}

/** How many ops of the open group would actually run: everything live, minus what the user
 *  turned off. Derived from the COUNT, so it's right even for rows never fetched. */
export function approvableCount(): number {
  const group = openGroup()
  if (!group) return 0
  return Math.max(0, group.liveOpCount - suggestedOpsState.deselected.size)
}

/** Live only while the dialog is open: the badge owns the session-long subscription. */
let unlistenChanges: UnlistenFn | null = null

export function closeSuggestedOps(): void {
  suggestedOpsState.open = false
  suggestedOpsState.decisionNotice = null
  collapseGroup()
  suggestedOpsState.refusals.clear()
  unlistenChanges?.()
  unlistenChanges = null
}

/**
 * Opens the dialog and loads what's waiting. Idempotent, so a menu, palette, and shortcut
 * triple-fire opens it once. Always opens, even on a read failure: a dead menu item is worse
 * than a dialog that says it couldn't read.
 */
export async function openSuggestedOps(): Promise<void> {
  if (suggestedOpsState.open) return
  suggestedOpsState.open = true
  await subscribeWhileOpen()
  await refreshSuggestions()
}

/**
 * Listen for changes while the dialog is up.
 *
 * The interesting case is `amended` on the group the user has open. A count comparison alone
 * would miss an amendment that swapped a path but kept the op count, and `groupId` alone can't
 * tell an amendment from the user's own approval: both carry the same id, and only one of them
 * means "the thing you are reading moved".
 */
async function subscribeWhileOpen(): Promise<void> {
  if (unlistenChanges) return
  try {
    unlistenChanges = await listenForChanges()
  } catch (e) {
    // The dialog opens either way. Losing the subscription costs the live notice, not the
    // review: every refresh still compares the open group's op count, so a change that alters
    // it is still announced.
    log.warn("Couldn't subscribe to suggestion changes: {error}", { error: String(e) })
  }
}

function listenForChanges(): Promise<UnlistenFn> {
  return onSuggestionsChanged((payload) => {
    if (payload.reason === 'amended' && payload.groupId !== null && payload.groupId === suggestedOpsState.openGroupId) {
      // The rows on screen stay exactly where they are; the notice offers the reload.
      suggestedOpsState.changedUnderReview = true
    }
    void refreshSuggestions()
  })
}

/** Re-read the waiting sweeps. */
export async function refreshSuggestions(): Promise<void> {
  suggestedOpsState.loading = true
  suggestedOpsState.loadError = false
  try {
    const sweeps = await listSuggestedOps()
    const openId = suggestedOpsState.openGroupId
    const before = openId === null ? null : pendingGroups().find((group) => group.groupId === openId)
    suggestedOpsState.sweeps = sweeps
    if (openId !== null) {
      const after = pendingGroups().find((group) => group.groupId === openId)
      if (!after) {
        // The group the user was reading is gone (answered elsewhere, or withdrawn).
        collapseGroup()
      } else if (before && after.liveOpCount !== before.liveOpCount) {
        // Announce it. The rows on screen stay exactly where they are.
        suggestedOpsState.changedUnderReview = true
      }
    }
  } catch (e) {
    suggestedOpsState.loadError = true
    log.warn("Couldn't read the waiting suggestions: {error}", { error: String(e) })
  } finally {
    suggestedOpsState.loading = false
  }
}

/** Expand one group and load its first window. */
export async function expandGroup(groupId: number): Promise<void> {
  if (suggestedOpsState.openGroupId === groupId) return
  suggestedOpsState.openGroupId = groupId
  suggestedOpsState.window = null
  suggestedOpsState.windowError = false
  suggestedOpsState.deselected = new SvelteSet<number>()
  suggestedOpsState.changedUnderReview = false
  await ensureOpWindow(groupId, 0)
}

export function collapseGroup(): void {
  suggestedOpsState.openGroupId = null
  suggestedOpsState.window = null
  suggestedOpsState.windowError = false
  suggestedOpsState.deselected = new SvelteSet<number>()
  suggestedOpsState.changedUnderReview = false
}

/**
 * Make sure the rows around `startIndex` are loaded, fetching a window when they aren't.
 *
 * The virtual list calls this as it scrolls, and the retry after a failed read calls it too. A
 * request for rows already held is a no-op, so ordinary scrolling inside a window costs nothing.
 */
export async function ensureOpWindow(groupId: number, startIndex: number): Promise<void> {
  const held = suggestedOpsState.window
  if (held && held.groupId === groupId && startIndex >= held.offset && startIndex < held.offset + held.ops.length) {
    return
  }
  if (suggestedOpsState.windowLoading) return

  // Centre the window on what was asked for, so scrolling either way stays inside it.
  const offset = Math.max(0, startIndex - OP_WINDOW_SIZE / 4)
  suggestedOpsState.windowLoading = true
  suggestedOpsState.windowError = false
  try {
    const page = await pageSuggestedOps(groupId, offset, OP_WINDOW_SIZE)
    // The user may have collapsed or switched groups while this was in flight.
    if (suggestedOpsState.openGroupId !== groupId) return
    suggestedOpsState.window = {
      groupId,
      offset: page.offset,
      ops: page.ops,
      total: page.total,
    }
  } catch (e) {
    // The virtual list only asks again when the viewport moves, so without this the rows would
    // sit on their loading placeholder for good while Approve stays live.
    if (suggestedOpsState.openGroupId === groupId) suggestedOpsState.windowError = true
    log.warn("Couldn't read a page of proposed ops: {error}", { error: String(e) })
  } finally {
    suggestedOpsState.windowLoading = false
  }
}

/** The op at a whole-group index, or `null` when its window isn't loaded yet. */
export function opAt(index: number): SuggestedOpView | null {
  const held = suggestedOpsState.window
  if (!held) return null
  const local = index - held.offset
  return held.ops[local] ?? null
}

/** Turn one op off, or back on. */
export function toggleOp(opId: number): void {
  if (suggestedOpsState.deselected.has(opId)) {
    suggestedOpsState.deselected.delete(opId)
  } else {
    suggestedOpsState.deselected.add(opId)
  }
}

/**
 * Approve a group: claim it and hand its ops to the queue.
 *
 * Sends the ids the user turned OFF, never the ones they kept. On success the dialog gets out
 * of the way and the queue surface takes over, which is the hand-off the whole feature is
 * built around: an approved op is an ordinary queued op from here on.
 *
 * A refusal is not a failure to hide. Each variant means a different thing to the user, so the
 * dialog re-reads and lets them see the state that actually exists. A refusal to START gives
 * the group back: nothing ran, it's pending again, and its reason sits under it, so the user is
 * where they were and knows why.
 */
export async function approveGroup(groupId: number): Promise<void> {
  if (suggestedOpsState.busyGroupId !== null) return
  suggestedOpsState.busyGroupId = groupId
  suggestedOpsState.decisionNotice = null
  suggestedOpsState.refusals.delete(groupId)
  try {
    const result = await approveSuggestedGroup(groupId, [...suggestedOpsState.deselected])
    if (isRefusalToStart(result)) suggestedOpsState.refusals.set(groupId, result)
    if (result.kind === 'started') {
      if (suggestedOpsState.openGroupId === groupId) collapseGroup()
      await refreshSuggestions()
      // Nothing waiting means nothing left to decide, so the dialog closes itself rather than
      // sitting there empty over the queue that just took over.
      if (suggestedOpsState.sweeps.length === 0) suggestedOpsState.open = false
      return
    }
    log.info('A suggestion group was not approved: {kind}', { kind: result.kind })
    if (result.kind === 'listChanged') suggestedOpsState.changedUnderReview = true
    await refreshSuggestions()
  } catch (e) {
    log.warn("Couldn't approve the group: {error}", { error: String(e) })
    await answerFailedDecision(e)
  } finally {
    suggestedOpsState.busyGroupId = null
  }
}

/** The answers that give a group back with a reason, as opposed to closing it or re-reading. */
function isRefusalToStart(result: ApprovalResultView): result is GroupRefusal {
  switch (result.kind) {
    case 'refused':
    case 'nothingToRun':
    case 'couldNotStart':
      return true
    case 'started':
    case 'alreadyAnswered':
    case 'listChanged':
    case 'unknown':
      return false
  }
}

/**
 * Record the user's "no" for a group, then re-read.
 *
 * A group somebody already answered isn't an error: the dialog reloads so the user sees the
 * state that actually exists rather than being told their click failed.
 */
export async function rejectGroup(groupId: number): Promise<void> {
  if (suggestedOpsState.busyGroupId !== null) return
  suggestedOpsState.busyGroupId = groupId
  suggestedOpsState.decisionNotice = null
  suggestedOpsState.refusals.delete(groupId)
  try {
    const result = await rejectSuggestedGroup(groupId)
    if (result.kind !== 'rejected') {
      log.info('A suggestion group was already answered before this rejection: {kind}', { kind: result.kind })
    }
    if (suggestedOpsState.openGroupId === groupId) collapseGroup()
    await refreshSuggestions()
  } catch (e) {
    log.warn("Couldn't record the rejection: {error}", { error: String(e) })
    await answerFailedDecision(e)
  } finally {
    suggestedOpsState.busyGroupId = null
  }
}

/**
 * After an Approve or Reject that threw: re-read, then say what happened.
 *
 * The re-read comes first because the store is the only thing that knows whether the decision
 * landed, and the group can read as pending while its ops already run.
 */
async function answerFailedDecision(error: unknown): Promise<void> {
  await refreshSuggestions()
  suggestedOpsState.decisionNotice = decisionNoticeFor(error)
}

/** Only a dead approval worker leaves "did it start?" open. A store refusal happens before the
 *  claim transaction commits, so nothing ran and pressing again is safe. */
function decisionNoticeFor(error: unknown): DecisionNotice {
  switch (asSuggestedOpsError(error)?.type) {
    case 'approvalDidntFinish':
      return 'suggestedOps.approvalUnsure'
    case 'storeNotOpen':
    case 'store':
    case 'unexpected':
    case undefined:
      return 'suggestedOps.decisionNotRecorded'
  }
}
