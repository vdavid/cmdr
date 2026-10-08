/**
 * The pane's side of per-history-entry cursor memory
 * (`../navigation/history-cursor.ts`): it reports where the cursor sits, and it
 * restores the cursor a Back / Forward destination remembers.
 *
 * Reporting: every cursor move reports its index SYNCHRONOUSLY, tagged with the
 * listing on screen, and the coordinator writes it into the current history entry
 * only when that entry IS this listing. So the stretch between a history walk and
 * its landing, when the entry has already moved on and the rows haven't, writes
 * nothing. The row's path follows from the selection-info feed's read, and only
 * when that read is confirmed for the same listing and index (`reportRow`).
 *
 * Restoring: `restore()` parks the destination's cursor. A listing takes it in
 * `listing-loader.ts` when its load starts (`takeForLoad`) and lands on it before
 * the rows paint; a search-results snapshot has no load, so the effect below
 * applies it once the snapshot's rows are mounted at the parked path.
 */

import { untrack } from 'svelte'
import { restoredCursorIndex, type CursorReading } from '../navigation/history-cursor'
import type { HistoryCursor } from '../navigation/navigation-history'
import type { HistoryCursorTarget } from './types'

export interface HistoryCursorSyncDeps {
  getCursorIndex: () => number
  /** The listing whose rows are settled on screen, or null (loading, no listing, a view with no rows). Reactive. */
  getShownLocation: () => { volumeId: string; path: string } | null
  /** The search-results snapshot's rows, when the pane shows one. Reactive. */
  getSnapshotRows: () => readonly { path: string }[] | undefined
  getCurrentPath: () => string
  /** The pane's full cursor write: scrolls the row into view. */
  setCursorIndex: (index: number) => void
  onReading?: (reading: CursorReading) => void
}

export interface HistoryCursorSync {
  /** Park a history walk's destination cursor until its rows land. */
  restore: (target: HistoryCursorTarget) => void
  /** The parked cursor when it's meant for `path`; clears it either way. */
  takeForLoad: (path: string) => HistoryCursor | undefined
  /** A read confirmed the row at `index`; reported only while the cursor is still there. */
  reportRow: (index: number, rowPath: string) => void
}

export function createHistoryCursorSync(deps: HistoryCursorSyncDeps): HistoryCursorSync {
  let pending = $state.raw<HistoryCursorTarget | null>(null)

  $effect(() => {
    const index = deps.getCursorIndex()
    const location = deps.getShownLocation()
    if (!location) return
    untrack(() => deps.onReading?.({ ...location, index }))
  })

  $effect(() => {
    const target = pending
    if (!target) return
    const rows = deps.getSnapshotRows()
    if (!rows || deps.getCurrentPath() !== target.path) return
    pending = null
    const rowPath = target.cursor?.rowPath
    const found = rowPath === undefined ? -1 : rows.findIndex((row) => row.path === rowPath)
    const index = restoredCursorIndex(target.cursor, found === -1 ? undefined : found, rows.length)
    untrack(() => {
      deps.setCursorIndex(index)
    })
  })

  return {
    restore: (target) => {
      pending = target
    },
    takeForLoad: (path) => {
      const target = untrack(() => pending)
      pending = null
      return target?.path === path ? target.cursor : undefined
    },
    reportRow: (index, rowPath) => {
      const location = deps.getShownLocation()
      if (!location || deps.getCursorIndex() !== index) return
      deps.onReading?.({ ...location, index, rowPath })
    },
  }
}
