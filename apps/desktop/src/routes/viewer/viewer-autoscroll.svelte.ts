/**
 * Drag-autoscroll loop driver for the viewer.
 *
 * Owns the RAF id and the running flag. The page wires the rest: it tells the loop
 * the current pointer Y on each move, picks the scroll target (`contentRef`), and the
 * loop tells the page when to re-resolve the caret after each scroll step. Pure
 * functions go in `viewer-autoscroll.ts`; the side-effecting bits (RAF, scrollTop
 * mutation) live here.
 */

import { computeAutoscrollPxPerFrame } from './viewer-autoscroll'

interface AutoscrollDeps {
  /** Returns the scrollable element to mutate. Re-read every frame so an unmount
   *  during the loop bails cleanly. */
  getContentRef: () => HTMLElement | undefined
  /** Returns the pointer's most-recent clientY. Re-read every frame. */
  getPointerY: () => number
  /** Called after each scroll step so the page can update the selection focus. */
  onScrollStep: (pointerY: number) => void
}

export interface AutoscrollController {
  /** Idempotently starts the loop. No-op if already running. */
  start(): void
  /** Stops the loop if running. Safe to call from anywhere (pointerup, blur, unmount). */
  stop(): void
  /** Returns whether the loop is currently running. Test-only hook. */
  isRunning(): boolean
}

export function createViewerAutoscroll(deps: AutoscrollDeps): AutoscrollController {
  let rafId: number | null = null

  function tick(): void {
    const content = deps.getContentRef()
    if (!content) {
      rafId = null
      return
    }
    const rect = content.getBoundingClientRect()
    const y = deps.getPointerY()
    const delta = computeAutoscrollPxPerFrame(y, rect.top, rect.bottom)
    if (delta === 0) {
      rafId = null
      return
    }
    content.scrollTop += delta
    deps.onScrollStep(y)
    rafId = requestAnimationFrame(tick)
  }

  function start(): void {
    if (rafId !== null) return
    rafId = requestAnimationFrame(tick)
  }

  function stop(): void {
    if (rafId === null) return
    cancelAnimationFrame(rafId)
    rafId = null
  }

  function isRunning(): boolean {
    return rafId !== null
  }

  return { start, stop, isRunning }
}
