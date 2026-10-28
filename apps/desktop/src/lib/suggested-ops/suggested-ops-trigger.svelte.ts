/**
 * Reactive state and the open/close seam for the AI suggestions dialog.
 *
 * The dialog's job is to lay a suggestion out so the user can decide, which shapes three
 * things here more than any convenience would:
 *
 * - **Every group shows its files right away, and each loads a WINDOW at a time.** A group of
 *   60,000 is legitimate, so nothing loads a group to show it: each group's list knows its
 *   `total` from a `COUNT(*)` and fetches the rows its viewport actually reaches. The list starts
 *   at `INITIAL_ROWS` and grows only when the user asks.
 * - **Selection holds only what the user flipped by hand.** A review opens on the STORED ticks
 *   (an earlier "no" stays off) and records flips against them; select all / none reset to an
 *   empty set over all-or-nothing. Either way the wire stays small at any group size: the user
 *   ticks or unticks a few rows by hand, never 60,000.
 * - **A group that changed under an open review is ANNOUNCED, never swapped.** The rows the
 *   user is reading stay put and a notice offers the reload, because re-ordering a list
 *   somebody is halfway through deciding on is how a wrong row gets approved.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { tick } from 'svelte'
import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { getAppLogger } from '#lib/logging/logger.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import {
  type OpSelection as WireOpSelection,
  approveSuggestedGroup,
  approveSuggestedServer,
  listSuggestedOps,
  onSuggestionsChanged,
  pageSuggestedOps,
  rejectSuggestedGroup,
  liveSuggestedOpFacts,
  type ApprovalResultView,
  type LiveFileState,
  type ServerApprovalResultView,
  type SuggestedGroupView,
  type SuggestedOpLiveFactsAnswer,
  type SuggestedOpPage,
  type SuggestedOpView,
  type SuggestedSweepView,
} from '#lib/tauri-commands/index.js'
import { asSuggestedOpsError } from './suggested-ops-failure'

const log = getAppLogger('suggestedOps')

/** How many op rows one fetch pulls. Comfortably more than a viewport, so ordinary scrolling
 *  reads from memory and only a jump costs a round trip. */
export const OP_WINDOW_SIZE = 200

/** Rows each group's list shows before the user asks for more. */
export const INITIAL_ROWS = 10

/** How many more rows "Show more" adds. */
export const SHOW_MORE_STEP = 20

/** The rows currently held for one group. */
interface OpWindow {
  /** Index of `ops[0]` within the whole group. */
  offset: number
  ops: SuggestedOpView[]
  /** Every op row the group has, from `COUNT(*)`. The list sizes itself from this. */
  total: number
}

/** What Cmdr found on disk for one op just now, or that it couldn't tell. */
export type LiveFact = LiveFileState

/** Everything the dialog holds about one group while the user reads it. */
export interface GroupReview {
  window: OpWindow | null
  windowLoading: boolean
  /** True when the group's last page read threw. The list says so and offers a retry, rather
   *  than leaving its rows on a loading placeholder nothing will ever fill. */
  windowError: boolean
  /** How many rows the list shows: `INITIAL_ROWS`, grown by "Show more" or "Show all". */
  shown: number
  /** Which ops are ticked. Survives scrolling: it is keyed by op id, not by row position. */
  selection: OpSelection
  /** The group `revision` whose rows are on screen. Approve sends it, so the store refuses to
   *  run a version the user hasn't read. Set when the review first reads, and on a reload. */
  seenRevision: number | null
  /** Set when the agent amended this group, or a re-read found its op count moved. */
  changedUnderReview: boolean
  /** What the disk says now, per op id, for the rows that have been on screen. */
  liveFacts: SvelteMap<number, LiveFact>
  /** Set when the group's source volume can't be asked at all, so its rows stay on the
   *  snapshot and the dialog says why once, above the list. */
  sourceUnreachable: 'notConnected' | 'gone' | null
  /** The rows the list last drew, so a page that lands late still gets its live facts. */
  range: { start: number; end: number }
}

