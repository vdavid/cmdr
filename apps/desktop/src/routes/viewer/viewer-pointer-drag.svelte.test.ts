/**
 * Focus behaviour of the viewer's pointer/drag controller.
 *
 * The selection-by-pointer math is covered by `viewer-pointer.test.ts`; what's tested
 * here is the DOM-focus side effect, because the keyboard router decides where ⌘C goes
 * by looking at `document.activeElement`.
 */
import { describe, it, expect, beforeEach, afterEach, vi, type Mock } from 'vitest'

import { createViewerPointerDrag } from './viewer-pointer-drag.svelte'
import type { LineOffset } from './selection.svelte'

type SetOffset = (offset: LineOffset) => void

interface Harness {
  container: HTMLElement
  content: HTMLElement
  searchInput: HTMLInputElement
  setAnchor: Mock<SetOffset>
  setFocus: Mock<SetOffset>
}

/**
 * Builds a minimal viewer DOM: the focusable `.viewer-container` the page focuses after
 * a session opens, a `.file-content` holding one rendered line, and a focused search
 * input standing in for the search bar.
 */
function mountHarness(): Harness {
  const container = document.createElement('main')
  container.className = 'viewer-container'
  container.tabIndex = -1

  const content = document.createElement('div')
  content.className = 'file-content'
  content.tabIndex = 0
  content.innerHTML = '<div data-line="0"><span class="line-text">hello world</span></div>'

  const searchInput = document.createElement('input')
  searchInput.type = 'search'

  container.append(content, searchInput)
  document.body.append(container)
  searchInput.focus()

  return { container, content, searchInput, setAnchor: vi.fn<SetOffset>(), setFocus: vi.fn<SetOffset>() }
}

/** Wires the controller against the harness DOM. */
function createDrag(harness: Harness, lineText: string | undefined) {
  return createViewerPointerDrag({
    getContentRef: () => harness.content,
    getLineText: () => lineText,
    hasSelection: () => false,
    setAnchor: harness.setAnchor,
    setFocus: harness.setFocus,
    takeFocus: () => {
      harness.container.focus({ preventScroll: true })
    },
  })
}

/** Points `caretRangeFromPoint` at the middle of the rendered line. */
function stubCaretAtLineStart(content: HTMLElement): void {
  const textNode = content.querySelector('.line-text')?.firstChild
  Object.defineProperty(document, 'caretRangeFromPoint', {
    configurable: true,
    value: () => (textNode ? { startContainer: textNode, startOffset: 3 } : null),
  })
}

/** A left-button pointerdown over the rendered line. */
function pointerDown(button = 0): PointerEvent {
  return new PointerEvent('pointerdown', {
    bubbles: true,
    cancelable: true,
    clientX: 10,
    clientY: 10,
    button,
    pointerId: 1,
  })
}

let harness: Harness

beforeEach(() => {
  harness = mountHarness()
})

afterEach(() => {
  document.body.innerHTML = ''
  Reflect.deleteProperty(document, 'caretRangeFromPoint')
})

describe('viewer pointer drag focus', () => {
  it('moves focus to the file content when a selection gesture starts', () => {
    stubCaretAtLineStart(harness.content)
    const drag = createDrag(harness, 'hello world')

    expect(document.activeElement).toBe(harness.searchInput)

    drag.handlePointerDown(pointerDown())

    // Without this, the search input keeps focus through the drag (the handler's
    // `preventDefault()` suppresses the native focus move), so ⌘C copies the query.
    expect(document.activeElement).toBe(harness.container)
    expect(harness.setAnchor).toHaveBeenCalledOnce()
  })

  it('moves focus even when the point resolves to no caret', () => {
    const drag = createDrag(harness, undefined)

    // No `caretRangeFromPoint` stub: the gutter / spacer case, where the handler bails
    // before touching the selection. The click still belongs to the document.
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
