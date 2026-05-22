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
  /**
   * Returns whether the OS has `prefers-reduced-motion: reduce`. Injected so tests can
   * exercise both branches deterministically. Defaults to `window.matchMedia` in the
   * default factory below.
   */
  prefersReducedMotion?: () => boolean
}

export interface AutoscrollController {
  /** Idempotently starts the loop. No-op if already running. */
  start(): void
  /** Stops the loop if running. Safe to call from anywhere (pointerup, blur, unmount). */
  stop(): void
  /** Returns whether the loop is currently running. Test-only hook. */
  isRunning(): boolean
}

/**
 * Default `prefers-reduced-motion` probe. Reads `window.matchMedia` once per call so
 * the page picks up live OS changes without restarting the drag.
 */
function defaultPrefersReducedMotion(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

export function createViewerAutoscroll(deps: AutoscrollDeps): AutoscrollController {
  let rafId: number | null = null
  const prefersReducedMotion = deps.prefersReducedMotion ?? defaultPrefersReducedMotion

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

  /**
   * Snap-scroll: under reduced motion, scroll once by an oversized delta (the loop's
   * max per-frame amount, ~30 lines), no RAF, no animation. The user's continued drag
   * past the edge will re-fire `start()` on every subsequent `pointermove`, so they
   * still progress through the file; they just don't see a continuous animation.
   */
  function snapStep(): void {
    const content = deps.getContentRef()
    if (!content) return
    const rect = content.getBoundingClientRect()
    const y = deps.getPointerY()
    const delta = computeAutoscrollPxPerFrame(y, rect.top, rect.bottom)
    if (delta === 0) return
    content.scrollTop += delta
    deps.onScrollStep(y)
  }

  function start(): void {
    if (rafId !== null) return
    if (prefersReducedMotion()) {
      // No RAF loop under reduced motion; do a single snap and stop. The page's
      // pointermove handler calls `start()` again on the next move, so each move
      // produces one discrete scroll step instead of continuous animation.
      snapStep()
      return
    }
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
