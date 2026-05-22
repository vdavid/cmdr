import { describe, it, expect, beforeEach } from 'vitest'

import { caretFromPoint, findLineAncestor, findLineTextNode, sumOffsetWithin } from './viewer-pointer'

/** Test-local helper: asserts that a node lookup succeeded and returns the value. */
function nn<T>(value: T | null | undefined, what: string): T {
  if (value === null || value === undefined) throw new Error(`Expected ${what} to be non-null in test fixture`)
  return value
}

/**
 * Sets up a viewer-shaped DOM fragment with one or more lines. Each line gets a
 * `[data-line]` element with `.line-text` inside. Returns the root element and a
 * helper to find the `.line-text` of line `n`.
 */
function buildLineDom(linesInnerHtml: string[]): { root: HTMLElement; getLineText: (n: number) => HTMLElement } {
  const root = document.createElement('div')
  for (let i = 0; i < linesInnerHtml.length; i++) {
    const line = document.createElement('div')
    line.className = 'line'
    line.setAttribute('data-line', String(i))
    const lineText = document.createElement('span')
    lineText.className = 'line-text'
    lineText.innerHTML = linesInnerHtml[i]
    line.appendChild(lineText)
    root.appendChild(line)
  }
  document.body.appendChild(root)
  return {
    root,
    getLineText: (n) => root.querySelectorAll('.line-text')[n] as HTMLElement,
  }
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('findLineAncestor', () => {
  it('finds the [data-line] ancestor when started from a text node', () => {
    const { root } = buildLineDom(['hello world'])
    const textNode = nn(nn(root.querySelector('.line-text'), '.line-text').firstChild, 'text node')
    const line = nn(findLineAncestor(textNode), 'line ancestor')
    expect(line.getAttribute('data-line')).toBe('0')
  })

  it('finds the [data-line] when started from a nested mark', () => {
    const { root } = buildLineDom(['hel<mark>lo</mark> world'])
    const markText = nn(nn(root.querySelector('mark'), 'mark').firstChild, 'mark text')
    const line = nn(findLineAncestor(markText), 'line ancestor')
    expect(line.getAttribute('data-line')).toBe('0')
  })

  it('returns null for nodes outside any [data-line]', () => {
    const stray = document.createElement('div')
    document.body.appendChild(stray)
    expect(findLineAncestor(stray)).toBeNull()
  })
})

describe('findLineTextNode', () => {
  it('finds the .line-text inside a line node', () => {
    const { root } = buildLineDom(['hello'])
    const line = root.querySelector('[data-line="0"]') as HTMLElement
    expect(findLineTextNode(line)).toBe(line.querySelector('.line-text'))
  })
})

describe('sumOffsetWithin', () => {
  it('plain text node: returns the caretOffset directly', () => {
    const { getLineText } = buildLineDom(['hello world'])
    const lineText = getLineText(0)
    const text = nn(lineText.firstChild, 'first child')
    expect(sumOffsetWithin(lineText, text, 6)).toBe(6)
  })

  it('caret inside a nested <mark>: sums preceding sibling text + caretOffset', () => {
    // "hello <mark>world</mark>"
    const { getLineText } = buildLineDom(['hello <mark>world</mark>'])
    const lineText = getLineText(0)
    const markText = nn(nn(lineText.querySelector('mark'), 'mark').firstChild, 'mark text')
    expect(sumOffsetWithin(lineText, markText, 2)).toBe(6 + 2) // "hello " (6) + "wo" (2)
  })

  it('caret after the <mark>: sums everything before plus offset in the trailing text', () => {
    // "foo <mark>bar</mark> baz"
    const { getLineText } = buildLineDom(['foo <mark>bar</mark> baz'])
    const lineText = getLineText(0)
    // The trailing " baz" text node is the last text node.
    const walker = document.createTreeWalker(lineText, NodeFilter.SHOW_TEXT)
    let last: Node | null = null
    let n: Node | null = walker.nextNode()
    while (n !== null) {
      last = n
      n = walker.nextNode()
    }
    const trailing = nn(last, 'trailing text node')
    expect(trailing.nodeValue).toBe(' baz')
    // Offset 2 in " baz": "foo " (4) + "bar" (3) + " b" (2) = 9
    expect(sumOffsetWithin(lineText, trailing, 2)).toBe(9)
  })

  it('multiple marks in a row: sums correctly', () => {
    const { getLineText } = buildLineDom(['<mark>aaa</mark><mark>bbb</mark>ccc'])
    const lineText = getLineText(0)
    const walker = document.createTreeWalker(lineText, NodeFilter.SHOW_TEXT)
    const firstMark = nn(walker.nextNode(), 'first mark text') // "aaa"
    const secondMark = nn(walker.nextNode(), 'second mark text') // "bbb"
    const trailing = nn(walker.nextNode(), 'trailing text') // "ccc"
    expect(sumOffsetWithin(lineText, firstMark, 2)).toBe(2)
    expect(sumOffsetWithin(lineText, secondMark, 0)).toBe(3)
    expect(sumOffsetWithin(lineText, secondMark, 2)).toBe(5)
    expect(sumOffsetWithin(lineText, trailing, 1)).toBe(7)
  })

  it('UTF-16 surrogate pair: each text node treats the emoji as 2 UTF-16 units', () => {
    // "👋hi" - emoji is 2 UTF-16 units in the text node's nodeValue.
    const { getLineText } = buildLineDom(['👋hi'])
    const lineText = getLineText(0)
    const text = nn(lineText.firstChild, 'first child')
    expect(sumOffsetWithin(lineText, text, 2)).toBe(2) // end of emoji, before 'h'
    expect(sumOffsetWithin(lineText, text, 3)).toBe(3) // end of 'h'
  })

  it('returns null when caretNode is outside lineTextRoot', () => {
    const { getLineText, root } = buildLineDom(['hello'])
    const lineText = getLineText(0)
    const stray = document.createElement('div')
    root.appendChild(stray)
    expect(sumOffsetWithin(lineText, stray, 0)).toBeNull()
  })

  it('caret on an element node (selected via boundary index)', () => {
    const { getLineText } = buildLineDom(['a<span>bc</span>d'])
    const lineText = getLineText(0)
    // Caret on lineText with offset 1 means "boundary at child 1" = before the <span>.
    expect(sumOffsetWithin(lineText, lineText, 1)).toBe(1) // "a" before the span
    // Offset 2 = after the span, before "d": "a" + "bc" = 3.
    expect(sumOffsetWithin(lineText, lineText, 2)).toBe(3)
  })
})

describe('caretFromPoint', () => {
  it('returns null when caretPositionFromPoint resolves outside a [data-line]', () => {
    // Make a doc that returns body for any (x, y).
    const fakeDoc = {
      caretPositionFromPoint: () => ({ offsetNode: document.body, offset: 0 }),
    } as unknown as Document
    expect(caretFromPoint(fakeDoc, 0, 0)).toBeNull()
  })

  it('integrates: finds line + offset from a caret inside a nested mark', () => {
    const { root } = buildLineDom(['<mark>foo</mark>bar'])
    const markText = nn(nn(root.querySelector('mark'), 'mark').firstChild, 'mark text')
    const fakeDoc = {
      caretPositionFromPoint: () => ({ offsetNode: markText, offset: 1 }),
    } as unknown as Document
    expect(caretFromPoint(fakeDoc, 0, 0)).toEqual({ line: 0, offset: 1 })
  })

  it('uses caretRangeFromPoint fallback when caretPositionFromPoint is unavailable', () => {
    const { root } = buildLineDom(['hello'])
    const text = nn(nn(root.querySelector('.line-text'), '.line-text').firstChild, 'first child')
    const fakeDoc = {
      caretRangeFromPoint: () => ({ startContainer: text, startOffset: 3 }),
    } as unknown as Document
    expect(caretFromPoint(fakeDoc, 0, 0)).toEqual({ line: 0, offset: 3 })
  })

  it('parses multi-digit line numbers from data-line', () => {
    // Build a few lines so the 42nd one renders with data-line="42".
    const lines: string[] = []
    for (let i = 0; i < 43; i++) lines.push(`line ${String(i)}`)
    const { root } = buildLineDom(lines)
    const lineNode = nn(root.querySelector('[data-line="42"]'), 'line 42 node')
    const lineText = nn(lineNode.querySelector('.line-text'), '.line-text in line 42')
    const line42 = nn(lineText.firstChild, 'first child of line 42 text')
    const fakeDoc = {
      caretPositionFromPoint: () => ({ offsetNode: line42, offset: 2 }),
    } as unknown as Document
    expect(caretFromPoint(fakeDoc, 0, 0)).toEqual({ line: 42, offset: 2 })
  })
})
