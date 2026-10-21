/**
 * The range editor's pure logic: how a key or a drag moves the characters picked on the ribbon,
 * and the plain-words reading of a range. Which characters a range takes is the engine's answer
 * (`RangeSample`); turning a pick into a token is `range-token.ts`. Pure.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { Range, Selection } from './range-token'

/** Which part of the selection a key moves: all of it (←/→), its start (⌥), or its end (⇧). */
export type SelectionEdge = 'both' | 'start' | 'end'

/** `selection` moved one character by `step`, kept within the text; a move that can't happen leaves it. */
export function moveSelection(selection: Selection, move: { step: 1 | -1; edge: SelectionEdge }): Selection {
  const { first, last, length } = selection
  const { step, edge } = move
  if (edge === 'end') return { first, last: Math.max(first, Math.min(length - 1, last + step)), length }
  if (edge === 'start') return { first: Math.min(last, Math.max(0, first + step)), last, length }
  if (first + step < 0 || last + step > length - 1) return selection
  return { first: first + step, last: last + step, length }
}

/** The characters between two ribbon cells, both included, whichever way the drag ran. */
export function spanBetween(a: number, b: number, length: number): Selection {
  return { first: Math.min(a, b), last: Math.max(a, b), length }
}

/** Where a ⇧-click on cell `index` extends from: the selection's edge farther from it, which stays put. */
export function extendAnchor(selection: Selection, index: number): number {
  return index >= selection.first ? selection.first : selection.last
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

/** What a key on the ribbon does: ← / → move the selection, ⇧ its end, ⌥ its start; `null` for any other key. */
export function ribbonMoveOf(
  e: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'altKey' | 'metaKey' | 'ctrlKey'>,
): { step: 1 | -1; edge: SelectionEdge } | null {
  if (e.metaKey || e.ctrlKey || (e.shiftKey && e.altKey)) return null
  const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : null
  if (step === null) return null
  return { step, edge: e.shiftKey ? 'end' : e.altKey ? 'start' : 'both' }
}