/**
 * Which of a group's ops would run, as flips over a base.
 *
 * - `fromStored`: the stored ticks (an op an earlier review turned off arrives `excluded`) with
 *   `exclude` turned off and `include` turned back on. The state a group opens in, so a reopened
 *   review keeps the user's earlier "no"s without loading every op to find them.
 * - `allExcept`: every op but `ids` (after the header's select all).
 * - `only`: just `ids` (after the header's select none).
 *
 * The sets only ever hold rows the user flipped by hand.
 */
export type OpSelection =
  | { mode: 'fromStored'; exclude: SvelteSet<number>; include: SvelteSet<number> }
  | { mode: 'allExcept' | 'only'; ids: SvelteSet<number> }

/** The stored ticks, nothing flipped: the state a group opens in. */
export function storedSelection(): OpSelection {
  return { mode: 'fromStored', exclude: new SvelteSet<number>(), include: new SvelteSet<number>() }
}

/** The selection as approve sends it. */
function selectionWire(selection: OpSelection): WireOpSelection {
  if (selection.mode === 'fromStored') {
    return { mode: 'fromStored', excludeOpIds: [...selection.exclude], includeOpIds: [...selection.include] }
  }
  return { mode: selection.mode, opIds: [...selection.ids] }
}

/** Why the last Approve or Reject didn't happen, as the notice the dialog shows. */
export type DecisionNotice = Extract<MessageKey, 'suggestedOps.decisionNotRecorded' | 'suggestedOps.approvalUnsure'>

/** Why a file group's approval didn't start. */
export type FileGroupRefusal = Extract<
  ApprovalResultView,
  { kind: 'refused' | 'nothingToRun' | 'nothingSelected' | 'couldNotStart' }
>

/** Why a server card's save gave it back: nothing was written. */
export type ServerRefusal = Extract<
  ServerApprovalResultView,
  { kind: 'refused' | 'addServer' | 'serverChanged' | 'alreadySaved' | 'nameTaken' }
>

/**
 * Why an approval didn't start. The group is pending again, and the dialog shows this under it.
 * A server card's answer rides under its own `kind`: its `refused` carries a saved-server
 * outcome, not a write error.
 */
export type GroupRefusal = FileGroupRefusal | { kind: 'server'; answer: ServerRefusal }

interface SuggestedOpsState {
  open: boolean
  /** True while the sweep list is loading (the dialog shows a spinner). */
  loading: boolean
  /** True when the read threw: the dialog says so rather than showing an empty list, which
   *  would read as "the agent has suggested nothing". */
  loadError: boolean
  sweeps: SuggestedSweepView[]
  /** One review per pending group, keyed by group id. */
  reviews: SvelteMap<number, GroupReview>
  /** The group whose decision is in flight, so its buttons can disable without freezing the
   *  rest of the dialog. */
  busyGroupId: number | null
  /** Why the last decision didn't happen, or `null`. Set AFTER the re-read, so it speaks about
   *  the list the user is now looking at, and cleared when the next decision starts. */
  decisionNotice: DecisionNotice | null
  /** Groups whose last approval refused to start, keyed by group id. Each one is pending again
   *  and keeps its reason under it until the user answers it again or closes the dialog. */
  refusals: SvelteMap<number, GroupRefusal>
  /** Where op pages and their live facts come from: `null` reads the suggestion store. The
   *  dialog gallery seeds its own, so a fixture group bigger than the rows it seeded still pages
   *  as you scroll (the store has no such group and would answer a total of 0, emptying the list). */
  rowSource: SuggestedOpRowSource | null
}

/** One read a `SuggestedOpRowSource` answers: rows `offset` to `offset + limit` of a group. */
export interface SuggestedOpRowAsk {
  groupId: number
  offset: number
  limit: number
}

/** A stand-in for the store's two row reads, with the same answers (`suggestedOpsState.rowSource`). */
export interface SuggestedOpRowSource {
  page: (ask: SuggestedOpRowAsk) => Promise<SuggestedOpPage>
  liveFacts: (ask: SuggestedOpRowAsk) => Promise<SuggestedOpLiveFactsAnswer>
}

export const suggestedOpsState = $state<SuggestedOpsState>({
  open: false,
  loading: false,
  loadError: false,
  sweeps: [],
  reviews: new SvelteMap<number, GroupReview>(),
  busyGroupId: null,
  decisionNotice: null,
  refusals: new SvelteMap<number, GroupRefusal>(),
  rowSource: null,
})

