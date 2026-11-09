/**
 * Where the rename field's selection goes: the F2 cycle and the extension-dot
 * word stops. Both are pure, so `InlineRenameEditor.svelte` only reads the
 * input's selection and writes the answer back. What an extension is comes from
 * the product's one rule, `#lib/utils/file-extension`, read off the name as
 * typed. `DETAILS.md` § "Selection: the F2 cycle and the dot stops".
 */

import { fileExtension, type NameKind } from '#lib/utils/file-extension.js'

/** A selection in the field, in UTF-16 offsets, as `setSelectionRange` takes it. */
export interface TextRange {
  start: number
  end: number
}

/**
 * The selections pressing `file.rename` again steps through: the name without
 * its extension (where the editor opens), the whole name, then the extension
 * alone. A step that would be empty or repeat the one before is left out, so a
 * name with no extension has the one stage.
 */
export function renameSelectionStages(value: string, kind: NameKind): TextRange[] {
  const ext = fileExtension(value, kind)
  const stemEnd = ext ? value.length - ext.length - 1 : value.length
  const candidates: TextRange[] = [
    { start: 0, end: stemEnd },
    { start: 0, end: value.length },
    { start: ext ? stemEnd + 1 : value.length, end: value.length },
  ]
  const stages: TextRange[] = []
  for (const range of candidates) {
    if (range.start === range.end) continue
    if (stages.some((kept) => kept.start === range.start && kept.end === range.end)) continue
    stages.push(range)
  }
  return stages
}

/**
 * Which end of a stage moves under ⇧← / ⇧⌥←, where a person likely grows it: the name and the
 * whole name at their END (drop the last word), the extension alone at its START (grow into the
 * name). Never 'none': a directionless selection lets macOS move the START on ⇧←, so ⇧⌥← on the
 * pre-selected name would grow it left from 0, which does nothing.
 */
export function stageDirection(range: TextRange): 'forward' | 'backward' {
  return range.start > 0 ? 'backward' : 'forward'
}

/** The stage after the one `current` is, or the first one when it's none of them. */
export function nextRenameSelection(value: string, kind: NameKind, current: TextRange): TextRange {
  const stages = renameSelectionStages(value, kind)
  if (stages.length === 0) return current
  const at = stages.findIndex((range) => range.start === current.start && range.end === current.end)
  return stages[(at + 1) % stages.length]
}

/** The selection a word move reads: the input's own three fields, plus the move. */
export interface WordStopInput {
  value: string
  /** File, folder, or package, which decides where the extension starts. */
  nameKind: NameKind
  selectionStart: number
  selectionEnd: number
  selectionDirection: 'forward' | 'backward' | 'none'
  /** ⌥← is `left`, ⌥→ is `right`. */
  direction: 'left' | 'right'
  /** Shift held: extend the selection instead of moving the caret. */
  extend: boolean
}

/** Where a word move lands instead of the system's, as `setSelectionRange` takes it. */
export interface WordStopResult extends TextRange {
  direction: 'forward' | 'backward' | 'none'
}

/** Text the system's word move crosses in one hop: a word, with any whitespace before it. */
const ONE_WORD_HOP = /^\s*[\p{L}\p{M}\p{N}_]+$/u
const PLAIN_EXTENSION_CHARS = /^[A-Za-z0-9]*$/

/**
 * The extra word stop at the extension's dot, or `null` to leave the move to
 * the system.
 *
 * The system treats `report.pdf` as one word, so a word move skips the dot. This
 * answers ONLY where the system's move would certainly cross it, and lands on
 * the dot's near side instead: the extension's start going left, the name's end
 * going right. Anywhere a native boundary could come first (a space, a hyphen,
 * a compound extension's inner dot) it answers `null`, so the system's own
 * word logic, undo, and IME stay untouched.
 */
export function wordStopSelection(input: WordStopInput): WordStopResult | null {
  const { value, selectionStart, selectionEnd, selectionDirection, direction, extend } = input
  const ext = fileExtension(value, input.nameKind)
  if (!ext) return null
  // A plain move with a selection collapses it first, which is the system's call.
  if (!extend && selectionStart !== selectionEnd) return null

  const backward = selectionDirection === 'backward'
  const focus = backward ? selectionStart : selectionEnd
  const anchor = backward ? selectionEnd : selectionStart
  const extStart = value.length - ext.length
  const stemEnd = extStart - 1

  let target: number | null = null
  if (direction === 'left' && focus > extStart && PLAIN_EXTENSION_CHARS.test(value.slice(extStart, focus))) {
    target = extStart
  } else if (direction === 'right' && focus < stemEnd && ONE_WORD_HOP.test(value.slice(focus, stemEnd))) {
    target = stemEnd
  }
  if (target === null) return null

  if (!extend) return { start: target, end: target, direction: 'none' }
  return target < anchor
    ? { start: target, end: anchor, direction: 'backward' }
    : { start: anchor, end: target, direction: 'forward' }
}
