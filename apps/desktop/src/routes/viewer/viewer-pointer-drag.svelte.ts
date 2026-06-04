/**
 * Pointer / drag / context-menu controller for the viewer.
 *
 * Owns the stateful side of text selection by pointer: the active drag's
 * `pointerId` + last `clientY`, the in-app context-menu position, and the
 * drag-autoscroll RAF loop. The pure caret-from-point math lives in
 * `viewer-pointer.ts`; the autoscroll speed curve and RAF driver live in
 * `viewer-autoscroll.ts` / `viewer-autoscroll.svelte.ts`. This controller wires
 * those together against the page's selection model and scroll composable.
 *
 * The page provides getters/callbacks for the scroll container ref, the line
 * cache (for double / triple-click word/line selection), and the selection
 * model's mutators. It binds the returned handlers to the `.file-content`
 * element and the `<svelte:window on:blur>` safety net.
 */

import { caretFromPoint } from './viewer-pointer'
import { computeAutoscrollPxPerFrame } from './viewer-autoscroll'
import { createViewerAutoscroll } from './viewer-autoscroll.svelte'
import { findWordBoundsAt } from './viewer-word'
import type { LineOffset } from './selection.svelte'

interface PointerDragDeps {
  /** Returns the scrollable `.file-content` element, or `undefined` before mount. */
  getContentRef: () => HTMLElement | undefined
  /** Reads the cached text of a line (for double / triple-click), or `undefined` if not cached. */
  getLineText: (line: number) => string | undefined
  /** Whether a selection currently exists (for shift-click extend vs. fresh anchor). */
  hasSelection: () => boolean
  /** Sets the selection anchor (start a fresh selection). */
  setAnchor: (offset: LineOffset) => void
  /** Moves the selection focus (extend the active selection). */
  setFocus: (offset: LineOffset) => void
}

export function createViewerPointerDrag(deps: PointerDragDeps) {
  /**
   * Whether a pointer drag is currently in progress. Tracks `pointerId` so we only
   * react to moves from the same pointer that started the gesture (multi-touch is a
   * future concern; today the viewer is a mouse-only surface but the type is
   * correct).
   */
  let dragPointerId: number | null = null

  /** The pointer's most-recent Y position, used by the autoscroll RAF loop. */
  let dragPointerY = 0

  /** Position of the in-app context menu while it's open, or `null`. */
  let contextMenuPos = $state<{ x: number; y: number } | null>(null)

  /**
   * Re-resolves the caret after each autoscroll step. Uses the X position one px
   * past the left edge of `.file-content` so the caret lands inside the line text
   * (not the line-number gutter, which sits flush to the left edge).
   */
  function reAimAfterAutoscroll(pointerY: number): void {
    const content = deps.getContentRef()
    if (!content) return
    const rect = content.getBoundingClientRect()
    const caret = caretFromPoint(document, rect.left + 1, pointerY)
    if (caret !== null) deps.setFocus(caret)
  }

  const autoscroll = createViewerAutoscroll({
    getContentRef: deps.getContentRef,
    getPointerY: () => dragPointerY,
    onScrollStep: reAimAfterAutoscroll,
  })

  function handlePointerDown(e: PointerEvent): void {
    // Left mouse button only (button 0). Right-click goes to the context menu.
    if (e.button !== 0) return
    const caret = caretFromPoint(document, e.clientX, e.clientY)
    if (caret === null) return
    e.preventDefault()

    // Shift-click extends the existing selection from its anchor to the clicked
    // position. If there's no current selection, treat shift-click as a plain click.
    if (e.shiftKey && deps.hasSelection()) {
      deps.setFocus(caret)
    } else {
      deps.setAnchor(caret)
    }

    dragPointerId = e.pointerId
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
    dragPointerY = e.clientY
    const caret = caretFromPoint(document, e.clientX, e.clientY)
    if (caret !== null) deps.setFocus(caret)

    // Check whether the pointer is near a viewport edge; start/stop autoscroll as needed.
    const content = deps.getContentRef()
    if (!content) return
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

  /**
   * Selects the word under the pointer on double-click, or the whole line on
   * triple-click. The browser delivers consecutive clicks with `detail = 2` and
   * `detail = 3`; we read the click count from there.
   */
  function handleClick(e: MouseEvent): void {
    if (e.detail !== 2 && e.detail !== 3) return
    const caret = caretFromPoint(document, e.clientX, e.clientY)
    if (caret === null) return
    const lineText = deps.getLineText(caret.line) ?? ''

    if (e.detail === 2) {
      const { start, end } = findWordBoundsAt(lineText, caret.offset)
      deps.setAnchor({ line: caret.line, offset: start })
      deps.setFocus({ line: caret.line, offset: end })
      return
    }

    // Triple-click: select the whole line.
    deps.setAnchor({ line: caret.line, offset: 0 })
    deps.setFocus({ line: caret.line, offset: lineText.length })
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
    handleClick,
    closeContextMenu,
    handleWindowBlur,
  }
}
