/**
 * What drag and drop reports to analytics, as pure vocabulary.
 *
 * The question these answer isn't "does drag work?" but "how much of Cmdr's file
 * moving happens by DRAG rather than by keyboard or menu?" — which
 * `file_transfer_completed` can't say, because by the time an operation settles,
 * nothing remembers how it was started.
 *
 * PII-free by construction: nothing here can see a path or a file name
 * (`apps/desktop/src-tauri/src/analytics/CLAUDE.md`).
 */

import { itemCountBucket, trackEvent } from '$lib/tauri-commands'

/** Where the dragged items came from. */
export type DropOrigin =
  /** A Cmdr pane: the drag started and ended inside the app. */
  | 'self'
  /** Another application, or Finder. */
  | 'external'

/**
 * How a drop ended.
 *
 * The three refusals are the denominator, and they're why this is one event
 * rather than a success counter: a drop that lands nowhere feels identical to
 * the user, so a `transfer`-only metric would report a smoothly working feature
 * while people keep missing the target.
 */
export type DropOutcome =
  /** The transfer opened; from here `file_transfer_completed` takes over. */
  | 'transfer'
  /** Released outside both panes. */
  | 'noTarget'
  /** Dropped back on the pane it came from, with nothing to do. */
  | 'samePane'
  /** Blocked: the target is the source itself, or inside it. */
  | 'selfDescendant'

/** Reports one drop into a Cmdr pane, whatever came of it. */
export function reportDropReceived(
  origin: DropOrigin,
  outcome: DropOutcome,
  operation: 'move' | 'copy',
  itemCount: number,
): void {
  void trackEvent('drop_received', {
    origin,
    outcome,
    op: operation,
    item_count: itemCountBucket(itemCount),
  })
}

/**
 * Reports one drag OUT of Cmdr (into Finder, a browser upload widget, a
 * terminal) once its session drains. Counted per SESSION, matching the toast:
 * one gesture is one drag however many files it carried.
 */
export function reportDragOutCompleted(itemCount: number, failedCount: number): void {
  void trackEvent('drag_out_completed', {
    item_count: itemCountBucket(itemCount),
    outcome: failedCount === 0 ? 'done' : 'partial',
  })
}
