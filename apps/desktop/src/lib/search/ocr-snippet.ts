// Pure parser for the OCR "why matched" snippet.
//
// The backend's `media_index_search_ocr` returns each hit's `snippet` with the matched
// terms wrapped in `[` / `]` markers (fts5's `snippet()` delimiters). The grid renders
// the matched runs highlighted — but via structured segments + a `<mark>` element, NEVER
// by injecting the raw string as HTML (no `{@html}`), so a document whose OCR text
// happens to contain markup can't inject anything.

/** One run of the snippet: `matched` runs render highlighted, the rest as plain text. */
export interface OcrSnippetSegment {
  text: string
  matched: boolean
}

/**
 * Split an OCR snippet into plain and matched (`[...]`) segments. Adjacent runs of the
 * same kind collapse into one segment. An unbalanced trailing `[` (no closing `]`) keeps
 * its text visible rather than dropping it. Returns a single plain segment for a snippet
 * with no markers, and an empty array for an empty string.
 */
export function parseOcrSnippet(snippet: string): OcrSnippetSegment[] {
  const segments: OcrSnippetSegment[] = []
  let buffer = ''
  let matched = false

  const flush = (): void => {
    if (buffer !== '') {
      segments.push({ text: buffer, matched })
      buffer = ''
    }
  }

  for (const ch of snippet) {
    if (!matched && ch === '[') {
      flush()
      matched = true
      continue
    }
    if (matched && ch === ']') {
      flush()
      matched = false
      continue
    }
    buffer += ch
  }
  flush()

  return segments
}
