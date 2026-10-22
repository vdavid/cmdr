/**
 * The range editor's pure logic: the text box's selection read as characters (and back), the token
 * a fresh selection writes, and the plain-words reading of a range. Which characters a range takes
 * is the engine's answer (`RangeSample`); turning a pick into a token is `range-token.ts`. Pure.
 *
 * A text field counts its selection in UTF-16 units; the engine counts characters (what
 * `Array.from` splits), so a name with an emoji needs the two told apart.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { autoAnchors, rangeForSelection, type Anchors, type Range, type RangeForm, type Selection } from './range-token'

/** How many characters the first `units` UTF-16 units of `text` hold. */
function charsIn(text: string, units: number): number {
  return Array.from(text.slice(0, units)).length
}

/**
 * The characters a text field's selection (`selectionStart` / `selectionEnd`, UTF-16 units) covers,
 * or `null` for a bare caret, which picks nothing.
 */
export function selectionInText(text: string, start: number, end: number): Selection | null {
  const first = charsIn(text, Math.min(start, end))
  const past = charsIn(text, Math.max(start, end))
  if (past <= first) return null
  return { first, last: past - 1, length: Array.from(text).length }
}

/** The text field's selection, in UTF-16 units, that covers `selection`'s characters. */
export function textOffsetsOf(text: string, selection: Selection): { start: number; end: number } {
  const chars = Array.from(text)
  return {
    start: chars.slice(0, selection.first).join('').length,
    end: chars.slice(0, selection.last + 1).join('').length,
  }
}

/**
 * What a selection in the text box picks, and the range that takes it, in the token's current form.
 * A fresh pick (no `keep`) counts each end from the edge it's nearer (`autoAnchors`); `keep` holds
 * the token's own edges, for a selection the keys only adjusted. `null` for a bare caret, which
 * leaves the token.
 */
export function textPick(args: {
  text: string
  start: number
  end: number
  form: RangeForm
  keep?: Anchors
}): { selection: Selection; range: Range } | null {
  const selection = selectionInText(args.text, args.start, args.end)
  if (!selection) return null
  const anchors = args.keep ?? autoAnchors(selection)
  return { selection, range: rangeForSelection({ selection, anchors, form: args.form }) }
}

/** A range in plain words: a message key and its numbers (positions as typed, from-the-end ones made positive). */
export function rangeReading(range: Range): { key: MessageKey; params: Record<string, number> } {
  switch (range.kind) {
    case 'all':
      return { key: 'multiRename.rangeEditor.reading.whole', params: {} }
    case 'one':
      return range.at < 0
        ? { key: 'multiRename.rangeEditor.reading.oneFromEnd', params: { at: -range.at } }
        : { key: 'multiRename.rangeEditor.reading.one', params: { at: range.at } }
    case 'from':
      if (range.at === 1) return { key: 'multiRename.rangeEditor.reading.whole', params: {} }
      return range.at < 0
        ? { key: 'multiRename.rangeEditor.reading.lastChars', params: { count: -range.at } }
        : { key: 'multiRename.rangeEditor.reading.toEnd', params: { at: range.at } }
    case 'length':
      return range.at < 0
        ? { key: 'multiRename.rangeEditor.reading.lengthFromEnd', params: { count: range.length, at: -range.at } }
        : { key: 'multiRename.rangeEditor.reading.length', params: { count: range.length, at: range.at } }
    case 'span':
      // A negative start counts a positive end from the end too (`[N-8-5]`).
      if (range.at < 0) {
        return {
          key: 'multiRename.rangeEditor.reading.spanFromEnd',
          params: { from: -range.at, to: Math.abs(range.to) },
        }
      }
      return range.to < 0
        ? { key: 'multiRename.rangeEditor.reading.spanToFromEnd', params: { from: range.at, to: -range.to } }
        : { key: 'multiRename.rangeEditor.reading.span', params: { from: range.at, to: range.to } }
  }
}
