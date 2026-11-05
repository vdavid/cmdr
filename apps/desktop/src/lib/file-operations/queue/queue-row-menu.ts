/**
 * A queue row's right-click menu: what it lists, and what a pick does. The menu is a
 * native one (`crates/cmdr-menu/src/context/popups.rs`'s `queue_row_menu` lays it out from these entries),
 * and every pick runs the keyboard action the same item's key runs, so the two can't
 * drift (`routes/queue/+page.svelte`). DETAILS § "The row's right-click menu".
 */

import type { OperationSnapshot, QueueRowMenuAction, QueueRowMenuEntry } from '#lib/tauri-commands/index.js'
import type { QueueKeyAction } from './queue-keys'

/** Everything the menu asks about one row, read when it opens and again on the pick. */
export interface QueueRowMenuFacts {
  /** Running or paused: the row gets Show, Pause or Resume, and Rollback too. */
  active: boolean
  paused: boolean
  /** The row offers its Show button (`QueueRow`'s `canForeground`). */
  canShow: boolean
  /** The row offers its Rollback button. */
  canRollback: boolean
  /** The row offers its Cancel button: it's running, paused, or waiting. */
  canCancel: boolean
  /** The row offers its Dismiss button: a retained failure. Dismiss takes Cancel's place. */
  canDismiss: boolean
  /** The row has a checkbox for "Cancel selected" (a settled failure has none). */
  selectable: boolean
  selected: boolean
  /** A waiting row with another waiting row before / after it (`queuedNeighbors`). */
  canMoveEarlier: boolean
  canMoveLater: boolean
}

/** The facts the ROW answers for itself, next to the buttons they decide
 *  (`QueueRow`'s `menuFacts`); the page adds the selection and the line's order. */
export type QueueRowOwnMenuFacts = Pick<
  QueueRowMenuFacts,
  'active' | 'paused' | 'canShow' | 'canRollback' | 'canCancel' | 'canDismiss'
>

/** The row's export the menu reads, beside its keyboard half (`QueueRowApi`). */
export interface QueueRowMenuSource {
  menuFacts: () => QueueRowOwnMenuFacts
}

function item(action: QueueRowMenuAction, enabled: boolean): QueueRowMenuEntry {
  return { kind: 'item', action, enabled }
}

/** Select is ticked on a selected row, as its checkbox is; picking it again unticks it. */
function selectItem(facts: QueueRowMenuFacts): QueueRowMenuEntry {
  return { kind: 'check', action: 'select', enabled: facts.selectable, checked: facts.selected }
}

const separator: QueueRowMenuEntry = { kind: 'separator' }

/**
 * The menu for one row. Every row lists Move up / Move down, Select, and Cancel, in that
 * order, greying what doesn't apply. A running or paused row leads with what only it
 * offers (Show, Pause or Resume), and puts Rollback beside Cancel: the two ways to stop
 * it, grouped at the bottom where the irreversible end of a menu sits.
 */
export function queueRowMenuEntries(facts: QueueRowMenuFacts): QueueRowMenuEntry[] {
  const middle: QueueRowMenuEntry[] = [
    item('moveEarlier', facts.canMoveEarlier),
    item('moveLater', facts.canMoveLater),
    separator,
    selectItem(facts),
    separator,
    facts.canDismiss ? item('dismiss', true) : item('cancel', facts.canCancel),
  ]
  if (!facts.active) return middle
  return [
    item('show', facts.canShow),
    item(facts.paused ? 'resume' : 'pause', true),
    separator,
    ...middle,
    item('rollback', facts.canRollback),
  ]
}

/**
 * Whether the row still offers `action` live. The pick arrives after the menu closed,
 * and the row may have moved on meanwhile (paused from the toolbar, finished), so a
 * Pause picked off a row that's paused by now must not resume it.
 */
export function queueRowMenuOffers(facts: QueueRowMenuFacts, action: QueueRowMenuAction): boolean {
  return queueRowMenuEntries(facts).some(
    (entry) => entry.kind !== 'separator' && entry.action === action && entry.enabled,
  )
}

/** The window's keyboard action a pick runs: the same one as the item's key. */
export function queueRowMenuKeyAction(action: QueueRowMenuAction): QueueKeyAction {
  switch (action) {
    case 'moveEarlier':
      return { kind: 'move', direction: 'earlier' }
    case 'moveLater':
      return { kind: 'move', direction: 'later' }
    case 'select':
      return { kind: 'toggleSelected' }
    case 'cancel':
      return { kind: 'cancel' }
    case 'dismiss':
      return { kind: 'cancelOrDismiss' }
    case 'show':
      return { kind: 'show' }
    case 'pause':
    case 'resume':
      return { kind: 'pauseOrResume' }
    case 'rollback':
      return { kind: 'rollback' }
  }
}

/**
 * Whether the row `id` can move up or down the line. Only waiting operations trade
 * places, and only with each other: a running or paused one between them keeps its
 * slot (`crates/cmdr-operations/DETAILS.md` § "Reordering the line"). `rows` is in line
 * order, which is the snapshot's order.
 */
export function queuedNeighbors(
  rows: readonly { id: string; status: OperationSnapshot['status'] }[],
  id: string,
): { canMoveEarlier: boolean; canMoveLater: boolean } {
  const waiting = rows.filter((row) => row.status === 'queued').map((row) => row.id)
  const at = waiting.indexOf(id)
  return { canMoveEarlier: at > 0, canMoveLater: at !== -1 && at < waiting.length - 1 }
}
