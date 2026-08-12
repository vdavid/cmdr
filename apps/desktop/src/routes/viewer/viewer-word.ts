/**
 * Word-boundary helper for the viewer's double-click selection.
 *
 * Word boundaries follow Unicode word-segmentation rules via `Intl.Segmenter`, available
 * in Safari 14.1+ (we target macOS Big Sur and up, so it's safe without a polyfill).
 *
 * Triple-click (whole-line selection) doesn't need its own helper: the caller wraps the
 * line's UTF-16 length into a `{ start: { line, offset: 0 }, end: { line, offset: len } }`
 * selection directly.
 */

/** Any letter or digit. A segment holding one is a word, whatever the engine claims. */
const WORD_CHAR = /[\p{L}\p{N}]/u

/**
 * Whether a segment counts as a word.
 *
 * Gotcha/Why: `Intl.Segmenter` splits at the right places everywhere, but its `isWordLike`
 * flag is wrong in JavaScriptCore, the app's engine. JSC reports `false` for every segment
 * ICU classifies as numeric, which is any word ENDING in a digit: `123`, `3.14`, `v2`,
 * `sha256`, `abc123`. Trusting it made a double-click on `"1292507278647433"` select the
 * key before it. So we decide word-ness from the segment's own characters and only fall
 * back to the flag (it can add a word, never remove one). (Verified on macOS 26.5.2
 * WKWebView vs. Node 24 and Playwright's WebKit, offscreen WKWebView probe, 2026-08-13.)
 */
function isWordSegment(seg: Intl.SegmentData): boolean {
  return seg.isWordLike === true || WORD_CHAR.test(seg.segment)
}

/**
 * Returns the `[start, end)` UTF-16 bounds of the word containing `offset` in
 * `lineText`. If `offset` doesn't fall on a word segment (it's at a separator like
 * whitespace or punctuation), returns the adjacent word; if there's no word, returns
 * a zero-length range at `offset`.
 *
 * Locale is the user's runtime default; for plain-text logs the locale rarely matters
 * for word boundaries, but using the default keeps the behavior consistent with
 * `Intl.Segmenter`'s contract.
 */
export function findWordBoundsAt(lineText: string, offset: number): { start: number; end: number } {
  if (lineText.length === 0) return { start: 0, end: 0 }
  const clamped = Math.max(0, Math.min(offset, lineText.length))
  const segmenter = new Intl.Segmenter(undefined, { granularity: 'word' })

  let lastWord: { start: number; end: number } | null = null
  const segments = Array.from(segmenter.segment(lineText))

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i]
    const start = seg.index
    const end = seg.index + seg.segment.length

    if (start <= clamped && clamped < end) {
      if (isWordSegment(seg)) return { start, end }
      // Caret on a non-word segment (whitespace, punctuation). Prefer the closest
      // adjacent word: previous if any, otherwise the next word in the line.
      if (lastWord !== null) return lastWord
      for (let j = i + 1; j < segments.length; j++) {
        const next = segments[j]
        if (isWordSegment(next)) {
          return { start: next.index, end: next.index + next.segment.length }
        }
      }
      return { start: clamped, end: clamped }
    }

    if (isWordSegment(seg)) lastWord = { start, end }
  }

  // Caret past the end: return the last word, or zero-length.
  if (lastWord !== null) return lastWord
  return { start: clamped, end: clamped }
}