function readPage(ask: SuggestedOpRowAsk): Promise<SuggestedOpPage> {
  const source = suggestedOpsState.rowSource
  return source ? source.page(ask) : pageSuggestedOps(ask.groupId, ask.offset, ask.limit)
}

function readLiveFacts(ask: SuggestedOpRowAsk): Promise<SuggestedOpLiveFactsAnswer> {
  const source = suggestedOpsState.rowSource
  return source ? source.liveFacts(ask) : liveSuggestedOpFacts(ask.groupId, ask.offset, ask.limit)
}

/** Op ids whose live facts were already asked for, per group. Not state: nothing renders it,
 *  it only keeps a scroll from asking twice. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping only; nothing renders from it
const factsRequested = new Map<number, Set<number>>()

/** A fresh review, reactive field by field. */
export function newGroupReview(): GroupReview {
  const review = $state<GroupReview>({
    window: null,
    windowLoading: false,
    windowError: false,
    shown: INITIAL_ROWS,
    selection: storedSelection(),
    seenRevision: null,
    changedUnderReview: false,
    liveFacts: new SvelteMap<number, LiveFact>(),
    sourceUnreachable: null,
    range: { start: 0, end: INITIAL_ROWS },
  })
  return review
}

/** Every group still waiting, flattened out of its sweep. */
export function pendingGroups(): SuggestedGroupView[] {
  return suggestedOpsState.sweeps.flatMap((sweep) => sweep.groups)
}

function groupById(groupId: number): SuggestedGroupView | undefined {
  return pendingGroups().find((group) => group.groupId === groupId)
}

/** The review for a group, or `undefined` once it's gone. */
export function reviewOf(groupId: number): GroupReview | undefined {
  return suggestedOpsState.reviews.get(groupId)
}

/** Every op row the group has: the page's `COUNT(*)` once one landed, the header's until then. */
export function totalRows(group: SuggestedGroupView): number {
  return reviewOf(group.groupId)?.window?.total ?? group.totalOpCount
}

/** How many rows the group's list draws right now. */
export function shownRows(group: SuggestedGroupView): number {
  const review = reviewOf(group.groupId)
  return Math.min(review?.shown ?? INITIAL_ROWS, totalRows(group))
}

/** "Show more": `SHOW_MORE_STEP` more rows, never past the end. */
export function showMoreRows(groupId: number): void {
  const review = reviewOf(groupId)
  if (review) review.shown += SHOW_MORE_STEP
}

/** "Show all": the whole group, still drawn a window at a time. */
export function showAllRows(groupId: number): void {
  const review = reviewOf(groupId)
  const group = groupById(groupId)
  if (review && group) review.shown = totalRows(group)
}

/** How many ops of a group would actually run. Derived from the COUNTs and the hand-flipped
 *  ids, so it's right even for rows never fetched.
 *
 *  `fromStored` starts from the stored live set (`liveOpCount`, which leaves out ops an earlier
 *  review excluded); `allExcept` counts down from EVERY op row (`totalOpCount`), since the
 *  backend starts that mode from everything the group offers. */
export function approvableCount(groupId: number): number {
  const group = groupById(groupId)
  const selection = reviewOf(groupId)?.selection
  if (!group) return 0
  if (!selection) return group.liveOpCount
  switch (selection.mode) {
    case 'fromStored':
      return Math.max(0, group.liveOpCount - selection.exclude.size + selection.include.size)
    case 'only':
      return selection.ids.size
    case 'allExcept':
      return Math.max(0, group.totalOpCount - selection.ids.size)
  }
}

/** Whether Approve may run: not while the group changed under the review, since the version
 *  on screen isn't the one that would run until the user reads the new one. */
export function canApprove(groupId: number): boolean {
  if (reviewOf(groupId)?.changedUnderReview === true) return false
  // A server card has no ops to count: the card itself is what the click saves.
  return Boolean(groupById(groupId)?.server) || approvableCount(groupId) > 0
}

/** Whether one op is ticked. Under the stored base, an op an earlier review excluded starts off. */
export function isOpSelected(groupId: number, op: SuggestedOpView): boolean {
  const selection = reviewOf(groupId)?.selection
  if (!selection) return op.status !== 'excluded'
  switch (selection.mode) {
    case 'fromStored':
      return op.status === 'excluded' ? selection.include.has(op.opId) : !selection.exclude.has(op.opId)
    case 'allExcept':
      return !selection.ids.has(op.opId)
    case 'only':
      return selection.ids.has(op.opId)
  }
}

