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
      if (seg.isWordLike) return { start, end }
      // Caret on a non-word segment (whitespace, punctuation). Prefer the closest
      // adjacent word: previous if any, otherwise the next word in the line.
      if (lastWord !== null) return lastWord
      for (let j = i + 1; j < segments.length; j++) {
        const next = segments[j]
        if (next.isWordLike) {
          return { start: next.index, end: next.index + next.segment.length }
        }
      }
      return { start: clamped, end: clamped }
    }

    if (seg.isWordLike) lastWord = { start, end }
  }

  // Caret past the end: return the last word, or zero-length.
  if (lastWord !== null) return lastWord
  return { start: clamped, end: clamped }
}
