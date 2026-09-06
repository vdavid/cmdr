/**
 * Pointer / drag / context-menu controller for the viewer.
 *
 * Owns the stateful side of text selection by pointer: the active drag's
 * `pointerId` + last pointer position, the click cycle behind word / line selection, the
 * gesture's selection granularity, the in-app context-menu position, and the
 * drag-autoscroll RAF loop. Point → caret resolution lives in `viewer-pointer.ts` (over
 * `viewer-caret-geometry.ts`), the click cycle in `viewer-multi-click.ts`, the
 * granularity arithmetic in `viewer-selection-granularity.ts`, and the autoscroll speed
 * curve and RAF driver in `viewer-autoscroll.ts` / `viewer-autoscroll.svelte.ts`. This
 * controller wires those together against the page's selection model and scroll
 * composable.
 *
 * Every gesture rides the `pointerdown` stream, `click` included: the controller counts
 * presses itself rather than reading a mouse event's `detail`.
 *
 * The page provides getters/callbacks for the scroll container ref, the line
 * cache (for word / line granularity), and the selection model's mutators. It binds the
 * returned handlers to the `.file-content` element and the `<svelte:window on:blur>`
 * safety net.
 */

import { caretFromPoint, caretFromPointClamped } from './viewer-pointer'
import { computeAutoscrollPxPerFrame } from './viewer-autoscroll'
import { createViewerAutoscroll } from './viewer-autoscroll.svelte'
import { advanceMultiClick, type MultiClickState } from './viewer-multi-click'
import {
  extendRangeToGranularity,
  rangeAtCaret,
  type LineRange,
  type SelectionGranularity,
} from './viewer-selection-granularity'
import type { LineOffset, Selection } from './selection.svelte'

interface PointerDragDeps {
  /** Returns the scrollable `.file-content` element, or `undefined` before mount. */
  getContentRef: () => HTMLElement | undefined
  /** Reads the cached text of a line (for word / line granularity), or `undefined` if not cached. */
  getLineText: (line: number) => string | undefined
  /** Whether a selection currently exists (for shift-click extend vs. fresh anchor). */
  hasSelection: () => boolean
  /** Sets the selection anchor (start a fresh selection). */
  setAnchor: (offset: LineOffset) => void
  /** Moves the selection focus (extend the active selection). */
  setFocus: (offset: LineOffset) => void
  /** Sets both endpoints at once, for word- and line-granularity gestures. */
  setRange: (range: Selection) => void
  /**
   * Gives DOM focus back to the viewer surface (the page focuses its container, the
   * same element it focuses after a session opens). Called when a selection gesture
   * starts, because the handler's `preventDefault()` blocks the native focus move.
   */
  takeFocus: () => void
}