/** The header checkbox: every op ticked, none, or a mix. */
export function selectionState(groupId: number): 'all' | 'none' | 'some' {
  const group = groupById(groupId)
  const count = approvableCount(groupId)
  if (count === 0) return 'none'
  if (group && count >= group.totalOpCount) return 'all'
  return 'some'
}

/** The header checkbox's click: a full list unticks everything, anything else ticks everything. */
export function toggleAllOps(groupId: number): void {
  const review = reviewOf(groupId)
  if (!review) return
  review.selection = {
    mode: selectionState(groupId) === 'all' ? 'only' : 'allExcept',
    ids: new SvelteSet<number>(),
  }
}

/** Live only while the dialog is open: the badge owns the session-long subscription. */
let unlistenChanges: UnlistenFn | null = null

/**
 * Closes the dialog. The last review's state stays put until the next open drops it:
 * clearing it here, in the tick that unmounts the dialog, pulls each row's op out from
 * under its cells mid-teardown (Ark's checkbox reads `checked` on the way out), and the
 * throw takes the whole main window down to its crash panel.
 */
export function closeSuggestedOps(): void {
  suggestedOpsState.open = false
  unlistenChanges?.()
  unlistenChanges = null
}

/** Every review, refusal, and notice the last open left behind. Only while nothing renders them. */
function forgetLastReview(): void {
  suggestedOpsState.decisionNotice = null
  suggestedOpsState.reviews.clear()
  factsRequested.clear()
  suggestedOpsState.refusals.clear()
}

/**
 * Opens the dialog and loads what's waiting. Idempotent, so a menu, palette, and shortcut
 * triple-fire opens it once. Always opens, even on a read failure: a dead menu item is worse
 * than a dialog that says it couldn't read.
 */
export async function openSuggestedOps(): Promise<void> {
  if (suggestedOpsState.open) return
  // Before `open` flips, so the dialog mounts over a clean slate and never sees the old one.
  forgetLastReview()
  suggestedOpsState.open = true
  await subscribeWhileOpen()
  await refreshSuggestions()
}

/**
 * Listen for changes while the dialog is up.
 *
 * The interesting case is `amended` on a group the user is reading. A count comparison alone
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
    // review: every refresh still compares each group's op count, so a change that alters it
    // is still announced.
    log.warn("Couldn't subscribe to suggestion changes: {error}", { error: e })
  }
}

function listenForChanges(): Promise<UnlistenFn> {
  return onSuggestionsChanged((payload) => {
    if (payload.reason === 'amended' && payload.groupId !== null) {
      const review = reviewOf(payload.groupId)
      // The rows on screen stay exactly where they are; the notice offers the reload.
      if (review) review.changedUnderReview = true
    }
    void refreshSuggestions()
  })
}

/**
 * Re-read the waiting sweeps, then give every new group its review and first page.
 *
 * A group already under review keeps its rows: when its revision moved past the one on screen,
 * it's announced, never swapped. A group that's gone (answered here or elsewhere, or withdrawn)
 * drops its review, but only after the flush that takes its rows off screen: dropping it in the
 * same tick pulls each row's op out from under its cells mid-teardown (Ark's checkbox reads
 * `checked` on the way out), and the throw takes the main window down to its crash panel.
 */
export async function refreshSuggestions(): Promise<void> {
  suggestedOpsState.loading = true
  suggestedOpsState.loadError = false
  const fresh: number[] = []
  const gone: number[] = []
  try {
    suggestedOpsState.sweeps = await listSuggestedOps()
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local for one pass; nothing renders from it
    const live = new Set<number>()
    for (const group of pendingGroups()) {
      live.add(group.groupId)
      const review = reviewOf(group.groupId)
      if (!review) {
        const created = newGroupReview()
        created.seenRevision = group.revision
        suggestedOpsState.reviews.set(group.groupId, created)
        fresh.push(group.groupId)
      } else if (review.seenRevision !== null && review.seenRevision !== group.revision) {
        // The group moved since the rows on screen were read: announce it, never swap.
        review.changedUnderReview = true
      }
    }
    for (const groupId of suggestedOpsState.reviews.keys()) {
      if (!live.has(groupId)) gone.push(groupId)
    }
  } catch (e) {
    suggestedOpsState.loadError = true
    log.warn("Couldn't read the waiting suggestions: {error}", { error: e })
  } finally {
    suggestedOpsState.loading = false
  }
  if (gone.length > 0) {
    await tick()
    // A re-read that landed during the tick may have brought one back: it keeps its review.
    for (const groupId of gone) if (!groupById(groupId)) dropReview(groupId)
  }
  // Every list shows right away, so each new group reads its first page now, side by side.
  await Promise.all(fresh.map((groupId) => ensureOpWindow(groupId, 0)))
}

