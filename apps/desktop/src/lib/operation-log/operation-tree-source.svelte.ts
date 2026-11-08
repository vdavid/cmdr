/**
 * The item tree of the operation the Operation log's cursor sits on: its hand toggles
 * (expanded folders, the checkbox selection), its cursor, where it's scrolled, the last
 * window the backend answered, and the backfill an old operation needs before its tree
 * can open.
 *
 * ❌ No tree model here: `get_operation_log_tree` owns expansion, compaction, windows,
 * tri-state, and "N of M" (`crates/cmdr-operation-log/DETAILS.md` § "The tree view").
 * This holds only what the backend can't know, and sends the selection back exactly as
 * the last answer normalized it.
 *
 * Two rules keep fast input honest (`DETAILS.md` § "The tree source"):
 * - **One request in flight, generation-tagged.** Every local edit bumps `generation`;
 *   an answer is adopted only when it was asked at the latest one, so Space, Space fast
 *   can't let the first answer wipe the second toggle. The pressed row's checkbox shows
 *   its new state at once (`checkboxOf`) until that answer lands.
 * - **The operation settles before its tree loads.** `show` takes the cursor's operation
 *   at once (the header renders from the row), but reads its tree, and starts any
 *   backfill, only once the cursor has rested {@link SETTLE_MS}.
 */

import type {
  OperationRow,
  OperationTreeRow,
  OperationTreeView,
  RollbackSelection,
  SelectionToggle,
  TreeBackfillEvent,
  TreeCheckbox,
  TreeRowKey,
  UnbuiltReason,
} from '#lib/ipc/bindings.js'
// Straight from the module, not the barrel: only this file reads the tree.
import {
  cancelOperationLogTreeBackfill,
  getOperationLogTree,
  onOperationLogTreeBackfill,
  onOperationLogTreeIndexesSettled,
  startOperationLogTreeBackfill,
} from '#lib/tauri-commands/operation-log.js'
import type { ColumnListWindowedSource } from '#lib/ui/column-list-types.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { createCoalesced } from '#lib/utils/timing.js'
import { SvelteMap } from 'svelte/reactivity'

const log = getAppLogger('operationLogTree')

/** How long the op-list cursor rests before its tree loads: holding ↓ through 50 operations reads one tree. */
export const SETTLE_MS = 150
/** A load quicker than this shows no spinner at all. */
export const SPINNER_DELAY_MS = 200
/** A backfill shorter than this shows a spinner; a longer one earns a real progress bar. */
export const BACKFILL_PROGRESS_DELAY_MS = 10_000
/** Rows asked for above and below the viewport, so a short scroll lands on loaded rows. */
export const OVERSCAN_ROWS = 20
/** The most rows one answer carries: the backend's own cap. */
const MAX_LIMIT = 200
/** The most expanded folders a request carries; the oldest go first. */
const MAX_EXPANDED = 2_000
/** Top-level folders a fresh tree opens by itself, so a small operation reads at a glance. */
const AUTO_EXPAND_TOP_ROWS = 8

/** Why the tree has no rows to show, worded by the detail pane. */
export type TreeBlank =
  | { kind: 'loading' }
  | { kind: 'backfilling'; done: number; total: number }
  | { kind: 'indexesBuilding' }
  /** This launch's build of the journal's folder indexes couldn't finish; the next launch tries again. */
  | { kind: 'indexesUnavailable' }
  | { kind: 'unbuilt'; reason: UnbuiltReason }
  | { kind: 'gone' }
  | { kind: 'unreadable' }
  | { kind: 'empty' }

export function treeRowKeyId(key: TreeRowKey): string {
  return key.kind === 'folder' ? `folder-${String(key.dirId)}` : `item-${String(key.seq)}`
}

function sameKey(a: TreeRowKey | null, b: TreeRowKey | null): boolean {
  if (a === null || b === null) return a === b
  return treeRowKeyId(a) === treeRowKeyId(b)
}

/** A normalized selection that means exactly "everything", the untouched default. */
function isEverything(selection: RollbackSelection): boolean {
  return selection.base === 'everything' && selection.toggles.length === 0
}

