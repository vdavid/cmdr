/**
 * Pure keyboard math for a list's single cursor (`ColumnList`): what a cursor key asks for,
 * and which row it lands on. ↑/↓ step, PageUp/PageDown page by one row less than a screen
 * (the file panes' step, so a row of context stays), Home/End go to the ends, and a row the
 * cursor can't take (a group heading) is stepped over. No DOM and no state; `ColumnList`'s
 * `cursorTarget` feeds it the list's count, headings, and visible rows.
 */

/** What a cursor key asks of a list. */
export type ListCursorMove =
  | { kind: 'step'; delta: -1 | 1 }
  | { kind: 'page'; delta: -1 | 1 }
  | { kind: 'edge'; edge: 'first' | 'last' }

export interface ListCursorKeyOptions {
  /**
   * The keys come from a text field that drives the list (a combobox): Home and End stay the
   * field's caret keys, as the editable-combobox pattern has them.
   */
  textField?: boolean
}

/**
 * The cursor move a keypress asks for, or `null` for any other key.
 *
 * ❗ A CLASS-OF-KEY matcher, like `menu-navigation.ts`'s: the six bare cursor keys, with no
 * modifier held. ⇧↓, ⌘↓, ⌥↓, and ⌃↓ mean other things (a reorder, an open, a jump) and pass
 * through, so `cmdr/no-raw-key-match`'s target, a single combo with a modifier left
 * unconstrained, can't arise here.
 */
export function listCursorMoveOf(
  event: Pick<KeyboardEvent, 'key' | 'altKey' | 'metaKey' | 'ctrlKey' | 'shiftKey' | 'isComposing'>,
  options: ListCursorKeyOptions = {},
): ListCursorMove | null {
  if (event.isComposing || event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return null
  switch (event.key) {
    case 'ArrowDown':
      return { kind: 'step', delta: 1 }
    case 'ArrowUp':
      return { kind: 'step', delta: -1 }
    case 'PageDown':
      return { kind: 'page', delta: 1 }
    case 'PageUp':
      return { kind: 'page', delta: -1 }
    case 'Home':
      return options.textField ? null : { kind: 'edge', edge: 'first' }
    case 'End':
      return options.textField ? null : { kind: 'edge', edge: 'last' }
    default:
      return null
  }
}

export interface ListCursorTarget {
  count: number
  /** The cursor row, or -1 for none. One past the end (a list that just shrank) reads as the last row. */
  from: number
  /** How many rows fit on screen. A page moves one fewer, and at least one. */
  visibleRows: number
  /** A row the cursor can't take, like a group heading. */
  isSkipped?: (index: number) => boolean
  /** ↓ on the last row goes to the first, and ↑ on the first to the last (Search). Pages and ends never wrap. */
  wrap?: boolean
}

/** Where `move` takes the cursor: a row index, or -1 when no row can take it. */
export function nextListCursor(move: ListCursorMove, target: ListCursorTarget): number {
  const { count, isSkipped, wrap = false } = target
  const takes = (index: number): boolean => index >= 0 && index < count && !isSkipped?.(index)
  /** The first row from `start` that takes the cursor, walking by `delta`, or -1. */
  const scan = (start: number, delta: -1 | 1): number => {
    for (let index = start; index >= 0 && index < count; index += delta) {
      if (takes(index)) return index
    }
    return -1
  }
  const firstRow = scan(0, 1)
  if (firstRow < 0) return -1
  const lastRow = scan(count - 1, -1)
  const from = Math.min(target.from, count - 1)
  // With no cursor yet, ↑ and End start at the bottom, everything else at the top.
  if (from < 0) return startsFromBottom(move) ? lastRow : firstRow
  switch (move.kind) {
    case 'edge':
      return move.edge === 'first' ? firstRow : lastRow
    case 'step': {
      const next = scan(from + move.delta, move.delta)
      if (next >= 0) return next
      if (wrap) return move.delta > 0 ? firstRow : lastRow
      return takes(from) ? from : scan(from, -move.delta as -1 | 1)
    }
    case 'page': {
      const step = Math.max(1, target.visibleRows - 1)
      const landing = Math.max(0, Math.min(count - 1, from + move.delta * step))
      const onward = scan(landing, move.delta)
      return onward >= 0 ? onward : scan(landing, -move.delta as -1 | 1)
    }
  }
}

function startsFromBottom(move: ListCursorMove): boolean {
  return move.kind === 'edge' ? move.edge === 'last' : move.kind === 'step' && move.delta < 0
}
