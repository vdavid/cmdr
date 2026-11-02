/**
 * Toasts stay clear of the row the keyboard is on (WCAG 2.4.11): the stack sits at the top
 * right as always, moves to the bottom right only while it would cover the focused cursor
 * row, and only when the bottom is clear.
 */

import { describe, it, expect, afterEach, vi } from 'vitest'
import { eventually } from '#lib/test-frames.js'
import {
  bottomChromeInset,
  pickToastEdge,
  placementRects,
  watchToastPlacement,
  type ToastEdge,
} from './toast-placement'

const VIEWPORT = { width: 1200, height: 800 }

/** A stack 360 wide and 120 tall, 16 px in from the top right corner. */
const STACK_AT_TOP = { top: 16, bottom: 136, left: 824, right: 1184 }

function row(top: number, left = 600, right = 1180): DOMRect {
  return { top, bottom: top + 24, left, right, width: right - left, height: 24, x: left, y: top } as DOMRect
}

describe('placementRects', () => {
  it('mirrors the stack to the bottom with the same gap', () => {
    const rects = placementRects(STACK_AT_TOP, 'top', VIEWPORT.height)
    expect(rects.top).toEqual(STACK_AT_TOP)
    expect(rects.bottom).toEqual({ top: 664, bottom: 784, left: 824, right: 1184 })
  })

  it('mirrors a stack at the bottom back to the top', () => {
    const rects = placementRects({ top: 664, bottom: 784, left: 824, right: 1184 }, 'bottom', VIEWPORT.height)
    expect(rects.top).toEqual(STACK_AT_TOP)
  })
})

describe('keeping the bottom position above the function key bar', () => {
  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('sits the bottom placement above window chrome at the bottom', () => {
    const rects = placementRects(STACK_AT_TOP, 'top', VIEWPORT.height, 30)
    expect(rects.bottom).toEqual({ top: 634, bottom: 754, left: 824, right: 1184 })
  })

  it('mirrors a stack already above the chrome back to the same top gap', () => {
    const rects = placementRects({ top: 634, bottom: 754, left: 824, right: 1184 }, 'bottom', VIEWPORT.height, 30)
    expect(rects.top).toEqual(STACK_AT_TOP)
  })

  it('measures the function key bar’s real height, and nothing while it’s hidden', () => {
    expect(bottomChromeInset(VIEWPORT.height)).toBe(0)

    const bar = document.createElement('div')
    bar.setAttribute('data-toast-bottom-inset', '')
    bar.getBoundingClientRect = () => ({ top: 771, bottom: 800, height: 29 }) as DOMRect
    document.body.appendChild(bar)

    expect(bottomChromeInset(VIEWPORT.height)).toBe(29)
  })
})

describe('pickToastEdge', () => {
  const rects = placementRects(STACK_AT_TOP, 'top', VIEWPORT.height)

  it('stays at the top while the cursor row is elsewhere (the common case)', () => {
    expect(pickToastEdge({ current: 'top', rects, keepClear: row(400) })).toBe('top')
  })

  it('stays put with nothing to keep clear', () => {
    expect(pickToastEdge({ current: 'bottom', rects, keepClear: null })).toBe('bottom')
  })

  it('moves to the bottom while it covers the cursor row', () => {
    expect(pickToastEdge({ current: 'top', rects, keepClear: row(100) })).toBe('bottom')
  })

  it('leaves a row in the left pane alone', () => {
    expect(pickToastEdge({ current: 'top', rects, keepClear: row(100, 0, 590) })).toBe('top')
  })

  it('stays at the bottom while that’s clear, rather than jumping back as the cursor moves on', () => {
    expect(pickToastEdge({ current: 'bottom', rects, keepClear: row(400) })).toBe('bottom')
  })

  it('moves back up when the cursor row reaches the bottom', () => {
    expect(pickToastEdge({ current: 'bottom', rects, keepClear: row(700) })).toBe('top')
  })

  it('stays where it is when both places would cover the row', () => {
    const tall = { top: 0, bottom: 800, left: 600, right: 1180 }
    expect(pickToastEdge({ current: 'top', rects, keepClear: tall })).toBe('top')
  })
})

