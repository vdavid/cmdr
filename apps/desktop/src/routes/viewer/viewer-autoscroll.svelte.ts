/**
 * Drag-autoscroll loop driver for the viewer.
 *
 * Owns the RAF id and the running flag. The page wires the rest: it tells the loop
 * the current pointer Y on each move, picks the scroll target (`contentRef`), and the
 * loop tells the page when to re-resolve the caret after each scroll step. Pure
 * functions go in `viewer-autoscroll.ts`; the side-effecting bits (RAF, scrollTop
 * mutation) live here.
 */

import { computeAutoscrollPxPerSecond } from './viewer-autoscroll'

interface AutoscrollDeps {
  /** Returns the scrollable element to mutate. Re-read every frame so an unmount
   *  during the loop bails cleanly. */
  getContentRef: () => HTMLElement | undefined
  /** Returns the pointer's most-recent clientY. Re-read every frame. */
  getPointerY: () => number
  /** Called after each scroll step so the page can update the selection focus. */
  onScrollStep: (pointerY: number) => void
  /**
   * Scroll px per content px: below 1 when a huge file's spacer is squeezed under
   * WebKit's element-height cap (`viewer-scroll.svelte.ts`). Defaults to 1. Without it
   * a squeezed file would autoscroll faster than its text moves past.
   */
  getScrollScale?: () => number
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

/**
 * The longest frame the loop credits, so a stalled frame (a GC pause, a window that lost
 * the display) resumes at speed instead of leaping by the whole gap.
 */
const MAX_FRAME_MS = 100

/** WebKit's autoscroll timer period; one reduced-motion step covers one of its ticks. */
const WEBKIT_AUTOSCROLL_TICK_MS = 50

export function createViewerAutoscroll(deps: AutoscrollDeps): AutoscrollController {
  let rafId: number | null = null
  /** The previous frame's RAF timestamp; `null` until the loop's first frame sets the clock. */
  let lastFrameMs: number | null = null
  /** Scroll px owed but not yet applied: a slow crawl moves less than 1 px a frame. */
  let carryPx = 0
  const prefersReducedMotion = deps.prefersReducedMotion ?? defaultPrefersReducedMotion
  const getScrollScale = deps.getScrollScale ?? (() => 1)

  /** Content px/s for the pointer right now, or 0 when it's inside the viewport. */
  function speedNow(content: HTMLElement, y: number): number {
    const rect = content.getBoundingClientRect()
    return computeAutoscrollPxPerSecond(y, rect.top, rect.bottom)
  }

  function tick(nowMs: number): void {
    const content = deps.getContentRef()
    const y = deps.getPointerY()
    const speed = content ? speedNow(content, y) : 0
    if (!content || speed === 0) {
      rafId = null
      return
    }
    // Speed is per second, so the distance follows the clock, not the display's refresh rate.
    const elapsedMs = lastFrameMs === null ? 0 : Math.min(MAX_FRAME_MS, nowMs - lastFrameMs)
    lastFrameMs = nowMs
    carryPx += ((speed * elapsedMs) / 1000) * getScrollScale()
    const wholePx = Math.trunc(carryPx)
    if (wholePx !== 0) {
      carryPx -= wholePx
      content.scrollTop += wholePx
      deps.onScrollStep(y)
    }
    rafId = requestAnimationFrame(tick)
  }

  /**
   * Under reduced motion, scroll once per call by what one WebKit autoscroll tick would
   * (the distance past the edge), no RAF, no animation. The user's continued drag past
   * the edge re-fires `start()` on every `pointermove`, so they still progress through
   * the file; they just don't see a continuous animation.
   */
  function snapStep(): void {
    const content = deps.getContentRef()
    if (!content) return
    const y = deps.getPointerY()
    const delta = Math.round(((speedNow(content, y) * WEBKIT_AUTOSCROLL_TICK_MS) / 1000) * getScrollScale())
    if (delta === 0) return
    content.scrollTop += delta
    deps.onScrollStep(y)
  }

  function start(): void {
    if (rafId !== null) return
    lastFrameMs = null
    carryPx = 0
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