export function createViewerPointerDrag(deps: PointerDragDeps) {
  /**
   * Whether a pointer drag is currently in progress. Tracks `pointerId` so we only
   * react to moves from the same pointer that started the gesture (multi-touch is a
   * future concern; today the viewer is a mouse-only surface but the type is
   * correct).
   */
  let dragPointerId: number | null = null

  /** The pointer's most-recent position, used by the autoscroll RAF loop. */
  let dragPointerX = 0
  let dragPointerY = 0

  /** Position of the in-app context menu while it's open, or `null`. */
  let contextMenuPos = $state<{ x: number; y: number } | null>(null)

  /** The last press and where it sat in the click cycle, or `null` before the first one. */
  let lastPress: MultiClickState | null = null

  /**
   * Granularity of the drag in progress, reset by `endDrag`: it describes this drag and
   * nothing beyond it.
   */
  let dragGranularity: SelectionGranularity = 'character'

  /**
   * Granularity of the current gesture and the range its press covered, reset only by a
   * plain (count 1, non-shift) press. A later shift-click reads both.
   *
   * Gotcha/Why: ❌ don't fold these into `dragGranularity`. `endDrag` fires on the
   * double-press's own `pointerup`, so a shift-click reading the drag's granularity
   * would always see `character` and silently extend by one character. The anchor range
   * has to outlive the drag too, or a backwards shift-click can't anchor at the far edge
   * of the pressed word.
   */
  let gestureGranularity: SelectionGranularity = 'character'
  let gestureAnchorRange: LineRange | null = null

  /**
   * Re-resolves the caret after each autoscroll step. The pointer is past a viewport
   * edge by definition here (that's what started the autoscroll), so the aim is clamped
   * into `.file-content` and the selection sweeps whole rows of the newly-scrolled-in
   * text. Routed through the same extend call as a plain move, so a word drag past the
   * viewport edge keeps its granularity.
   */
  function reAimAfterAutoscroll(pointerY: number): void {
    const content = deps.getContentRef()
    if (!content) return
    const caret = caretFromPointClamped(content, dragPointerX, pointerY)
    if (caret !== null) extendToCaret(caret, dragGranularity)
  }

  /**
   * Starts a gesture at `granularity`, snapping the pressed caret to the anchor range the
   * rest of the gesture (drag, autoscroll, a later shift-click) extends from.
   */
  function startGesture(caret: LineOffset, granularity: SelectionGranularity): void {
    gestureGranularity = granularity
    gestureAnchorRange = rangeAtCaret({ caret, granularity, getLineText: deps.getLineText })
    dragGranularity = granularity
  }

  /**
   * Extends the selection to `caret` at `granularity`. Character granularity moves the
   * focus alone, which is what a plain drag does; word and line re-derive both endpoints
   * from the remembered anchor range, so the selection stays snapped in both directions.
   */
  function extendToCaret(caret: LineOffset, granularity: SelectionGranularity): void {
    if (granularity === 'character' || gestureAnchorRange === null) {
      deps.setFocus(caret)
      return
    }
    deps.setRange(
      extendRangeToGranularity({
        anchorRange: gestureAnchorRange,
        focus: caret,
        granularity,
        getLineText: deps.getLineText,
      }),
    )
  }

  const autoscroll = createViewerAutoscroll({
    getContentRef: deps.getContentRef,
    getPointerY: () => dragPointerY,
    onScrollStep: reAimAfterAutoscroll,
  })

  function handlePointerDown(e: PointerEvent): void {
    // Left mouse button only (button 0). Right-click goes to the context menu.
    if (e.button !== 0) return

    // Claim DOM focus for the viewer before anything else. The `preventDefault()`
    // below suppresses the native focus move, so without this the search input keeps
    // focus through the whole drag and ⌘C lands on its text rather than the selection
    // the user just made. Focus follows the surface the pointer works on, exactly as a
    // click in the document takes focus off any editor's find field.
    deps.takeFocus()

    const content = deps.getContentRef()
    if (!content) return
    const caret = caretFromPoint(content, e.clientX, e.clientY)
    if (caret === null) return
    e.preventDefault()

    const press = advanceMultiClick(lastPress, { x: e.clientX, y: e.clientY, time: e.timeStamp })
    lastPress = press

    // Shift-click extends the existing selection to the clicked position, at whatever
    // granularity the gesture is running (native behavior: shift-clicking after a
    // double-click extends by whole words). If there's no current selection, treat
    // shift-click as a plain click. It's an extend gesture, never part of a word/line
    // cycle, so the count restarts.
    if (e.shiftKey && deps.hasSelection()) {
      lastPress = { ...press, count: 1 }
      dragGranularity = gestureGranularity
      extendToCaret(caret, gestureGranularity)
    } else if (press.count === 1) {
      // A plain press ends the previous gesture and starts a fresh caret selection.
      startGesture(caret, 'character')
      deps.setAnchor(caret)
    } else {
      // Second press selects the word, third the whole line, and the drag carries on at
      // that granularity: every move re-derives the selection from the pressed range, so
      // a twitch inside it yields the same union rather than collapsing to a caret.
      startGesture(caret, press.count === 2 ? 'word' : 'line')
      extendToCaret(caret, gestureGranularity)
    }

    dragPointerId = e.pointerId
    dragPointerX = e.clientX
    dragPointerY = e.clientY
    // Capture so we keep receiving pointer events even if the cursor leaves the
    // webview (the user dragged past the edge into another macOS window or the
    // desktop). Without capture, autoscroll would never see a `pointerup` to stop.
    try {
      ;(e.currentTarget as Element | null)?.setPointerCapture(e.pointerId)
    } catch {
      // Capture can throw on some webviews if the target isn't focusable; ignoring
      // is safe (the drag still works, just without the safety net).
    }
  }

  function handlePointerMove(e: PointerEvent): void {
    if (dragPointerId === null || e.pointerId !== dragPointerId) return
    dragPointerX = e.clientX
    dragPointerY = e.clientY

    const content = deps.getContentRef()
    if (!content) return
    // Clamped: a drag that has left the viewport still extends the selection to the
    // nearest edge of the rendered text instead of freezing where it crossed out.
    const caret = caretFromPointClamped(content, e.clientX, e.clientY)
    if (caret !== null) extendToCaret(caret, dragGranularity)

    // Check whether the pointer is near a viewport edge; start/stop autoscroll as needed.
    const rect = content.getBoundingClientRect()
    const delta = computeAutoscrollPxPerFrame(e.clientY, rect.top, rect.bottom)
    if (delta !== 0) {
      autoscroll.start()
    } else {
      autoscroll.stop()
    }
  }

  function endDrag(pointerId: number): void {
    if (dragPointerId !== pointerId) return
    dragPointerId = null
    dragGranularity = 'character'
    autoscroll.stop()
  }

  function handlePointerUp(e: PointerEvent): void {
    endDrag(e.pointerId)
  }

  function handlePointerCancel(e: PointerEvent): void {
    endDrag(e.pointerId)
  }

  function handleContextMenu(e: MouseEvent): void {
    // Suppress the native OS context menu so our in-app one wins.
    e.preventDefault()
    contextMenuPos = { x: e.clientX, y: e.clientY }
  }

  function closeContextMenu(): void {
    contextMenuPos = null
  }

  /**
   * Window `blur` safety net: macOS may hand focus to another app mid-drag without
   * firing a `pointerup` or `pointercancel`. Without this, the autoscroll RAF loop
   * would keep running indefinitely.
   */
  function handleWindowBlur(): void {
    if (dragPointerId !== null) {
      dragPointerId = null
      dragGranularity = 'character'
    }
    autoscroll.stop()
  }

  return {
    get contextMenuPos() {
      return contextMenuPos
    },
    handlePointerDown,
    handlePointerMove,
    handlePointerUp,
    handlePointerCancel,
    handleContextMenu,
    closeContextMenu,
    handleWindowBlur,
  }
}