describe('watchToastPlacement', () => {
  let stop: (() => void) | null = null

  afterEach(() => {
    stop?.()
    stop = null
    document.body.innerHTML = ''
  })

  function rectOf(element: HTMLElement, rect: { top: number; bottom: number; left: number; right: number }): void {
    element.getBoundingClientRect = () =>
      ({ ...rect, width: rect.right - rect.left, height: rect.bottom - rect.top, x: rect.left, y: rect.top }) as DOMRect
  }

  /** A focused listbox naming its cursor row through `aria-activedescendant`, the file list's model. */
  function mountList(cursorTop: number): { list: HTMLElement; rows: HTMLElement[] } {
    const list = document.createElement('div')
    list.setAttribute('role', 'listbox')
    list.tabIndex = -1
    const rows = [0, 1].map((i) => {
      const option = document.createElement('div')
      option.setAttribute('role', 'option')
      option.id = `row-${String(i)}`
      list.appendChild(option)
      return option
    })
    rectOf(rows[0], row(cursorTop))
    rectOf(rows[1], row(400))
    document.body.appendChild(list)
    list.focus()
    list.setAttribute('aria-activedescendant', 'row-0')
    return { list, rows }
  }

  /** Waits for the watcher's frame to read the stack's rect, which it does before reporting in that same turn. */
  async function untilMeasured(measure: () => void): Promise<void> {
    await eventually(() => {
      expect(measure).toHaveBeenCalled()
    })
  }

  it('moves the stack off the focused cursor row, and back once the row moves away from the bottom', async () => {
    window.innerHeight = VIEWPORT.height
    const stack = document.createElement('div')
    rectOf(stack, STACK_AT_TOP)
    document.body.appendChild(stack)
    const { list } = mountList(100)
    const edges: ToastEdge[] = []
    stop = watchToastPlacement(stack, ({ edge }) => {
      edges.push(edge)
    }).stop

    await eventually(() => {
      expect(edges.at(-1)).toBe('bottom')
    })

    rectOf(stack, { top: 664, bottom: 784, left: 824, right: 1184 })
    const bottomRow = document.createElement('div')
    bottomRow.id = 'row-bottom'
    rectOf(bottomRow, row(700))
    list.appendChild(bottomRow)
    list.setAttribute('aria-activedescendant', 'row-bottom')
    await eventually(() => {
      expect(edges.at(-1)).toBe('top')
    })
  })

  it('reports nothing while the cursor row is clear', async () => {
    window.innerHeight = VIEWPORT.height
    const stack = document.createElement('div')
    rectOf(stack, STACK_AT_TOP)
    document.body.appendChild(stack)
    mountList(400)
    const measure = vi.spyOn(stack, 'getBoundingClientRect')
    const onEdge = vi.fn()
    stop = watchToastPlacement(stack, onEdge).stop

    await untilMeasured(measure)
    expect(onEdge).not.toHaveBeenCalled()
  })

  it('re-measures the bottom chrome when asked, so a function key bar toggled on lifts the stack at once', async () => {
    window.innerHeight = VIEWPORT.height
    const stack = document.createElement('div')
    rectOf(stack, STACK_AT_TOP)
    document.body.appendChild(stack)
    mountList(400)
    const measure = vi.spyOn(stack, 'getBoundingClientRect')
    const insets: number[] = []
    const watch = watchToastPlacement(stack, ({ bottomInset }) => {
      insets.push(bottomInset)
    })
    stop = watch.stop
    await untilMeasured(measure)

    const bar = document.createElement('div')
    bar.setAttribute('data-toast-bottom-inset', '')
    rectOf(bar, { top: 771, bottom: 800, left: 0, right: 1200 })
    document.body.appendChild(bar)
    watch.remeasure()

    await eventually(() => {
      expect(insets.at(-1)).toBe(29)
    })
  })
})
