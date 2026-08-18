/**
 * The keypress and the index math behind a chained rename: ArrowDown saves the
 * name being edited and reopens the editor on the row below, ArrowUp on the row
 * above. Both sides are pure, so the flow that performs the step
 * (`pane/rename-flow.svelte.ts`) is the only place with moving parts.
 */

import { eventMatchesCommand } from '$lib/shortcuts'

/** Which way a chained rename step moves through the listing. */
export type RenameStepDirection = 'up' | 'down'

/** What the listing looks like around the editor, in cursor-row indices. */
export interface RenameStepBounds {
  /** The row the editor is open on. */
  cursorIndex: number
  /** Cursor-addressable rows, `..` included when there is one. */
  rowCount: number
  /** Whether row 0 is the synthetic `..` row. */
  hasParent: boolean
}

/**
 * The row a step lands on, or `undefined` when there is none.
 *
 * `undefined` means the key does NOTHING: no commit, no discard, the editor
 * stays open with the edit intact. Running off the end of a directory is the
 * user finding the edge, not a decision about the name they're typing. `..` is
 * not a rename target, so it bounds an upward step the same way the top of the
 * listing does.
 */
export function resolveStepIndex(direction: RenameStepDirection, bounds: RenameStepBounds): number | undefined {
  const next = direction === 'down' ? bounds.cursorIndex + 1 : bounds.cursorIndex - 1
  const firstRenamableRow = bounds.hasParent ? 1 : 0
  if (next < firstRenamableRow || next >= bounds.rowCount) return undefined
  return next
}

/**
 * The direction this keypress asks the chain to move, or `undefined` for
 * anything else.
 *
 * Matched on the WHOLE combo through the file list's own up/down commands, so
 * only a bare arrow chains: `⌘↓` (open), `⌥↑` (go to first), and `⇧↓` (extend
 * the selection) keep their meanings, and Page Up/Down, Home, and End never
 * chain at all. The caret-to-start/end that a bare arrow used to do inside the
 * input is given up for this, and `⌘←` / `⌘→` are what still do it: Home and End
 * reach the input un-prevented, and this webview moves the caret nowhere for
 * them, in any text field in the app (verified on macOS 26.5.2 / WKWebView, real
 * OS key events, 2026-08-18).
 */
export function renameStepDirection(event: KeyboardEvent): RenameStepDirection | undefined {
  if (eventMatchesCommand(event, 'nav.down')) return 'down'
  if (eventMatchesCommand(event, 'nav.up')) return 'up'
  return undefined
}