function dropReview(groupId: number): void {
  suggestedOpsState.reviews.delete(groupId)
  factsRequested.delete(groupId)
}

/**
 * "Show the new version" on a group the agent changed: read its rows again from the top.
 *
 * The selection survives on purpose. An op the user turned off stays off if the new version
 * still has it; resetting it would quietly turn their "no" back into a "yes".
 */
export async function reloadGroup(groupId: number): Promise<void> {
  const review = reviewOf(groupId)
  if (!review) return
  review.window = null
  review.liveFacts.clear()
  review.sourceUnreachable = null
  factsRequested.delete(groupId)
  await refreshSuggestions()
  // The rows about to load are the new version: that's the one the user now reads.
  review.seenRevision = groupById(groupId)?.revision ?? review.seenRevision
  review.changedUnderReview = false
  await ensureOpWindow(groupId, 0)
}

/**
 * The list drew rows `start` to `end`: make sure their page is held, then check them on disk.
 *
 * The list calls this as it scrolls. A request for rows already held is a no-op, so ordinary
 * scrolling inside a window costs nothing.
 */
export async function showRange(groupId: number, start: number, end: number): Promise<void> {
  const review = reviewOf(groupId)
  if (!review) return
  review.range = { start, end }
  await ensureOpWindow(groupId, start, end)
  await ensureLiveFacts(groupId)
}

/**
 * Make sure rows `start` to `end` are loaded, fetching a window when any of them isn't.
 *
 * The WHOLE range, not its first row alone: checking only `start` left the bottom of the drawn
 * range on blank placeholders whenever it ran past the held window's end, until the top row
 * crossed too. The list's range changes call this, and so does the retry after a failed read.
 */
export async function ensureOpWindow(groupId: number, start: number, end = start + 1): Promise<void> {
  const review = reviewOf(groupId)
  // A server card has no ops: nothing to page, and with no window, nothing to check on disk.
  if (!review || groupById(groupId)?.server) return
  if (windowHolds(review.window, start, end)) return
  if (review.windowLoading) return

  // Centre the window on what was asked for, so scrolling either way stays inside it: a
  // quarter of the room the range leaves goes above it, the rest below.
  const slack = Math.max(0, OP_WINDOW_SIZE - (end - start))
  const offset = Math.max(0, start - Math.floor(slack / 4))
  review.windowLoading = true
  review.windowError = false
  const rangeAtAsk = { ...review.range }
  try {
    const page = await readPage({ groupId, offset, limit: OP_WINDOW_SIZE })
    // The group may have been answered, or the dialog closed, while this was in flight.
    if (reviewOf(groupId) !== review) return
    review.window = { offset: page.offset, ops: page.ops, total: page.total }
    // A scroll made while this page was in flight was turned away above, and the list won't ask
    // for that range again, so follow it. Only a range that MOVED meanwhile: one past the
    // group's end is never held, and must not loop.
    const now = review.range
    const moved = now.start !== rangeAtAsk.start || now.end !== rangeAtAsk.end
    if (moved && !windowHolds(review.window, now.start, now.end)) {
      review.windowLoading = false
      await ensureOpWindow(groupId, now.start, now.end)
      return
    }
  } catch (e) {
    // The list only asks again when its range moves, so without this the rows would sit on
    // their loading placeholder for good while Approve stays live.
    review.windowError = true
    log.warn("Couldn't read a page of proposed ops: {error}", { error: e })
    return
  } finally {
    review.windowLoading = false
  }
  // A page that lands after the list already drew its rows still owes them their live facts.
  await ensureLiveFacts(groupId)
}

