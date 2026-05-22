/**
 * Pointer-to-selection helpers for the viewer.
 *
 * Resolves `(clientX, clientY)` to a `{ line, offset }` `LineOffset` in the viewer's
 * logical coordinates (UTF-16 code units inside the line text). Pure functions live
 * here so the math (especially the sibling-offset summation across nested `<mark>`
 * spans from search highlighting) is testable with mocked DOM nodes.
 *
 * Browser-API note: WebKit returns a non-null `caretPositionFromPoint` even on text
 * inside a `user-select: none` ancestor in modern macOS WebKit (verified against the
 * project's minimum target during the M3a spike). Older webviews may need the
 * `caretRangeFromPoint` fallback, which is enabled below.
 */

import type { LineOffset } from './selection.svelte'

/**
 * Resolves a viewport-relative point to a `LineOffset`. Returns `null` when the point
 * lands outside any rendered `[data-line]` (the gutter, the scroll spacer, the status
 * bar). Browser-API support is detected at call time; `caretPositionFromPoint` is
 * preferred, with `caretRangeFromPoint` as a WebKit fallback.
 */
export function caretFromPoint(doc: Document, x: number, y: number): LineOffset | null {
  const caret = resolveCaret(doc, x, y)
  if (!caret) return null
  const lineNode = findLineAncestor(caret.node)
  if (!lineNode) return null
  const lineNumber = parseLineNumber(lineNode)
  if (lineNumber === null) return null

  const lineTextNode = findLineTextNode(lineNode)
  if (!lineTextNode) return null

  // Compute the UTF-16 offset within `.line-text` by summing the text lengths of every
  // sibling span before the caret's text node, plus the caret's offset within its own
  // text node.
  const offset = sumOffsetWithin(lineTextNode, caret.node, caret.offset)
  if (offset === null) return null

  return { line: lineNumber, offset }
}

interface CaretHit {
  node: Node
  offset: number
}

interface CaretCapableDocument {
  caretPositionFromPoint?: (x: number, y: number) => { offsetNode?: Node | null; offset?: number } | null
  caretRangeFromPoint?: (x: number, y: number) => { startContainer: Node; startOffset: number } | null
}

function resolveCaret(doc: Document, x: number, y: number): CaretHit | null {
  const caretDoc = doc as unknown as CaretCapableDocument
  const pos = caretDoc.caretPositionFromPoint?.(x, y)
  if (pos && pos.offsetNode) {
    return { node: pos.offsetNode, offset: pos.offset ?? 0 }
  }
  const range = caretDoc.caretRangeFromPoint?.(x, y)
  if (range) {
    return { node: range.startContainer, offset: range.startOffset }
  }
  return null
}

/**
 * Walks up from `start` to find an ancestor element with a `data-line` attribute.
 * Returns `null` if no ancestor matches (clicked outside the file content).
 */
export function findLineAncestor(start: Node): HTMLElement | null {
  let cur: Node | null = start
  while (cur !== null) {
    if (cur.nodeType === Node.ELEMENT_NODE) {
      const el = cur as HTMLElement
      if (el.hasAttribute('data-line')) return el
    }
    cur = cur.parentNode
  }
  return null
}

/**
 * Returns the `.line-text` descendant of `lineNode`, or `null` if missing (the line is
 * still rendering, or the DOM shape changed).
 */
export function findLineTextNode(lineNode: HTMLElement): HTMLElement | null {
  return lineNode.querySelector('.line-text')
}

function parseLineNumber(lineNode: HTMLElement): number | null {
  const raw = lineNode.getAttribute('data-line')
  if (raw === null) return null
  const n = Number.parseInt(raw, 10)
  if (Number.isNaN(n) || n < 0) return null
  return n
}

/**
 * Computes the UTF-16 offset of `caretNode + caretOffset` within `lineTextRoot`. Walks
 * the text inside `lineTextRoot` in document order, accumulating string lengths until
 * it reaches the caret's text node. If the caret is on an element node, returns the
 * sum up to that element's start. Returns `null` if the caret isn't inside
 * `lineTextRoot`.
 *
 * Exported for unit testing with mocked DOM nodes; the caller (`caretFromPoint`) hides
 * this detail.
 */
export function sumOffsetWithin(lineTextRoot: HTMLElement, caretNode: Node, caretOffset: number): number | null {
  // Two cases:
  //   a) `caretNode` is a Text node: traverse all text nodes inside `lineTextRoot` in
  //      document order, sum lengths up to (but not including) `caretNode`, then add
  //      `caretOffset` (which is a UTF-16 index inside that text node).
  //   b) `caretNode` is an element node: same traversal, but stop at the first text
  //      node inside `caretNode` or right before `caretNode` itself. `caretOffset` is
  //      a child index in this case, not a UTF-16 offset.

  if (!lineTextRoot.contains(caretNode) && caretNode !== lineTextRoot) {
    return null
  }

  if (caretNode.nodeType === Node.TEXT_NODE) {
    let total = 0
    let found = false
    const walker = lineTextRoot.ownerDocument.createTreeWalker(lineTextRoot, NodeFilter.SHOW_TEXT)
    let node = walker.nextNode()
    while (node !== null) {
      if (node === caretNode) {
        total += caretOffset
        found = true
        break
      }
      total += (node.nodeValue ?? '').length
      node = walker.nextNode()
    }
    return found ? total : null
  }

  // Element-node caret: caretOffset is a child index inside caretNode. Walk text nodes
  // up to the child at `caretOffset`, then sum lengths inside the subtrees we cross.
  let total = 0
  const walker = lineTextRoot.ownerDocument.createTreeWalker(lineTextRoot, NodeFilter.SHOW_TEXT)
  let node = walker.nextNode()
  // Build the boundary node: either the child at `caretOffset` or null (after last child).
  const childrenArray = Array.from(caretNode.childNodes)
  const boundary: Node | null = childrenArray[caretOffset] ?? null

  while (node !== null) {
    if (boundary !== null && (node === boundary || (boundary.contains?.(node) ?? false))) {
      break
    }
    total += (node.nodeValue ?? '').length
    node = walker.nextNode()
  }
  return total
}
