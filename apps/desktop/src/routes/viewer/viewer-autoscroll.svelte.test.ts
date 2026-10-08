import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

import { AUTOSCROLL_PX_PER_SEC_PER_PX_PAST } from './viewer-autoscroll'
import { createViewerAutoscroll } from './viewer-autoscroll.svelte'

/**
 * Stub `requestAnimationFrame` / `cancelAnimationFrame` so tests can drive the loop
 * deterministically. Each `start()` queues a tick; the test then calls `runOneFrame()`
 * to fire it, `frameMs` after the previous frame (the RAF timestamp is the loop's clock).
 */
let scheduledTick: ((now: number) => void) | null = null
let nextRafId = 1
let clockMs = 0

function rafStub(cb: FrameRequestCallback): number {
  scheduledTick = cb
  return nextRafId++
}

function cancelStub(): void {
  scheduledTick = null
}

function runOneFrame(frameMs = 1000 / 60): void {
  const fn = scheduledTick
  scheduledTick = null
  clockMs += frameMs
  fn?.(clockMs)
}

let originalRaf: typeof requestAnimationFrame
let originalCaf: typeof cancelAnimationFrame

beforeEach(() => {
  originalRaf = globalThis.requestAnimationFrame
  originalCaf = globalThis.cancelAnimationFrame
  globalThis.requestAnimationFrame = rafStub
  globalThis.cancelAnimationFrame = cancelStub
  scheduledTick = null
  nextRafId = 1
  clockMs = 1000
})

afterEach(() => {
  globalThis.requestAnimationFrame = originalRaf
  globalThis.cancelAnimationFrame = originalCaf
})

function makeContent(rectTop: number, rectBottom: number): { el: HTMLElement; getScrollTop: () => number } {
  const el = document.createElement('div')
  // jsdom doesn't lay anything out, so stub getBoundingClientRect.
  el.getBoundingClientRect = () => ({
    top: rectTop,
    bottom: rectBottom,
    left: 0,
    right: 100,
    width: 100,
    height: rectBottom - rectTop,
    x: 0,
    y: rectTop,
    toJSON: () => ({}),
  })
  return { el, getScrollTop: () => el.scrollTop }
}

/** Runs `frames` frames of `frameMs` each, after the loop's first (timing-only) frame. */
function runFrames(frames: number, frameMs: number): void {
  runOneFrame(frameMs)
  for (let i = 0; i < frames; i++) runOneFrame(frameMs)
}