/**
 * Whether the held window covers rows `start` to `end`. A range wider than one page can't be
 * held whole, so only its first `OP_WINDOW_SIZE` rows count; otherwise every scroll would page.
 */
function windowHolds(held: OpWindow | null, start: number, end: number): boolean {
  if (held === null) return false
  const last = Math.max(start, Math.min(end, start + OP_WINDOW_SIZE) - 1)
  return start >= held.offset && last < held.offset + held.ops.length
}

/**
 * Check the drawn rows on disk: size and modified time now, or that the file is gone.
 *
 * Only the rows on screen, only once each: a 60,000-op group shown whole still costs one stat
 * per row the user actually scrolls past. One call covers the span of drawn rows not yet asked
 * about.
 *
 * Every answer short of a fact leaves rows on their creation snapshot, which the dialog labels
 * as such. A source that can't be asked at all (not connected, gone) is recorded once on the
 * review and not asked again; the dialog says it once for the group. Anything else is logged,
 * and the rows un-marked so the next scroll asks again.
 */
export async function ensureLiveFacts(groupId: number): Promise<void> {
  const review = reviewOf(groupId)
  const held = review?.window
  if (!review || !held || review.sourceUnreachable !== null) return
  let asked = factsRequested.get(groupId)
  if (!asked) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping only, see `factsRequested`
    asked = new Set<number>()
    factsRequested.set(groupId, asked)
  }
  const span = unaskedSpan(review, held, asked)
  if (!span) return
  for (const opId of span.opIds) asked.add(opId)
  try {
    const answer = await readLiveFacts({ groupId, offset: span.offset, limit: span.opIds.length })
    if (reviewOf(groupId) !== review) return
    if (takeLiveAnswer(review, answer)) return
  } catch (e) {
    log.warn("Couldn't check the suggested files on disk: {error}", { error: e })
  }
  for (const opId of span.opIds) asked.delete(opId)
}

/** The drawn rows nobody has asked about yet, as one contiguous span of the held page. */
function unaskedSpan(
  review: GroupReview,
  held: OpWindow,
  asked: Set<number>,
): { offset: number; opIds: number[] } | null {
  const from = Math.max(review.range.start, held.offset)
  const to = Math.min(review.range.end, held.offset + held.ops.length)
  let first = -1
  let last = -1
  for (let index = from; index < to; index++) {
    const opId = held.ops[index - held.offset].opId
    if (asked.has(opId) || review.liveFacts.has(opId)) continue
    if (first < 0) first = index
    last = index
  }
  if (first < 0) return null
  return { offset: first, opIds: held.ops.slice(first - held.offset, last + 1 - held.offset).map((op) => op.opId) }
}

/** Record an answer. `false` means it held nothing to keep, so the rows should be asked again. */
function takeLiveAnswer(review: GroupReview, answer: SuggestedOpLiveFactsAnswer): boolean {
  if (answer.kind === 'facts') {
    for (const { opId, state } of answer.facts) review.liveFacts.set(opId, state)
    return true
  }
  switch (answer.reason.type) {
    case 'sourceNotConnected':
      review.sourceUnreachable = 'notConnected'
      return true
    case 'sourceGone':
      review.sourceUnreachable = 'gone'
      return true
    case 'unknownGroup':
    case 'storeNotOpen':
    case 'store':
    case 'unexpected':
      log.warn("Couldn't check the suggested files on disk: {reason}", { reason: answer.reason })
      return false
  }
}

/** The op at a whole-group index, or `null` when its window isn't loaded yet. */
export function opAt(groupId: number, index: number): SuggestedOpView | null {
  const held = reviewOf(groupId)?.window
  if (!held) return null
  return held.ops[index - held.offset] ?? null
}

/** Turn one op off, or back on: flip its membership in whichever set records it against the base. */
export function toggleOp(groupId: number, op: SuggestedOpView): void {
  const selection = reviewOf(groupId)?.selection
  if (!selection) return
  const flips =
    selection.mode !== 'fromStored' ? selection.ids : op.status === 'excluded' ? selection.include : selection.exclude
  if (flips.has(op.opId)) {
    flips.delete(op.opId)
  } else {
    flips.add(op.opId)
  }
}

