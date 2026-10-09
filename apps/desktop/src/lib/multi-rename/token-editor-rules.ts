/**
 * When `MaskInput`'s token editor opens and closes on its own: the caret sitting inside a token, the
 * pointer resting on one, or the user working in the editor. The timing (open delays) is the
 * component's; this is which token's editor is open, and why. Pure.
 */
import type { PlaceholderSpan } from './mask-tokens'

/**
 * Why the editor is open, which decides what closes it.
 *
 * - `caret`: the caret is inside the token; focus stays in the field, and the caret leaving closes it.
 * - `hover`: the pointer rests on the token or the editor; leaving both closes it.
 * - `focus`: the user is in the editor (ArrowDown, the marker, a click into it); only leaving it does.
 */
export type OpenReason = 'caret' | 'hover' | 'focus'

/** The open editor: its token, by start, and why it's open. */
export interface EditorOpen {
  from: number
  reason: OpenReason
}

export type EditorEvent =
  /** The field's caret moved: the token it's inside of, or `null`. Only while the field has focus. */
  | { type: 'caret'; from: number | null }
  /** The pointer settled on a token, or left the tokens and the editor (`null`). */
  | { type: 'hover'; from: number | null }
  /** ArrowDown or a marker click: open this token's editor and go into it. */
  | { type: 'request'; from: number }
  /** Focus went into the open editor. */
  | { type: 'engage' }
  /** The mask changed: the starts of the tokens it holds now. */
  | { type: 'tokens'; froms: readonly number[] }
  /** Escape, Enter, a click elsewhere. */
  | { type: 'dismiss' }

/** The token the caret is strictly inside of: on a bracket's outer side, it's just next to it. */
export function tokenInside<T extends { span: PlaceholderSpan }>(tokens: readonly T[], caret: number): T | undefined {
  return tokens.find((token) => caret > token.span.from && caret < token.span.to)
}

/** The caret only moves while the field has focus, so an editor the user was in follows the caret again, as does one the pointer opened once the caret lands in a token. */
function onCaret(open: EditorOpen | null, from: number | null): EditorOpen | null {
  if (from !== null) return { from, reason: 'caret' }
  return open?.reason === 'hover' ? open : null
}

/** The pointer only opens or moves an editor nothing else holds. */
function onHover(open: EditorOpen | null, from: number | null): EditorOpen | null {
  if (open !== null && open.reason !== 'hover') return open
  return from === null ? null : { from, reason: 'hover' }
}

export function nextEditor(open: EditorOpen | null, event: EditorEvent): EditorOpen | null {
  switch (event.type) {
    case 'caret':
      return onCaret(open, event.from)
    case 'hover':
      return onHover(open, event.from)
    case 'request':
      return { from: event.from, reason: 'focus' }
    case 'engage':
      return open === null ? null : { ...open, reason: 'focus' }
    case 'tokens':
      return open !== null && event.froms.includes(open.from) ? open : null
    case 'dismiss':
      return null
  }
}
