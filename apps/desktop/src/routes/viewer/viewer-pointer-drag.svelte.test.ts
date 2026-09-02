/**
 * Focus and drag-extension behaviour of the viewer's pointer/drag controller.
 *
 * The point → offset math is covered by `viewer-pointer.test.ts` and
 * `viewer-caret-geometry.test.ts`; what's tested here is the DOM-focus side effect
 * (because the keyboard router decides where ⌘C goes by looking at
 * `document.activeElement`) and the drag/autoscroll wiring on top of it.
 */
import { describe, it, expect, beforeEach, afterEach, vi, type Mock } from 'vitest'

import { createViewerPointerDrag } from './viewer-pointer-drag.svelte'
import type { LineOffset } from './selection.svelte'

type SetOffset = (offset: LineOffset) => void

/** The fake layout the harness stubs: a content box exactly filled by two rendered rows. */
const ROW_H = 18
const CONTENT = { left: 0, top: 0, right: 400, bottom: 2 * ROW_H }
const TEXT_LEFT = 40
const CHAR_W = 8

interface Harness {
  container: HTMLElement
  content: HTMLElement
  searchInput: HTMLInputElement
  setAnchor: Mock<SetOffset>
  setFocus: Mock<SetOffset>
}

function rect(left: number, top: number, right: number, bottom: number): DOMRect {
  return { left, top, right, bottom, width: right - left, height: bottom - top } as unknown as DOMRect
}

/**
 * Builds a minimal viewer DOM: the focusable `.viewer-container` the page focuses after
 * a session opens, a `.file-content` holding two rendered lines, and a focused search
 * input standing in for the search bar. Rects are stubbed as a monospace grid, the same
 * shape `viewer-pointer.test.ts` uses, so the caret resolver has real geometry to read.
 */
function mountHarness(): Harness {
  const container = document.createElement('main')
  container.className = 'viewer-container'
  container.tabIndex = -1

  const content = document.createElement('div')
  content.className = 'file-content'
  content.tabIndex = 0
  content.innerHTML =
    '<div data-line="0"><span class="line-number">1</span><span class="line-text">hello world</span></div>' +
    '<div data-line="1"><span class="line-number">2</span><span class="line-text">second line</span></div>'

  content.getBoundingClientRect = () => rect(CONTENT.left, CONTENT.top, CONTENT.right, CONTENT.bottom)
  const rows = content.querySelectorAll<HTMLElement>('[data-line]')
  const starts = new Map<Node, number>()
  for (const [index, row] of rows.entries()) {
    row.getBoundingClientRect = () => rect(CONTENT.left, index * ROW_H, CONTENT.right, (index + 1) * ROW_H)
    const textNode = row.querySelector('.line-text')?.firstChild
    if (textNode) starts.set(textNode, index * ROW_H)
  }

  vi.spyOn(Range.prototype, 'getClientRects').mockImplementation(function (this: Range) {
    const rowTop = starts.get(this.startContainer)
    if (rowTop === undefined) return [] as unknown as DOMRectList
    return [
      rect(TEXT_LEFT + this.startOffset * CHAR_W, rowTop, TEXT_LEFT + this.endOffset * CHAR_W, rowTop + ROW_H),
    ] as unknown as DOMRectList
  })

  const searchInput = document.createElement('input')
  searchInput.type = 'search'

  container.append(content, searchInput)
  document.body.append(container)
  searchInput.focus()

  return { container, content, searchInput, setAnchor: vi.fn<SetOffset>(), setFocus: vi.fn<SetOffset>() }
}

/** Wires the controller against the harness DOM. */
function createDrag(harness: Harness, lineText: string | undefined, content = harness.content) {
  return createViewerPointerDrag({
    getContentRef: () => content,
    getLineText: () => lineText,
    hasSelection: () => false,
    setAnchor: harness.setAnchor,
    setFocus: harness.setFocus,
    takeFocus: () => {
      harness.container.focus({ preventScroll: true })
    },
  })
}

function pointerEvent(type: string, { x = 10, y = 10, button = 0 } = {}): PointerEvent {
  return new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button, pointerId: 1 })
}

/** A left-button pointerdown over the rendered line. */
function pointerDown(button = 0): PointerEvent {
  return pointerEvent('pointerdown', { button })
}

let harness: Harness

beforeEach(() => {
  harness = mountHarness()
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  document.body.innerHTML = ''
})

describe('viewer pointer drag focus', () => {
  it('moves focus to the file content when a selection gesture starts', () => {
    const drag = createDrag(harness, 'hello world')

    expect(document.activeElement).toBe(harness.searchInput)

    drag.handlePointerDown(pointerDown())

    // Without this, the search input keeps focus through the drag (the handler's
    // `preventDefault()` suppresses the native focus move), so ⌘C copies the query.
    expect(document.activeElement).toBe(harness.container)
    expect(harness.setAnchor).toHaveBeenCalledOnce()
  })

  it('moves focus even when the point resolves to no caret', () => {
    // An unrendered content element: nothing to anchor to, but the click still belongs
    // to the document.
    const empty = document.createElement('div')
    const drag = createDrag(harness, undefined, empty)

    drag.handlePointerDown(pointerDown())

    expect(document.activeElement).toBe(harness.container)
    expect(harness.setAnchor).not.toHaveBeenCalled()
  })

  it('leaves focus alone for a non-primary button', () => {
    const drag = createDrag(harness, 'hello world')

    drag.handlePointerDown(pointerDown(2))

    expect(document.activeElement).toBe(harness.searchInput)
  })
})

