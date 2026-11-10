/**
 * Pure keyboard math for a tree's ← and → (a `ColumnList` treegrid), beside `list-cursor.ts`,
 * which keeps ↑ ↓ PageUp PageDown Home End. → opens a shut folder, or steps into an open one;
 * ← shuts an open folder, or steps out to its parent, or, on a top-level row, asks to leave
 * the tree (a master–detail dialog moves focus back to its list). No DOM, no state, and no
 * tree model: the consumer hands in the cursor row's facts and its parent's index, both of
 * which a backend-built window already answers (`ancestors`).
 */

/** What ← or → asks of a tree. */
export type TreeCursorKey = 'expand' | 'collapse'

/** The cursor row, as the tree's ARIA describes it (`ColumnListRowAria` fits). */
export interface TreeCursorRow {
  /** 1-based depth: a top-level row is 1. */
  level: number
  /** `true` open, `false` shut, `undefined` for a row with nothing to open (a file, an empty folder). */
  expanded?: boolean
}

/** What the consumer does for the key. */
export type TreeCursorAction =
  | { kind: 'expand' }
  | { kind: 'collapse' }
  | { kind: 'move'; index: number }
  /** ← on a top-level row: focus goes to whatever sits before the tree. */
  | { kind: 'leave' }

/**
 * The tree key a keypress asks for, or `null` for any other key.
 *
 * ❗ A CLASS-OF-KEY matcher, like `listCursorMoveOf`: bare ← and → only. ⌥← / ⌘← and friends
 * mean other things and pass through.
 */
export function treeCursorKeyOf(
  event: Pick<KeyboardEvent, 'key' | 'altKey' | 'metaKey' | 'ctrlKey' | 'shiftKey' | 'isComposing'>,
): TreeCursorKey | null {
  if (event.isComposing || event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return null
  if (event.key === 'ArrowRight') return 'expand'
  if (event.key === 'ArrowLeft') return 'collapse'
  return null
}

export interface TreeCursorTarget {
  /** The cursor row's index. */
  from: number
  /** The cursor row, or `undefined` while it's still loading (the key then does nothing). */
  row: TreeCursorRow | undefined
  /** The tree's row count. */
  count: number
  /** The cursor row's parent's index, or `null` when the consumer can't place it. Unused on a top-level row. */
  parentIndex: number | null
}

/** What `key` does from the cursor row, or `null` when it does nothing there. */
export function nextTreeCursor(key: TreeCursorKey, target: TreeCursorTarget): TreeCursorAction | null {
  const { from, row, count, parentIndex } = target
  if (row === undefined) return null
  if (key === 'expand') {
    if (row.expanded === false) return { kind: 'expand' }
    // An open folder's first child is the next row: a folder with nothing under it never
    // opens (its `expanded` is `undefined`), so an open one always has one.
    if (row.expanded === true && from + 1 < count) return { kind: 'move', index: from + 1 }
    return null
  }
  if (row.expanded === true) return { kind: 'collapse' }
  if (row.level <= 1) return { kind: 'leave' }
  return parentIndex === null ? null : { kind: 'move', index: parentIndex }
}