export interface OperationTreeSourceOptions {
  settleMs?: number
  spinnerDelayMs?: number
  backfillProgressDelayMs?: number
}

export interface OperationTreeSource {
  /** The settled operation's id, or `null` while none has settled. */
  readonly opId: string | null
  /** The last window the backend answered for the settled operation. */
  readonly view: OperationTreeView | null
  /** Why there are no rows, or `null` when the tree has rows to show. */
  readonly blank: TreeBlank | null
  /** A load has run past {@link SPINNER_DELAY_MS}: time for a spinner. */
  readonly slow: boolean
  /** A backfill has run past {@link BACKFILL_PROGRESS_DELAY_MS}: time for a progress bar. */
  readonly backfillSlow: boolean
  /** The rows, for `ColumnList`. */
  readonly rows: ColumnListWindowedSource<OperationTreeRow>
  readonly cursorIndex: number
  /** The person narrowed the selection: a rollback sends it, rather than `null` for everything. */
  readonly narrowed: boolean
  /** The selection as the backend last normalized it; `null` while untouched. */
  readonly selection: RollbackSelection | null
  /** The last pick went past the toggle cap and was refused; the previous selection stands. */
  readonly togglesCapped: boolean
  /** The header's select-all box: on, off, mixed, or none when there's nothing to pick. */
  readonly allCheckbox: TreeCheckbox
  /** Follow the op-list cursor. The tree reads once it has rested {@link SETTLE_MS}. */
  show: (op: OperationRow | null) => void
  /** The checkbox a row shows: a press's answer while it's on its way, else the backend's. */
  checkboxOf: (row: OperationTreeRow) => TreeCheckbox
  toggleExpanded: (row: OperationTreeRow) => void
  toggleChecked: (row: OperationTreeRow) => void
  /** The header box: everything, or nothing when everything is on. */
  toggleAll: () => void
  /**
   * ⌘A (`true`) or ⌘⇧A (`false`): everything, or nothing. Idempotent, so the two halves of a
   * keyboard + menu double fire land as one pick; `false` when there's nothing to pick.
   */
  setAll: (on: boolean) => boolean
  setCursor: (index: number) => void
  /** The flat index of a loaded row's parent, `null` on a top-level row or one the window doesn't place. */
  parentIndexOf: (index: number) => number | null
  /**
   * Read the tree again: its outcomes changed (a reversal ended). `resetSelection` drops
   * the person's picks too, since they described a state that's gone.
   */
  refresh: (options?: { resetSelection?: boolean }) => void
  dispose: () => void
}

