/**
 * Pure helper: classifies a `keydown` event for the Selection dialog's
 * `+` / `-` shortcuts inside a file pane.
 *
 * Total Commander parity: `+` opens "Select files…", `-` opens "Deselect
 * files…". Both require NO command-style modifier (`metaKey`, `altKey`,
 * `ctrlKey`). We INTENTIONALLY don't test `shiftKey` because on US QWERTY
 * layouts, `Shift+=` is the only way to produce `event.key === '+'`. On layouts
 * where `+` is unshifted, plain `+` fires the same event. Either way, the dialog
 * opens.
 *
 * Deselect also fires on the physical Minus key regardless of Shift
 * (`event.code === 'Minus'`), so `⇧-` (which is `event.key === '_'` on US QWERTY)
 * deselects too — symmetry with the shifted `+`, and layout-independent.
 *
 * Tested separately in `selection-dialog-keys.test.ts` so the contract is
 * pinned without spinning up `FilePane`.
 */

export type SelectionDialogAction = 'open-add' | 'open-remove' | null

export function classifySelectionDialogKey(e: KeyboardEvent): SelectionDialogAction {
  if (e.metaKey || e.altKey || e.ctrlKey) return null
  if (e.key === '+') return 'open-add'
  if (e.key === '-' || e.code === 'Minus') return 'open-remove'
  return null
}