describe('viewer pointer drag extension', () => {
  it('anchors in the line-number gutter instead of dropping the whole gesture', () => {
    const drag = createDrag(harness, 'hello world')

    drag.handlePointerDown(pointerEvent('pointerdown', { x: 4, y: ROW_H + 9 }))

    expect(harness.setAnchor).toHaveBeenCalledWith({ line: 1, offset: 0 })
  })

  it('keeps extending the selection when the pointer leaves the viewport', () => {
    const drag = createDrag(harness, 'hello world')
    drag.handlePointerDown(pointerDown())

    drag.handlePointerMove(pointerEvent('pointermove', { x: 4000, y: CONTENT.bottom + 500 }))

    // Clamped into the content box: the end of the last rendered row, not nothing.
    expect(harness.setFocus).toHaveBeenLastCalledWith({ line: 1, offset: 11 })
  })

  it('re-aims the selection after an autoscroll step', () => {
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => frames.push(cb))
    vi.stubGlobal('cancelAnimationFrame', () => undefined)

    const drag = createDrag(harness, 'hello world')
    drag.handlePointerDown(pointerDown())
    // Past the bottom edge, so the autoscroll loop starts.
    drag.handlePointerMove(pointerEvent('pointermove', { x: TEXT_LEFT + CHAR_W, y: CONTENT.bottom + 40 }))
    expect(frames).toHaveLength(1)

    harness.setFocus.mockClear()
    frames[0](0)

    // Dragging below the viewport sweeps whole rows: the end of the bottom visible row.
    expect(harness.setFocus).toHaveBeenCalledWith({ line: 1, offset: 11 })
  })
})

describe('viewer multi-click selection', () => {
  /** x that lands on offset 6 of `hello world`, the `w` of `world`. */
  const WORD_X = TEXT_LEFT + 6 * CHAR_W + 2
  const WORD_Y = ROW_H / 2

  /** Presses and releases at the same spot `times` times, as a person clicking does. */
  function clickTimes(drag: ReturnType<typeof createDrag>, times: number, x = WORD_X, y = WORD_Y): void {
    for (let i = 0; i < times; i++) {
      drag.handlePointerDown(pointerEvent('pointerdown', { x, y }))
      drag.handlePointerUp(pointerEvent('pointerup', { x, y }))
    }
  }

  it('selects the word on the second press', () => {
    const drag = createDrag(harness, 'hello world')

    clickTimes(drag, 2)

    expect(harness.setAnchor).toHaveBeenLastCalledWith({ line: 0, offset: 6 })
    expect(harness.setFocus).toHaveBeenLastCalledWith({ line: 0, offset: 11 })
  })

  it('selects the whole logical line on the third press', () => {
    const drag = createDrag(harness, 'hello world')

    clickTimes(drag, 3)

    expect(harness.setAnchor).toHaveBeenLastCalledWith({ line: 0, offset: 0 })
    expect(harness.setFocus).toHaveBeenLastCalledWith({ line: 0, offset: 11 })
  })

  it('restarts the cycle on the fourth press instead of leaving the line selected', () => {
    const drag = createDrag(harness, 'hello world')

    clickTimes(drag, 3)
    harness.setAnchor.mockClear()
    harness.setFocus.mockClear()
    clickTimes(drag, 1)

    // Back to a plain click: a fresh caret anchor where the pointer is (which collapses
    // the selection), and no focus move to stretch it anywhere.
    expect(harness.setAnchor).toHaveBeenCalledExactlyOnceWith({ line: 0, offset: 6 })
    expect(harness.setFocus).not.toHaveBeenCalled()
  })

  it('treats a press that drifted away as a fresh single click', () => {
    const drag = createDrag(harness, 'hello world')

    clickTimes(drag, 1)
    clickTimes(drag, 1, WORD_X + 40)

    // The second press is its own gesture, so it anchors a caret rather than a word.
    expect(harness.setAnchor).toHaveBeenLastCalledWith({ line: 0, offset: 11 })
    expect(harness.setFocus).not.toHaveBeenCalled()
  })

  it('keeps the word selected when the pointer twitches after a double-click', () => {
    const drag = createDrag(harness, 'hello world')

    clickTimes(drag, 2)
    harness.setFocus.mockClear()
    drag.handlePointerMove(pointerEvent('pointermove', { x: WORD_X + 2, y: WORD_Y }))

    // A multi-click press doesn't arm the drag: a stray move can't collapse the word
    // selection back to a caret.
    expect(harness.setFocus).not.toHaveBeenCalled()
  })

  it('selects the whole logical line even when the line wraps across visual rows', () => {
    // The harness renders one row per line, so the wrap case is expressed by the line
    // text being longer than the row: the selection still runs to the logical end.
    const drag = createDrag(harness, 'hello world and then some more text')

    clickTimes(drag, 3)

    expect(harness.setAnchor).toHaveBeenLastCalledWith({ line: 0, offset: 0 })
    expect(harness.setFocus).toHaveBeenLastCalledWith({ line: 0, offset: 35 })
  })
})
