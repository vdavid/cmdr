import { describe, it, expect, afterEach, vi } from 'vitest'

import { caretFromPoint, caretFromPointClamped, caretRectFor } from './viewer-pointer'

/**
 * The fake layout every test measures against. `.file-content` fills a 400x200 box,
 * the rendered rows start `PAD_TOP` below its top (the blank strip a short file leaves
 * above nothing, and below its last row), and `.line-text` starts at `TEXT_LEFT` with
 * the line-number gutter to its left.
 */
const CONTENT = { left: 0, top: 0, right: 400, bottom: 200 }
const PAD_TOP = 20
const TEXT_LEFT = 48
const CHAR_W = 8
const ROW_H = 18

/** Viewport x just inside the left edge of column `col`, away from the rounding tie. */
function colStartX(col: number): number {
  return TEXT_LEFT + col * CHAR_W + 1
}

interface FakeRect {
  left: number
  right: number
  top: number
  bottom: number
  width: number
  height: number
}

function rect(left: number, top: number, right: number, bottom: number): FakeRect {
  return { left, top, right, bottom, width: right - left, height: bottom - top }
}

/** Where a text node sits: which rendered line, and its UTF-16 start in that line. */
interface TextNodePosition {
  line: number
  start: number
}

interface ViewerHarness {
  content: HTMLElement
}

/**
 * Builds a viewer-shaped DOM (`.file-content` > `.scroll-spacer` > `.lines-container` >
 * `.line`s, each with a `.line-number` and a `.line-text`) and stubs the geometry the
 * real layout engine would produce: a monospace grid of `CHAR_W` x `ROW_H` cells that
 * wraps every `cols` UTF-16 code units.
 */
function mountViewer(lineHtml: string[], { cols = Number.POSITIVE_INFINITY } = {}): ViewerHarness {
  const content = document.createElement('div')
  content.className = 'file-content'
  const spacer = document.createElement('div')
  spacer.className = 'scroll-spacer'
  const container = document.createElement('div')
  container.className = 'lines-container'
  spacer.append(container)
  content.append(spacer)
  document.body.append(content)

  const positions = new Map<Node, TextNodePosition>()
  const lineTops: number[] = []
  let top = CONTENT.top + PAD_TOP

  for (const [index, html] of lineHtml.entries()) {
    const line = document.createElement('div')
    line.className = 'line'
    line.setAttribute('data-line', String(index))
    const gutter = document.createElement('span')
    gutter.className = 'line-number'
    gutter.textContent = String(index + 1)
    const lineText = document.createElement('span')
    lineText.className = 'line-text'
    lineText.innerHTML = html
    line.append(gutter, lineText)
    container.append(line)

    let start = 0
    const walker = document.createTreeWalker(lineText, NodeFilter.SHOW_TEXT)
    let node = walker.nextNode()
    while (node !== null) {
      positions.set(node, { line: index, start })
      start += (node.nodeValue ?? '').length
      node = walker.nextNode()
    }

    const rows = Math.max(1, Math.ceil(start / cols))
    const lineTop = top
    const height = rows * ROW_H
    lineTops.push(lineTop)
    line.getBoundingClientRect = () =>
      rect(CONTENT.left, lineTop, CONTENT.right, lineTop + height) as unknown as DOMRect
    // `.line-text` shrink-wraps its own content, so it starts at the gutter's right edge
    // and is as wide as the text. `caretRectFor` reads it for the empty-line fallback,
    // where there is no character box to measure.
    lineText.getBoundingClientRect = () =>
      rect(TEXT_LEFT, lineTop, TEXT_LEFT + start * CHAR_W, lineTop + height) as unknown as DOMRect
    top += height
  }

  content.getBoundingClientRect = () =>
    rect(CONTENT.left, CONTENT.top, CONTENT.right, CONTENT.bottom) as unknown as DOMRect

  /** The box of `[from, to)` inside line `line`, on the grid above. */
  function boxFor(line: number, from: number, to: number): FakeRect {
    const row = Number.isFinite(cols) ? Math.floor(from / cols) : 0
    const col = from - row * (Number.isFinite(cols) ? cols : 0)
    const rowTop = lineTops[line] + row * ROW_H
    return rect(TEXT_LEFT + col * CHAR_W, rowTop, TEXT_LEFT + (col + (to - from)) * CHAR_W, rowTop + ROW_H)
  }

  function rectsForRange(range: Range): FakeRect[] {
    const at = positions.get(range.startContainer)
    if (!at) return []
    return [boxFor(at.line, at.start + range.startOffset, at.start + range.endOffset)]
  }

  vi.spyOn(Range.prototype, 'getClientRects').mockImplementation(function (this: Range) {
    return rectsForRange(this) as unknown as DOMRectList
  })

  return { content }
}

