/**
 * Pure helper: classifies a `keydown` event for the Selection dialog's
 * `+` / `-` shortcuts inside a file pane.
 *
 * Total Commander parity: `+` opens "Select files…", `-` opens "Deselect
 * files…". Resolved through the command registry like its sibling
 * `selection-keys.ts`, so both keys follow a rebind, and layout independence
 * comes from the shared vocabulary: `formatKeyCombo` names a typed symbol by its
 * character, so US ⇧=, Hungarian ⇧3, and the numpad all mean `+`.
 *
 * Tested separately in `selection-dialog-keys.test.ts` so the contract is
 * pinned without spinning up `FilePane`.
 */

import { eventMatchesCommand } from '$lib/shortcuts'

export type SelectionDialogAction = 'open-add' | 'open-remove' | null

export function classifySelectionDialogKey(e: KeyboardEvent): SelectionDialogAction {
  if (eventMatchesCommand(e, 'selection.selectFiles')) return 'open-add'
  if (eventMatchesCommand(e, 'selection.deselectFiles')) return 'open-remove'
  return null
}