/**
 * Approve a group: claim it and hand its ops to the queue.
 *
 * Sends the selection in its own mode: the ids the user flipped by hand, never the whole list,
 * and nothing at all while the group changed under the review. On success the dialog gets out
 * of the way once nothing is left, and the queue surface takes over, which is the hand-off the
 * whole feature is built around: an approved op is an ordinary queued op from here on.
 *
 * A refusal is not a failure to hide. Each variant means a different thing to the user, so the
 * dialog re-reads and lets them see the state that actually exists. A refusal to START gives
 * the group back: nothing ran, it's pending again, and its reason sits under it, so the user is
 * where they were and knows why.
 */
export async function approveGroup(groupId: number): Promise<void> {
  if (suggestedOpsState.busyGroupId !== null || !canApprove(groupId)) return
  suggestedOpsState.busyGroupId = groupId
  suggestedOpsState.decisionNotice = null
  suggestedOpsState.refusals.delete(groupId)
  try {
    // The revision the user read, never the one the store holds now: an amendment that landed
    // since refuses as `listChanged` instead of running rows nobody saw.
    const revision = reviewOf(groupId)?.seenRevision ?? groupById(groupId)?.revision ?? 0
    // A server card has no ops to select: it saves through its own command.
    if (groupById(groupId)?.server) {
      await approveServerGroup(groupId, revision)
    } else {
      await approveFileGroup(groupId, revision)
    }
  } catch (e) {
    log.warn("Couldn't approve the group: {error}", { error: e })
    await answerFailedDecision(e)
  } finally {
    suggestedOpsState.busyGroupId = null
  }
}

/** Approve a file group with the user's selection, then re-read. */
async function approveFileGroup(groupId: number, revision: number): Promise<void> {
  const selection = reviewOf(groupId)?.selection ?? storedSelection()
  const result = await approveSuggestedGroup(groupId, revision, selectionWire(selection))
  if (isRefusalToStart(result)) suggestedOpsState.refusals.set(groupId, result)
  if (result.kind !== 'started') {
    log.info('A suggestion group was not approved: {kind}', { kind: result.kind })
  }
  if (result.kind === 'listChanged') markChangedUnderReview(groupId)
  await refreshSuggestions()
  // Nothing waiting means nothing left to decide, so the dialog closes itself rather than
  // sitting there empty over the queue that just took over.
  if (result.kind === 'started' && suggestedOpsState.sweeps.length === 0) closeSuggestedOps()
}

/**
 * Approve a server card: save it, then re-read. A refusal gives the card back pending with its
 * reason under it, so the re-read shows it again; `listChanged` marks it changed under the
 * review, as for a file group.
 */
async function approveServerGroup(groupId: number, revision: number): Promise<void> {
  const result = await approveSuggestedServer(groupId, revision)
  if (result.kind !== 'saved') {
    log.info('A server suggestion was not saved: {kind}', { kind: result.kind })
  }
  if (isServerRefusal(result)) suggestedOpsState.refusals.set(groupId, { kind: 'server', answer: result })
  if (result.kind === 'listChanged') markChangedUnderReview(groupId)
  await refreshSuggestions()
  if (result.kind === 'saved' && suggestedOpsState.sweeps.length === 0) closeSuggestedOps()
}

function markChangedUnderReview(groupId: number): void {
  const review = reviewOf(groupId)
  if (review) review.changedUnderReview = true
}

/** The answers that give a server card back with a reason: the save wrote nothing. */
function isServerRefusal(result: ServerApprovalResultView): result is ServerRefusal {
  switch (result.kind) {
    case 'refused':
    case 'addServer':
    case 'serverChanged':
    case 'alreadySaved':
    case 'nameTaken':
      return true
    case 'saved':
    case 'listChanged':
    case 'alreadyAnswered':
    case 'unknown':
      return false
  }
}

/** The answers that give a group back with a reason, as opposed to closing it or re-reading. */
function isRefusalToStart(result: ApprovalResultView): result is FileGroupRefusal {
  switch (result.kind) {
    case 'refused':
    case 'nothingToRun':
    case 'nothingSelected':
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
    await refreshSuggestions()
  } catch (e) {
    log.warn("Couldn't record the rejection: {error}", { error: e })
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