afterEach(() => {
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

describe('caretFromPoint', () => {
  it('resolves a point inside the line text to its offset', () => {
    const { content } = mountViewer(['hello world', 'second line'])
    expect(caretFromPoint(content, colStartX(4), PAD_TOP + 9)).toEqual({ line: 0, offset: 4 })
    expect(caretFromPoint(content, colStartX(2), PAD_TOP + ROW_H + 9)).toEqual({ line: 1, offset: 2 })
  })

  it('resolves the line-number gutter to the start of that line', () => {
    const { content } = mountViewer(['hello world', 'second line'])
    // x inside the gutter, y on the second row.
    expect(caretFromPoint(content, 20, PAD_TOP + ROW_H + 9)).toEqual({ line: 1, offset: 0 })
  })

  it("resolves the line's left padding to the start of that line", () => {
    const { content } = mountViewer(['hello world'])
    expect(caretFromPoint(content, 2, PAD_TOP + 9)).toEqual({ line: 0, offset: 0 })
  })

  it('resolves the blank area below the last line to the end of the last line', () => {
    const { content } = mountViewer(['hello world', 'second line'])
    expect(caretFromPoint(content, 200, 190)).toEqual({ line: 1, offset: 11 })
  })

  it('resolves the blank strip above the first rendered row to the start of that row', () => {
    const { content } = mountViewer(['hello world'])
    expect(caretFromPoint(content, 200, 5)).toEqual({ line: 0, offset: 0 })
  })

  it('clamps a point right of a row to the end of that row', () => {
    const { content } = mountViewer(['hello world'])
    expect(caretFromPoint(content, 380, PAD_TOP + 9)).toEqual({ line: 0, offset: 11 })
  })

  it('clamps to the wrap point, not the logical line end, on a wrapped line', () => {
    // "abcdefghij" at four columns: rows "abcd" / "efgh" / "ij".
    const { content } = mountViewer(['abcdefghij'], { cols: 4 })
    expect(caretFromPoint(content, 380, PAD_TOP + 9)).toEqual({ line: 0, offset: 4 })
    expect(caretFromPoint(content, 380, PAD_TOP + ROW_H + 9)).toEqual({ line: 0, offset: 8 })
    expect(caretFromPoint(content, 380, PAD_TOP + 2 * ROW_H + 9)).toEqual({ line: 0, offset: 10 })
  })

  it('resolves a point on the second visual row of a wrapped line', () => {
    const { content } = mountViewer(['abcdefghij'], { cols: 4 })
    expect(caretFromPoint(content, colStartX(1), PAD_TOP + ROW_H + 9)).toEqual({ line: 0, offset: 5 })
    expect(caretFromPoint(content, 4, PAD_TOP + 2 * ROW_H + 9)).toEqual({ line: 0, offset: 8 })
  })

  it('sums offsets across nested <mark> and <span> elements', () => {
    const { content } = mountViewer(['foo<mark>bar</mark><span class="selected">baz</span>!'])
    expect(caretFromPoint(content, colStartX(4), PAD_TOP + 9)).toEqual({ line: 0, offset: 4 })
    expect(caretFromPoint(content, colStartX(7), PAD_TOP + 9)).toEqual({ line: 0, offset: 7 })
    expect(caretFromPoint(content, colStartX(9), PAD_TOP + 9)).toEqual({ line: 0, offset: 9 })
  })

  it('never lands between the surrogates of an astral codepoint', () => {
    // "a👋b": the emoji spans offsets 1..3 and two grid cells.
    const { content } = mountViewer(['a👋b'])
    expect(caretFromPoint(content, TEXT_LEFT + CHAR_W + 1, PAD_TOP + 9)).toEqual({ line: 0, offset: 1 })
    expect(caretFromPoint(content, TEXT_LEFT + 3 * CHAR_W - 1, PAD_TOP + 9)).toEqual({ line: 0, offset: 3 })
    expect(caretFromPoint(content, colStartX(3), PAD_TOP + 9)).toEqual({ line: 0, offset: 3 })
  })

  it('resolves an empty line to offset 0', () => {
    const { content } = mountViewer(['first', '', 'third'])
    expect(caretFromPoint(content, 200, PAD_TOP + ROW_H + 9)).toEqual({ line: 1, offset: 0 })
  })

  it('returns null for points outside the content box', () => {
    const { content } = mountViewer(['hello world'])
    expect(caretFromPoint(content, 200, CONTENT.top - 10)).toBeNull() // toolbar
    expect(caretFromPoint(content, 200, CONTENT.bottom + 10)).toBeNull() // status bar
    expect(caretFromPoint(content, CONTENT.right + 10, PAD_TOP + 9)).toBeNull()
    expect(caretFromPoint(content, CONTENT.left - 10, PAD_TOP + 9)).toBeNull()
  })

  it('returns null when no lines are rendered', () => {
    const { content } = mountViewer([])
    expect(caretFromPoint(content, 200, 100)).toBeNull()
  })
})

describe('caretFromPointClamped', () => {
  it('pulls a point above the content down to the first rendered row', () => {
    const { content } = mountViewer(['hello world', 'second line'])
    expect(caretFromPointClamped(content, colStartX(3), CONTENT.top - 500)).toEqual({ line: 0, offset: 0 })
  })

  it('pulls a point below the content down to the end of the last rendered row', () => {
    const { content } = mountViewer(['hello world', 'second line'])
    expect(caretFromPointClamped(content, colStartX(3), CONTENT.bottom + 500)).toEqual({ line: 1, offset: 11 })
  })

  it('pulls a point right of the content in to the end of the row under it', () => {
    const { content } = mountViewer(['hello world', 'second line'])
    expect(caretFromPointClamped(content, CONTENT.right + 500, PAD_TOP + 9)).toEqual({ line: 0, offset: 11 })
  })
})

describe('caretRectFor', () => {
  /** A zero-width box on the grid: `col` is a character column, `row` a visual row. */
  function edge(col: number, row: number, lineTop = PAD_TOP) {
    const x = TEXT_LEFT + col * CHAR_W
    return { left: x, right: x, top: lineTop + row * ROW_H, bottom: lineTop + (row + 1) * ROW_H }
  }

  it('picks the left edge of the character at an interior offset', () => {
    const { content } = mountViewer(['hello world'])
    expect(caretRectFor(content, { line: 0, offset: 4 })).toEqual(edge(4, 0))
  })

  it('picks the left edge of the first character at the start of a line', () => {
    const { content } = mountViewer(['hello world'])
    expect(caretRectFor(content, { line: 0, offset: 0 })).toEqual(edge(0, 0))
  })

  it('picks the RIGHT edge of the last character at the end of a line', () => {
    // The edge rule's whole reason for being. `measureChar` clamps its probe to
    // `length - 1`, so offset 11 measures the box of `d` and would otherwise paint a
    // caret one glyph too far left — exactly where Shift+End and every rightward walk
    // park it.
    const { content } = mountViewer(['hello world'])
    expect(caretRectFor(content, { line: 0, offset: 11 })).toEqual(edge(11, 0))
  })

  it('clamps an offset past the end of the line to that same right edge', () => {
    const { content } = mountViewer(['hello world'])
    expect(caretRectFor(content, { line: 0, offset: 40 })).toEqual(edge(11, 0))
  })

  it('never lands between the surrogates of an astral codepoint', () => {
    // "a👋b": the emoji spans offsets 1..3 and two grid cells, so both its offsets
    // resolve to its left edge.
    const { content } = mountViewer(['a👋b'])
    expect(caretRectFor(content, { line: 0, offset: 1 })).toEqual(edge(1, 0))
    expect(caretRectFor(content, { line: 0, offset: 2 })).toEqual(edge(1, 0))
    expect(caretRectFor(content, { line: 0, offset: 3 })).toEqual(edge(3, 0))
  })

  it('sums offsets across nested <mark> and <span> elements', () => {
    const { content } = mountViewer(['foo<mark>bar</mark><span class="selected">baz</span>!'])
    expect(caretRectFor(content, { line: 0, offset: 7 })).toEqual(edge(7, 0))
    expect(caretRectFor(content, { line: 0, offset: 10 })).toEqual(edge(10, 0))
  })

  it('takes its height from the character box, not the (multi-row) wrapped line', () => {
    // "abcdefghij" at four columns: rows "abcd" / "efgh" / "ij". The row is three rows
    // tall; the caret at the line's end must be one row tall, on the last of them.
    const { content } = mountViewer(['abcdefghij'], { cols: 4 })
    expect(caretRectFor(content, { line: 0, offset: 10 })).toEqual(edge(2, 2))
  })

  it('paints an interior wrap point at the start of the next visual row', () => {
    // Documented downstream affinity: offset 4 sits between rows, and `rangeRect`
    // prefers the rect with width, which is the first glyph of row two. Leave it.
    const { content } = mountViewer(['abcdefghij'], { cols: 4 })
    expect(caretRectFor(content, { line: 0, offset: 4 })).toEqual(edge(0, 1))
  })

  it('falls back to the row rect on an empty line', () => {
    const { content } = mountViewer(['first', '', 'third'])
    expect(caretRectFor(content, { line: 1, offset: 0 })).toEqual(edge(0, 0, PAD_TOP + ROW_H))
  })

  it('returns null for a line that is not rendered', () => {
    const { content } = mountViewer(['hello world'])
    expect(caretRectFor(content, { line: 7, offset: 0 })).toBeNull()
  })

  it('returns null when the character box cannot be measured', () => {
    const { content } = mountViewer(['hello world'])
    vi.spyOn(Range.prototype, 'getClientRects').mockReturnValue([] as unknown as DOMRectList)
    expect(caretRectFor(content, { line: 0, offset: 3 })).toBeNull()
  })
})
