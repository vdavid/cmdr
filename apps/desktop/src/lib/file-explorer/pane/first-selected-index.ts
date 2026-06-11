/**
 * Pure helper for the post-select cursor jump in `FilePane.applyIndices`.
 *
 * After the Selection dialog commits an ADD, we move the pane's cursor to the first
 * newly-selected file and scroll it into view. The matched `idxs` are in the SAME
 * index space as the pane's selection set and cursor (snapshot indices, with the synthetic
 * `..` at index 0 when `hasParent`). `selection.applyIndices` skips index 0 under
 * `hasParent` (it never selects `..`); the cursor must land on the same first row it
 * actually selected, so we apply the identical skip here. Without it, an `idxs` that still
 * carried a leading `0` would jump the cursor onto the `..` row.
 *
 * `idxs` is already in sort order (the dialog's snapshot is sorted), so the first surviving
 * entry is the lowest selected file index. Returns `null` when nothing selectable remains.
 */
export function firstSelectedIndex(idxs: number[], hasParent: boolean): number | null {
  for (const i of idxs) {
    if (hasParent && i === 0) continue
    return i
  }
  return null
}
