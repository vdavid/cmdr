/**
 * Pure helper: classifies a `keydown` in a file pane against the five selection
 * commands (`Space`, `Insert`, `⌘A`, `⌘⇧A`, `⇧8` by default).
 *
 * Resolved through the command registry rather than hand-rolled key predicates, so
 * the keys stay customizable AND the match is exact: `⌥⌘A` (Ask Cmdr) is not `⌘A`,
 * and `⇧Space` (Quick Look) is not `Space`. FilePane used to test
 * `e.key === 'a' && e.metaKey`, a modifier superset, which made `⌥⌘A` select every
 * file on its way to opening the Ask Cmdr rail.
 *
 * Pinned by `selection-keys.test.ts` so the contract holds without spinning up
 * `FilePane`. Sibling of `selection-dialog-keys.ts` (the `+` / `-` classifier).
 */

import { eventMatchesCommand } from '$lib/shortcuts'

export type SelectionKeyCommand =
  | 'selection.toggle'
  | 'selection.toggleAndDown'
  | 'selection.selectAll'
  | 'selection.deselectAll'
  | 'selection.invert'

// Order matters only for readability: the five commands can't share a combo (that
// would be a registry conflict the Settings editor warns about).
const selectionCommands = [
  'selection.toggle',
  'selection.toggleAndDown',
  'selection.selectAll',
  'selection.deselectAll',
  'selection.invert',
] as const satisfies readonly SelectionKeyCommand[]

/** The selection command this keypress triggers, or `null` to let it fall through. */
export function classifySelectionKey(event: KeyboardEvent): SelectionKeyCommand | null {
  return selectionCommands.find((commandId) => eventMatchesCommand(event, commandId)) ?? null
}
