/**
 * The viewer's optional text cursor: a thin bar painted at the selection's focus, the
 * point every keyboard extension grows from. Off by default (`viewer.showTextCursor`).
 *
 * Purely a render layer over state that already exists. Nothing here feeds back into the
 * selection, the keyboard, or the scroll composable, which is what lets the whole thing
 * be a setting rather than a mode.
 *
 * "Text cursor" is the rendered bar; "caret" stays the name for a resolved text position
 * (`LineOffset`, `caretFromPoint`, `viewer-caret-geometry.ts`). `DETAILS.md` § "Text
 * cursor" has the boundary.
 */

import { tick } from 'svelte'
import { caretRectFor } from './viewer-pointer'
import type { CaretRect } from './viewer-caret-geometry'
import type { LineOffset } from './selection.svelte'

/** Where to paint the bar, in `.scroll-spacer` coordinates. */
export interface TextCursorBox {
  /** Pixels below the spacer's top edge. */
  top: number
  /** Pixels right of the spacer's left edge. */
  left: number
  /** One visual row tall, taken from the measured character box. */
  height: number
}

/** Just enough of a `DOMRect` to place a box against it. */
export interface RectOrigin {
  top: number
  left: number
}

/**
 * Converts a caret rect into the box the cursor element gets.
 *
 * ❌ This subtraction is the WHOLE conversion — never add `linesOffset` on top of it.
 * `caretRectFor` bottoms out in `Range.getClientRects()`, so it hands back a live
 * VIEWPORT rect read off the rendered row, which already carries `.lines-container`'s
 * `translateY` and the current scroll position. Applying the transform a second time
 * puts the cursor 10⁵-10⁷ px off screen on a large file.
 *
 * `null` for a rect with no height: an unlaid-out row measures zero, and a zero-height
 * bar would be an invisible cursor rather than an honest absence.
 */
export function toSpacerRelative(caret: CaretRect, spacer: RectOrigin): TextCursorBox | null {
  const height = caret.bottom - caret.top
  if (height <= 0) return null
  return { top: caret.top - spacer.top, left: caret.left - spacer.left, height }
}

interface TextCursorDeps {
  /** The `viewer.showTextCursor` setting, read reactively so a flip in Settings lands. */
  isEnabled: () => boolean
  /** The selection's moving end, or `null` when there's no selection. */
  getFocus: () => LineOffset | null
  getContentRef: () => HTMLElement | undefined
  getSpacerRef: () => HTMLElement | undefined
  /**
   * Anything that can move a rendered row while the focus stays put: the scroll
   * position, the rendered line set, the wrap flag, the text scale. Read for its
   * reactive dependencies only; the value is never inspected.
   */
  getLayoutKey: () => unknown
}

export function createViewerTextCursor(deps: TextCursorDeps) {
  let box = $state<TextCursorBox | null>(null)

  /** Re-keys the blink so a keypress always leaves the cursor solid (design principle 3:
   *  a keypress landing in the blink's "off" half would look like the cursor vanished).
   *  Keyed on the focus alone, so scrolling past a parked cursor doesn't reset it. */
  const blinkKey = $derived.by(() => {
    const focus = deps.getFocus()
    return focus === null ? '' : `${String(focus.line)}:${String(focus.offset)}`
  })

  /** Discards a measurement whose effect run has already been superseded. */
  let generation = 0

  /**
   * Re-measures the cursor box. The page wires this as an `$effect`; every dependency is
   * read synchronously up front so the effect tracks them, and the measurement itself
   * runs after `tick()`, once the DOM reflects the state that triggered the run.
   */
  function runMeasureEffect(): void {
    const enabled = deps.isEnabled()
    const focus = deps.getFocus()
    const content = deps.getContentRef()
    const spacer = deps.getSpacerRef()
    deps.getLayoutKey()

    const token = ++generation
    if (!enabled || focus === null || content === undefined || spacer === undefined) {
      box = null
      return
    }

    void tick().then(() => {
      if (token !== generation) return
      const caret = caretRectFor(content, focus)
      // `null` when the focus line isn't rendered (scrolled out, or the end-of-file
      // sentinel, which has no row of its own) or nothing could be measured.
      box = caret === null ? null : toSpacerRelative(caret, spacer.getBoundingClientRect())
    })
  }

  return {
    get box() {
      return box
    },
    get blinkKey() {
      return blinkKey
    },
    runMeasureEffect,
  }
}
