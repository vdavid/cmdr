import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'

import { createViewerAutoscroll } from './viewer-autoscroll.svelte'

/**
 * Stub `requestAnimationFrame` / `cancelAnimationFrame` so tests can drive the loop
 * deterministically. Each `start()` queues a tick; the test then calls `runOneFrame()`
 * to fire it.
 */
let scheduledTick: (() => void) | null = null
let nextRafId = 1

function rafStub(cb: FrameRequestCallback): number {
  scheduledTick = () => {
    cb(performance.now())
  }
  return nextRafId++
}

function cancelStub(): void {
  scheduledTick = null
}

function runOneFrame(): void {
  const fn = scheduledTick
  scheduledTick = null
  fn?.()
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

  it('loop scrolls and calls onScrollStep when the pointer is near the edge', () => {
    const { el } = makeContent(0, 400)
    const onStep = vi.fn()
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => 395, // 5 px from the bottom edge → autoscroll down.
      onScrollStep: onStep,
    })

    ctrl.start()
    runOneFrame()
    expect(el.scrollTop).toBeGreaterThan(0)
    expect(onStep).toHaveBeenCalledTimes(1)
    expect(onStep).toHaveBeenCalledWith(395)
    // The tick re-queued itself for the next frame.
    expect(ctrl.isRunning()).toBe(true)
  })

  it('loop self-terminates when the pointer re-enters the safe band', () => {
    const { el } = makeContent(0, 400)
    let y = 5
    const ctrl = createViewerAutoscroll({
      getContentRef: () => el,
      getPointerY: () => y,
      onScrollStep: () => {},
    })

    ctrl.start()
    runOneFrame() // First frame: pointer is at y=5 (near top), so we scroll up.
    expect(ctrl.isRunning()).toBe(true)
    y = 200 // Move pointer back to the middle.
    runOneFrame() // delta = 0, loop terminates.
    expect(ctrl.isRunning()).toBe(false)
  })

  it('loop self-terminates when the content ref disappears (unmount mid-drag)', () => {
    let contentRef: HTMLElement | undefined = makeContent(0, 400).el
    const ctrl = createViewerAutoscroll({
      getContentRef: () => contentRef,
      getPointerY: () => 5,
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
      getPointerY: () => 5,
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
})
