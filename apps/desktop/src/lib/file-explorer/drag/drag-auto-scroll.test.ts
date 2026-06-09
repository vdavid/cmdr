import { describe, expect, it } from 'vitest'
import { computeDragAutoScrollStep, type DragAutoScrollRect } from './drag-auto-scroll'

const rect: DragAutoScrollRect = { top: 100, right: 500, bottom: 500, left: 100 }

describe('computeDragAutoScrollStep', () => {
  it('stays inactive away from the edge band', () => {
    expect(
      computeDragAutoScrollStep({
        axis: 'vertical',
        pointer: { x: 250, y: 250 },
        rect,
        scrollOffset: 100,
        maxScrollOffset: 1000,
        elapsedMs: 16,
      }),
    ).toMatchObject({ active: false, scrolled: false, delta: 0, nextScrollOffset: 100 })
  })

  it('scrolls up in the vertical start band', () => {
    const step = computeDragAutoScrollStep({
      axis: 'vertical',
      pointer: { x: 250, y: 110 },
      rect,
      scrollOffset: 100,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })

    expect(step.active).toBe(true)
    expect(step.delta).toBeLessThan(0)
    expect(step.nextScrollOffset).toBeLessThan(100)
  })

  it('scrolls down in the vertical end band', () => {
    const step = computeDragAutoScrollStep({
      axis: 'vertical',
      pointer: { x: 250, y: 490 },
      rect,
      scrollOffset: 100,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })

    expect(step.active).toBe(true)
    expect(step.delta).toBeGreaterThan(0)
    expect(step.nextScrollOffset).toBeGreaterThan(100)
  })

  it('uses horizontal left and right bands for brief mode', () => {
    const left = computeDragAutoScrollStep({
      axis: 'horizontal',
      pointer: { x: 110, y: 250 },
      rect,
      scrollOffset: 100,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })
    const right = computeDragAutoScrollStep({
      axis: 'horizontal',
      pointer: { x: 490, y: 250 },
      rect,
      scrollOffset: 100,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })

    expect(left.delta).toBeLessThan(0)
    expect(right.delta).toBeGreaterThan(0)
  })

  it('does not activate when the cross-axis coordinate is outside the rect', () => {
    const step = computeDragAutoScrollStep({
      axis: 'vertical',
      pointer: { x: 50, y: 490 },
      rect,
      scrollOffset: 100,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })

    expect(step).toMatchObject({ active: false, scrolled: false, delta: 0, nextScrollOffset: 100 })
  })

  it('clamps at the scroll boundaries and stops the loop', () => {
    const atTop = computeDragAutoScrollStep({
      axis: 'vertical',
      pointer: { x: 250, y: 110 },
      rect,
      scrollOffset: 0,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })
    const atBottom = computeDragAutoScrollStep({
      axis: 'vertical',
      pointer: { x: 250, y: 490 },
      rect,
      scrollOffset: 1000,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })

    expect(atTop).toMatchObject({ active: false, scrolled: false, nextScrollOffset: 0 })
    expect(atBottom).toMatchObject({ active: false, scrolled: false, nextScrollOffset: 1000 })
  })

  it('accelerates as the pointer gets closer to the edge', () => {
    const shallow = computeDragAutoScrollStep({
      axis: 'vertical',
      pointer: { x: 250, y: 145 },
      rect,
      scrollOffset: 100,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })
    const deep = computeDragAutoScrollStep({
      axis: 'vertical',
      pointer: { x: 250, y: 105 },
      rect,
      scrollOffset: 100,
      maxScrollOffset: 1000,
      elapsedMs: 16,
    })

    expect(Math.abs(deep.delta)).toBeGreaterThan(Math.abs(shallow.delta))
  })
})
