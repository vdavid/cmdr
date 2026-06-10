import { describe, it, expect, beforeEach, afterEach } from 'vitest'

import { computeAvailableTextWidth, createTextWidthTracker } from './viewer-text-width.svelte'

/** Stub rAF so tests can flush the tracker's deferred measurements synchronously. */
let rafQueue: FrameRequestCallback[] = []
let originalRaf: typeof requestAnimationFrame

function flushRaf(): void {
  const queue = rafQueue
  rafQueue = []
  for (const cb of queue) cb(performance.now())
}

beforeEach(() => {
  originalRaf = globalThis.requestAnimationFrame
  globalThis.requestAnimationFrame = (cb: FrameRequestCallback): number => {
    rafQueue.push(cb)
    return rafQueue.length
  }
})

afterEach(() => {
  globalThis.requestAnimationFrame = originalRaf
  rafQueue = []
  document.body.innerHTML = ''
})

function makeRect(width: number): DOMRect {
  return {
    width,
    height: 18,
    top: 0,
    left: 0,
    right: width,
    bottom: 18,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  }
}

/**
 * Builds the viewer's content DOM with a SHORT first line ("# Cmdr", 44px of
 * text) inside a much wider viewport. Mirrors the regression case: the wrap
 * width must come from the row geometry, never from the first line's own
 * shrink-wrapped text span.
 */
function makeViewerDom({
  contentClientWidth,
  gutterWidth,
  firstLineTextWidth,
}: {
  contentClientWidth: number
  gutterWidth: number
  firstLineTextWidth: number
}): HTMLDivElement {
  const content = document.createElement('div')
  content.className = 'file-content'
  // happy-dom doesn't lay anything out; stub the scroll container's client width.
  Object.defineProperty(content, 'clientWidth', { value: contentClientWidth, configurable: true })

  const line = document.createElement('div')
  line.className = 'line'
  line.style.paddingLeft = '8px'
  line.style.paddingRight = '8px'

  const lineNumber = document.createElement('span')
  lineNumber.className = 'line-number'
  lineNumber.style.marginRight = '8px'
  lineNumber.getBoundingClientRect = () => makeRect(gutterWidth)

  const lineText = document.createElement('span')
  lineText.className = 'line-text'
  lineText.textContent = '# Cmdr'
  lineText.getBoundingClientRect = () => makeRect(firstLineTextWidth)

  line.append(lineNumber, lineText)
  content.appendChild(line)
  document.body.appendChild(content)
  return content
}

describe('computeAvailableTextWidth', () => {
  it('subtracts line padding and the gutter from the container width', () => {
    expect(
      computeAvailableTextWidth({
        contentClientWidth: 800,
        linePaddingLeft: 8,
        linePaddingRight: 8,
        gutterOuterWidth: 48,
      }),
    ).toBe(736)
  })

  it('clamps to 0 when the gutter eats the whole width', () => {
    expect(
      computeAvailableTextWidth({
        contentClientWidth: 40,
        linePaddingLeft: 8,
        linePaddingRight: 8,
        gutterOuterWidth: 48,
      }),
    ).toBe(0)
  })
})

describe('createTextWidthTracker', () => {
  it('measures the available row width, not the first line text span (which shrink-wraps to its content)', () => {
    const content = makeViewerDom({ contentClientWidth: 800, gutterWidth: 40, firstLineTextWidth: 44 })
    const tracker = createTextWidthTracker({
      getContentRef: () => content,
      getVisibleLinesKey: () => 1,
    })

    tracker.runVisibleLinesEffect()
    flushRaf()

    // 800 (scroll container) - 8 - 8 (line padding) - 40 (gutter) - 8 (gutter margin) = 736.
    // Pre-fix this returned 44 (the width of "# Cmdr"), which made the word-wrap
    // height map wrap every line at 44px and inflate the scroll height ~7x.
    expect(tracker.textWidth).toBe(736)
  })
})