export function createOperationTreeSource({
  settleMs = SETTLE_MS,
  spinnerDelayMs = SPINNER_DELAY_MS,
  backfillProgressDelayMs = BACKFILL_PROGRESS_DELAY_MS,
}: OperationTreeSourceOptions = {}): OperationTreeSource {
  let opId = $state<string | null>(null)
  let view = $state.raw<OperationTreeView | null>(null)
  let blank = $state.raw<TreeBlank | null>(null)
  let slow = $state(false)
  let backfillSlow = $state(false)
  let expanded = $state.raw<TreeRowKey[]>([])
  let selection = $state.raw<RollbackSelection | null>(null)
  let togglesCapped = $state(false)
  let cursorIndex = $state(0)
  let cursorKey: TreeRowKey | null = null
  /** The checkbox a pressed row shows until the answer to its press lands, by `treeRowKeyId`. */
  const optimistic = new SvelteMap<string, TreeCheckbox>()
  let optimisticAll = $state<TreeCheckbox | null>(null)

  let windowStart = 0
  let windowEnd = 0
  /** Bumped by every local edit and every operation switch: only the latest answer is adopted. */
  let generation = 0
  /** The settled operation has asked for its backfill (and so owes a cancel on the way out). */
  let backfillAsked = false
  /** The settled operation's tree has answered once: the auto-expand happens only then. */
  let answeredOnce = false
  let disposed = false

  let settleTimer: ReturnType<typeof setTimeout> | undefined
  let spinnerTimer: ReturnType<typeof setTimeout> | undefined
  let backfillTimer: ReturnType<typeof setTimeout> | undefined

  function request(): { id: string; generation: number; cursor: TreeRowKey | null } | null {
    if (opId === null) return null
    return { id: opId, generation, cursor: cursorKey }
  }

  const pulls = createCoalesced<null>(async () => {
    const asked = request()
    if (asked === null || disposed) return
    const offset = Math.max(0, windowStart - OVERSCAN_ROWS)
    const limit = Math.min(MAX_LIMIT, Math.max(1, windowEnd - windowStart) + 2 * OVERSCAN_ROWS)
    try {
      const answer = await getOperationLogTree(asked.id, {
        expanded,
        selection,
        cursor: asked.cursor,
        offset,
        limit,
      })
      // A later edit or another operation has a newer request coming; this one describes a
      // screen that has moved on. Read through a call: the awaits above can change it.
      if (isStale(asked)) return
      adopt(answer, asked)
    } catch (error: unknown) {
      if (isStale(asked)) return
      log.warn("Couldn't read the tree of {opId}: {error}", { opId: asked.id, error })
      setBlank({ kind: 'unreadable' })
    }
  })

  function isStale(asked: { id: string; generation: number }): boolean {
    return disposed || asked.id !== opId || asked.generation !== generation
  }

  function pull(): void {
    void pulls.call(null)
  }

  function setBlank(next: TreeBlank | null): void {
    blank = next
    if (next === null || next.kind !== 'loading') {
      clearTimeout(spinnerTimer)
      slow = false
    }
  }

  function adopt(answer: OperationTreeView | null, asked: { cursor: TreeRowKey | null }): void {
    if (answer === null) {
      view = null
      setBlank({ kind: 'gone' })
      return
    }
    view = answer
    optimistic.clear()
    optimisticAll = null
    if (selection !== null) selection = isEverything(answer.selection) ? null : answer.selection
    togglesCapped = answer.togglesCapped
    // The cursor came back on its row (or its nearest open ancestor). A cursor that moved
    // while the answer was out keeps where the person put it.
    if (sameKey(asked.cursor, cursorKey) && answer.cursorIndex !== null) {
      cursorIndex = answer.cursorIndex
      cursorKey = answer.rows[cursorIndex - answer.offset]?.key ?? cursorKey
    }
    if (cursorKey === null) cursorKey = answer.rows[cursorIndex - answer.offset]?.key ?? null
    applySummary(answer)
  }

  function applySummary(answer: OperationTreeView): void {
    const summary = answer.summary
    switch (summary.kind) {
      case 'built':
        clearTimeout(backfillTimer)
        backfillSlow = false
        setBlank(answer.totalRows === 0 ? { kind: 'empty' } : null)
        if (!answeredOnce) {
          answeredOnce = true
          autoExpand(answer)
        }
        return
      case 'building':
        askForBackfill()
        setBlank({ kind: 'backfilling', done: summary.progress.done, total: summary.progress.total })
        return
      case 'indexesBuilding':
        // Read again when the backend says the build settled (`onIndexesSettled`), never on a timer.
        setBlank({ kind: 'indexesBuilding' })
        return
      case 'indexesUnavailable':
        setBlank({ kind: 'indexesUnavailable' })
        return
      case 'unbuilt':
        if (summary.reason === 'notBuilt') {
          askForBackfill()
          setBlank({ kind: 'backfilling', done: 0, total: 0 })
        } else {
          setBlank({ kind: 'unbuilt', reason: summary.reason })
        }
        return
    }
  }

  /** A small fresh tree opens its top-level folders, so it reads at a glance, as the mockup's do. */
  function autoExpand(answer: OperationTreeView): void {
    if (expanded.length > 0 || answer.totalRows > answer.rows.length) return
    const top = answer.rows.filter((row) => row.level === 1)
    if (top.length > AUTO_EXPAND_TOP_ROWS) return
    const folders = top.filter((row) => row.expandable && !row.expanded).map((row) => row.key)
    if (folders.length === 0) return
    expanded = folders
    generation++
    pull()
  }

  function askForBackfill(): void {
    if (backfillAsked || opId === null) return
    backfillAsked = true
    const id = opId
    clearTimeout(backfillTimer)
    backfillTimer = setTimeout(() => (backfillSlow = true), backfillProgressDelayMs)
    // Starting also JOINS a running one and takes back a cancel a closed dialog asked for.
    startOperationLogTreeBackfill(id)
      .then((start) => {
        if (disposed || id !== opId) return
        switch (start.kind) {
          case 'started':
          case 'joined':
            return
          case 'built':
            pull()
            return
          case 'unbuilt':
            backfillAsked = false
            setBlank({ kind: 'unbuilt', reason: start.detail })
            return
          case 'unknownOperation':
            backfillAsked = false
            setBlank({ kind: 'gone' })
            return
        }
      })
      .catch((error: unknown) => {
        backfillAsked = false
        if (disposed || id !== opId) return
        log.warn("Couldn't start the tree backfill of {opId}: {error}", { opId: id, error })
        setBlank({ kind: 'unreadable' })
      })
  }

  function onBackfill(event: TreeBackfillEvent): void {
    if (disposed || event.opId !== opId) return
    const step = event.step
    if (step.kind === 'progress') {
      setBlank({ kind: 'backfilling', done: step.detail.done, total: step.detail.total })
      return
    }
    backfillAsked = false
    clearTimeout(backfillTimer)
    backfillSlow = false
    switch (step.detail.kind) {
      case 'built':
        pull()
        return
      case 'unbuilt':
        setBlank({ kind: 'unbuilt', reason: step.detail.detail })
        return
      case 'gone':
        setBlank({ kind: 'gone' })
        return
      case 'unreadable':
        setBlank({ kind: 'unreadable' })
        return
      case 'cancelled':
        // Only this view cancels, and only for an operation it has left.
        return
    }
  }

  /**
   * The journal's folder-index build settled. Read again if this tree waits on it, or if
   * its first answer is still on the way (it may have been asked before it settled).
   */
  function onIndexesSettled(): void {
    if (disposed || opId === null) return
    if (blank?.kind === 'indexesBuilding' || blank?.kind === 'loading') pull()
  }

  const unlisteners: (() => void)[] = []
  function follow(subscribe: Promise<() => void>, what: string, onSubscribed?: () => void): void {
    subscribe
      .then((stop) => {
        if (disposed) {
          stop()
          return
        }
        unlisteners.push(stop)
        onSubscribed?.()
      })
      .catch((error: unknown) => {
        log.warn("Couldn't follow {what}: {error}", { what, error })
      })
  }
  follow(onOperationLogTreeBackfill(onBackfill), 'tree backfills')
  // The event fires once per launch, so one that fired before this listened is gone: a
  // tree told "building" while subscribing asks once more, and finds them built.
  follow(onOperationLogTreeIndexesSettled(onIndexesSettled), 'the folder indexes', onIndexesSettled)

  function leaveOperation(): void {
    if (backfillAsked && opId !== null) {
      void cancelOperationLogTreeBackfill(opId).catch((error: unknown) => {
        log.debug("Couldn't cancel a tree backfill: {error}", { error })
      })
    }
    backfillAsked = false
    clearTimeout(backfillTimer)
    backfillSlow = false
  }

  function settle(next: string | null): void {
    leaveOperation()
    opId = next
    generation++
    view = null
    expanded = []
    selection = null
    togglesCapped = false
    optimistic.clear()
    optimisticAll = null
    cursorIndex = 0
    cursorKey = null
    windowStart = 0
    answeredOnce = false
    if (next === null) {
      setBlank(null)
      return
    }
    setBlank({ kind: 'loading' })
    clearTimeout(spinnerTimer)
    spinnerTimer = setTimeout(() => (slow = blank?.kind === 'loading'), spinnerDelayMs)
    pull()
  }

  /** The window's row at a flat index, if the last answer holds it. */
  function rowAt(index: number): OperationTreeRow | undefined {
    if (view === null || blank !== null) return undefined
    return view.rows[index - view.offset]
  }

  function covered(start: number, end: number): boolean {
    if (view === null) return false
    const last = Math.min(end, view.totalRows)
    return view.offset <= start && view.offset + view.rows.length >= last
  }

  function edit(): void {
    generation++
    togglesCapped = false
    pull()
  }

  function addToggle(toggle: SelectionToggle): void {
    const base = selection ?? { base: 'everything', toggles: [] }
    selection = { base: base.base, toggles: [...base.toggles, toggle] }
    edit()
  }

  /** The select-all box, from the op's "N of M": `none` on a read-only op or one with nothing to pick. */
  function allCheckbox(): TreeCheckbox {
    if (view === null || view.readOnly) return 'none'
    if (optimisticAll !== null) return optimisticAll
    const { selected, selectable } = view.tally
    if (selectable === 0) return 'none'
    if (selected === 0) return 'off'
    return selected === selectable ? 'on' : 'mixed'
  }

  function setAll(on: boolean): boolean {
    if (allCheckbox() === 'none') return false
    const base = on ? 'everything' : 'nothing'
    const current = selection ?? { base: 'everything', toggles: [] }
    if (current.base === base && current.toggles.length === 0) return true
    optimistic.clear()
    optimisticAll = on ? 'on' : 'off'
    selection = { base, toggles: [] }
    edit()
    return true
  }

  const rows: ColumnListWindowedSource<OperationTreeRow> = {
    get count() {
      return view === null || blank !== null ? 0 : view.totalRows
    },
    getRow: rowAt,
    onRangeChange: ({ start, end }) => {
      windowStart = start
      windowEnd = end
      if (!covered(start, end)) pull()
    },
  }

  return {
    get opId() {
      return opId
    },
    get view() {
      return view
    },
    get blank() {
      return blank
    },
    get slow() {
      return slow
    },
    get backfillSlow() {
      return backfillSlow
    },
    rows,
    get cursorIndex() {
      return cursorIndex
    },
    get narrowed() {
      return selection !== null
    },
    get selection() {
      return selection
    },
    get togglesCapped() {
      return togglesCapped
    },
    get allCheckbox() {
      return allCheckbox()
    },
    show(op) {
      const next = op?.opId ?? null
      clearTimeout(settleTimer)
      if (next === opId) return
      settleTimer = setTimeout(() => {
        settle(next)
      }, settleMs)
    },
    checkboxOf(row) {
      return optimistic.get(treeRowKeyId(row.key)) ?? row.checkbox
    },
    toggleExpanded(row) {
      if (!row.expandable) return
      const id = treeRowKeyId(row.key)
      const open = expanded.some((key) => treeRowKeyId(key) === id)
      const next = expanded.filter((key) => treeRowKeyId(key) !== id)
      if (!open) next.push(row.key)
      expanded = next.length > MAX_EXPANDED ? next.slice(next.length - MAX_EXPANDED) : next
      edit()
    },
    toggleChecked(row) {
      if (view === null || view.readOnly || row.role === 'contents') return
      const current = optimistic.get(treeRowKeyId(row.key)) ?? row.checkbox
      if (current === 'none') return
      const on = current !== 'on'
      optimistic.set(treeRowKeyId(row.key), on ? 'on' : 'off')
      addToggle(
        row.key.kind === 'folder'
          ? { kind: 'folder', dirId: row.key.dirId, on }
          : { kind: 'item', seq: row.key.seq, on },
      )
    },
    toggleAll() {
      const current = allCheckbox()
      if (current !== 'none') setAll(current !== 'on')
    },
    setAll,
    setCursor(index) {
      cursorIndex = index
      cursorKey = rowAt(index)?.key ?? null
    },
    parentIndexOf(index) {
      const row = rowAt(index)
      if (view === null || row === undefined || row.level <= 1) return null
      for (let at = index - 1; at >= view.offset; at--) {
        const above = view.rows[at - view.offset]
        if (above.level === row.level - 1) return at
      }
      // Above the window: the answer's open ancestors, outermost first.
      return view.ancestors.find((ancestor) => ancestor.level === row.level - 1)?.index ?? null
    },
    refresh({ resetSelection = false } = {}) {
      if (opId === null) return
      if (resetSelection) selection = null
      edit()
    },
    dispose() {
      disposed = true
      leaveOperation()
      clearTimeout(settleTimer)
      clearTimeout(spinnerTimer)
      pulls.cancel()
      for (const stop of unlisteners) stop()
    },
  }
}