describe('createViewerAutoscroll', () => {
  it('start() requests a frame; stop() cancels it', () => {
    const { el } = makeContent(0, 400)
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => 200,
      onScrollStep: () => {},
    })

    expect(ctrl.isRunning()).toBe(false)
    ctrl.start()
    expect(ctrl.isRunning()).toBe(true)
    ctrl.stop()
    expect(ctrl.isRunning()).toBe(false)
  })

  it('start() is idempotent: second call is a no-op', () => {
    const { el } = makeContent(0, 400)
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => 200,
      onScrollStep: () => {},
    })
    ctrl.start()
    const firstTick = scheduledTick
    ctrl.start()
    expect(scheduledTick).toBe(firstTick)
  })

  it('loop scrolls and calls onScrollStep when the pointer is past the edge', () => {
    const { el } = makeContent(0, 400)
    const onStep = vi.fn()
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => 420, // 20 px past the bottom edge → autoscroll down.
      onScrollStep: onStep,
    })

    ctrl.start()
    runFrames(1, 50)
    expect(el.scrollTop).toBeGreaterThan(0)
    expect(onStep).toHaveBeenCalledWith(420)
    // The tick re-queued itself for the next frame.
    expect(ctrl.isRunning()).toBe(true)
  })

  it('scrolls by speed × elapsed time: 10 px past for one second moves 10 × the per-px rate', () => {
    const { el } = makeContent(0, 400)
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => 410,
      onScrollStep: () => {},
    })

    ctrl.start()
    runFrames(60, 1000 / 60)
    expect(el.scrollTop).toBeCloseTo(10 * AUTOSCROLL_PX_PER_SEC_PER_PX_PAST, -1)
  })

  // A 120 Hz ProMotion display fires twice the frames; the distance covered must not double.
  it('covers the same distance at 60 Hz and 120 Hz', () => {
    const at60 = makeContent(0, 400)
    const ctrl60 = createViewerAutoscroll({
      getContentRef: () => at60.el,
      getPointerY: () => 410,
      onScrollStep: () => {},
    })
    ctrl60.start()
    runFrames(30, 1000 / 60)
    ctrl60.stop()

    const at120 = makeContent(0, 400)
    const ctrl120 = createViewerAutoscroll({
      getContentRef: () => at120.el,
      getPointerY: () => 410,
      onScrollStep: () => {},
    })
    ctrl120.start()
    runFrames(60, 1000 / 120)

    expect(Math.abs(at120.el.scrollTop - at60.el.scrollTop)).toBeLessThanOrEqual(1)
  })

  it('keeps moving at a crawl: sub-pixel frames accumulate instead of rounding to zero', () => {
    const { el } = makeContent(0, 400)
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => 401, // 1 px past: well under 1 px per frame.
      onScrollStep: () => {},
    })

    ctrl.start()
    runFrames(60, 1000 / 60)
    expect(el.scrollTop).toBeGreaterThan(0)
  })

  it('converts content px to scroll px when the scroll range is scaled down', () => {
    const plain = makeContent(0, 400)
    const plainCtrl = createViewerAutoscroll({
      getContentRef: () => plain.el,
      getPointerY: () => 410,
      onScrollStep: () => {},
    })
    plainCtrl.start()
    runFrames(60, 1000 / 60)
    plainCtrl.stop()

    const scaled = makeContent(0, 400)
    const scaledCtrl = createViewerAutoscroll({
      getContentRef: () => scaled.el,
      getPointerY: () => 410,
      getScrollScale: () => 0.5,
      onScrollStep: () => {},
    })
    scaledCtrl.start()
    runFrames(60, 1000 / 60)

    expect(scaled.el.scrollTop).toBeCloseTo(plain.el.scrollTop / 2, -1)
  })

  it('loop self-terminates when the pointer re-enters the viewport', () => {
    const { el } = makeContent(0, 400)
    let y = -5
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => y,
      onScrollStep: () => {},
    })

    ctrl.start()
    runOneFrame() // First frame: pointer is above the top, so we scroll up.
    expect(ctrl.isRunning()).toBe(true)
    y = 5 // Back inside, even if only just.
    runOneFrame() // No autoscroll, loop terminates.
    expect(ctrl.isRunning()).toBe(false)
  })

  it('loop self-terminates when the content ref disappears (unmount mid-drag)', () => {
    let contentRef: HTMLElement | undefined = makeContent(0, 400).el
    const ctrl = createViewerAutoscroll({
      getContentRef: () => contentRef,
      getPointerY: () => -5,
      onScrollStep: () => {},
    })

    ctrl.start()
    expect(ctrl.isRunning()).toBe(true)
    contentRef = undefined
    runOneFrame()
    expect(ctrl.isRunning()).toBe(false)
  })

  it('stop() during a tick prevents the next frame', () => {
    const { el } = makeContent(0, 400)
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => -5,
      onScrollStep: () => {},
    })

    ctrl.start()
    runOneFrame() // First frame fires and re-queues.
    expect(ctrl.isRunning()).toBe(true)
    ctrl.stop()
    expect(ctrl.isRunning()).toBe(false)
    // No more frames will fire.
    expect(scheduledTick).toBeNull()
  })

  describe('prefers-reduced-motion', () => {
    it('steps once per `start()`, by the distance past the edge, instead of running a RAF loop', () => {
      const { el } = makeContent(0, 400)
      const onStep = vi.fn()
      const ctrl = createViewerAutoscroll({
        getContentRef: () => el,
        getPointerY: () => 405, // 5 px past the bottom; would scroll down.
        onScrollStep: onStep,
        prefersReducedMotion: () => true,
      })

      ctrl.start()
      // The single step ran synchronously inside `start()` — no RAF queued. One WebKit
      // autoscroll tick's worth: the distance past the edge, not a page.
      expect(el.scrollTop).toBe(5)
      expect(onStep).toHaveBeenCalledTimes(1)
      expect(scheduledTick).toBeNull()
      expect(ctrl.isRunning()).toBe(false)
    })

    it('each subsequent `start()` steps again (the page calls it on every pointermove)', () => {
      const { el } = makeContent(0, 400)
      const onStep = vi.fn()
      const ctrl = createViewerAutoscroll({
        getContentRef: () => el,
        getPointerY: () => 405,
        onScrollStep: onStep,
        prefersReducedMotion: () => true,
      })

      ctrl.start()
      const after1 = el.scrollTop
      ctrl.start()
      const after2 = el.scrollTop
      ctrl.start()
      const after3 = el.scrollTop

      expect(after2).toBeGreaterThan(after1)
      expect(after3).toBeGreaterThan(after2)
      expect(onStep).toHaveBeenCalledTimes(3)
    })

    it('an in-viewport pointer does not move scrollTop even under reduced motion', () => {
      const { el } = makeContent(0, 400)
      const onStep = vi.fn()
      const ctrl = createViewerAutoscroll({
        getContentRef: () => el,
        getPointerY: () => 395, // Near the edge but inside: no autoscroll.
        onScrollStep: onStep,
        prefersReducedMotion: () => true,
      })

      ctrl.start()
      expect(el.scrollTop).toBe(0)
      expect(onStep).not.toHaveBeenCalled()
    })
  })
})
