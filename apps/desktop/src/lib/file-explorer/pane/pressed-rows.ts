/**
 * Reads the rows a press acts on (the pane's selection, else its cursor row)
 * through a revision-checked backend read, and waits out a busy folder.
 *
 * The pane's row numbers belong to the listing revision it last applied, and a
 * watcher insert reaches the backend first, so the checked read refuses them as
 * `changed` until the pane applies that diff. In a folder that changes every
 * few hundred milliseconds (a download running), refusing outright turned a
 * quarter of presses into a toast. So on `changed` this waits `PRESS_RETRY_MS`
 * and asks again, within one `ROW_SETTLE_BUDGET_MS` budget, and only the budget
 * running out tells the user.
 *
 * ❗ Each attempt re-reads the press FROM THE PANE: its selection, cursor, and
 * applied sequence, together and synchronously. The pane carries its selection
 * through every diff it applies, so those rows name the same files in the newer
 * revision. ❌ Never resend the first attempt's row numbers: against the newer
 * rows they name the neighbor. A press whose shape changed meanwhile (a
 * different listing, a selection that appeared or vanished) can't be re-derived
 * safely, so it's refused.
 */
import { asListingLookupError } from '#lib/tauri-commands/index.js'
import {
  toBackendCursorIndex,
  toBackendSelectionOrCursor,
} from '#lib/file-operations/transfer/transfer-dialog-utils.js'
import { announceRowsStillSettling } from './operation-start-gate'

/**
 * How long a press waits for the pane's rows to settle before saying why nothing
 * started. Rows settle in well under a second after a reload, resort, or diff; the
 * budget covers a loaded machine and a big folder's resort, and stays short enough
 * that a press never feels lost. While an operation runs from the pane its rows stay
 * unsettled until it ends (F5's `whenRowStateReady` wait), which reaches the toast.
 */
export const ROW_SETTLE_BUDGET_MS = 3000
/** How long a press waits between a `changed` refusal and its next attempt. */
export const PRESS_RETRY_MS = 100

/** What a press reads its rows from, live. `FilePaneAPI` is one. */
export interface PressedPane {
  getListingId(): string
  getSelectedIndices(): number[]
  getCursorIndex(): number
  hasParentEntry(): boolean
  getLastSequence(): number
}

/** One attempt's rows: backend row numbers in the pane's applied revision. */
export interface PressedRows {
  listingId: string
  backendRows: number[]
  expectedSequence: number
}

export interface PressOptions {
  /** With nothing selected, act on the cursor row; otherwise a press needs a selection. */
  cursorFallback: boolean
  /** When the press gives up; defaults to `ROW_SETTLE_BUDGET_MS` from now. */
  deadline?: number
  /** The MCP round-trip the refusal fails. */
  mcpRequestId?: string
  /** `false` when the caller words its own refusal. */
  announce?: boolean
}

/** A cursor-row read's options: a press's, minus the fallback (the cursor IS the target). */
export type CursorReadOptions = Omit<PressOptions, 'cursorFallback'>

/**
 * - `read`: the read's answer, and whether the press was on a selection.
 * - `none`: nothing to act on (no listing, no selection without a cursor fallback,
 *   the cursor on `..`). Silent.
 * - `refused`: the rows didn't settle in the budget, or the press changed shape.
 *   Announced unless `announce: false`.
 */
export type PressedRead<T> = { kind: 'read'; value: T; onSelection: boolean } | { kind: 'none' } | { kind: 'refused' }

export async function readPressedRows<T>(
  pane: PressedPane,
  read: (rows: PressedRows) => Promise<T>,
  options: PressOptions,
): Promise<PressedRead<T>> {
  const listingId = pane.getListingId()
  const onSelection = pane.getSelectedIndices().length > 0
  if (!listingId || (!onSelection && !options.cursorFallback)) return { kind: 'none' }
  const rowsNow = (): number[] | null => {
    const selected = pane.getSelectedIndices()
    if (selected.length > 0 !== onSelection) return null
    return toBackendSelectionOrCursor(selected, pane.getCursorIndex(), pane.hasParentEntry())
  }
  return readSettledRows(pane, listingId, rowsNow, read, options, onSelection)
}

/** What a cursor-row read needs from the pane, live: a press's pane minus the selection. */
export type CursorPane = Omit<PressedPane, 'getSelectedIndices'>

/**
 * Reads the CURSOR row alone, whatever is selected, through the same checked,
 * busy-folder-tolerant loop: for the commands that act on the row under the
 * cursor (F3, F4, Quick Look, Get Info, Show in Finder, copy name, open terminal
 * here, the cloud pair). Each attempt re-reads the cursor from the pane, which
 * carries it through every diff it applies, so the answer is the file the user
 * saw under it, ❌ never the neighbor a watcher insert pushed onto its old row
 * number. `none` on the `..` row.
 */
export async function readCursorRow<T>(
  pane: CursorPane,
  read: (rows: PressedRows) => Promise<T>,
  options: CursorReadOptions = {},
): Promise<PressedRead<T>> {
  const listingId = pane.getListingId()
  if (!listingId) return { kind: 'none' }
  const rowsNow = (): number[] => {
    const cursor = toBackendCursorIndex(pane.getCursorIndex(), pane.hasParentEntry())
    return cursor === null ? [] : [cursor]
  }
  return readSettledRows(pane, listingId, rowsNow, read, options, false)
}

/**
 * The retry loop both reads share. `rowsNow` re-derives the rows from the pane
 * for each attempt, or answers `null` when the press changed shape meanwhile.
 */
async function readSettledRows<T>(
  pane: CursorPane,
  listingId: string,
  rowsNow: () => number[] | null,
  read: (rows: PressedRows) => Promise<T>,
  options: CursorReadOptions,
  onSelection: boolean,
): Promise<PressedRead<T>> {
  const deadline = options.deadline ?? Date.now() + ROW_SETTLE_BUDGET_MS
  const refuse = (): PressedRead<T> => {
    if (options.announce !== false) announceRowsStillSettling(options.mcpRequestId)
    return { kind: 'refused' }
  }

  for (;;) {
    if (pane.getListingId() !== listingId) return refuse()
    const backendRows = rowsNow()
    if (backendRows === null) return refuse()
    if (backendRows.length === 0) return { kind: 'none' }
    const expectedSequence = pane.getLastSequence()
    try {
      return { kind: 'read', value: await read({ listingId, backendRows, expectedSequence }), onSelection }
    } catch (error) {
      const refusal = asListingLookupError(error)
      if (!refusal) throw error
      // A gone listing is a pane that re-lists; nothing to wait for.
      if (refusal.type === 'gone' || Date.now() + PRESS_RETRY_MS > deadline) return refuse()
    }
    await new Promise((resolve) => setTimeout(resolve, PRESS_RETRY_MS))
  }
}
