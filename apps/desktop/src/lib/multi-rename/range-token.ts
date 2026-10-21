/**
 * The range placeholders: `[N…]` `[E…]` `[P…]` `[G…]` and the full-name `[2-5]`, with `[N1]` one
 * character, `[N2-5]` a span, `[N2,5]` a start and length, `[N2-]` to the end, and negative
 * positions counting from the end (`[N-8-5]` is the 8th-to-last to the 5th-to-last). Reads exactly
 * what `range()` in `src-tauri/src/multi_rename/mask.rs` reads; both are tested against
 * `range_token_vectors.json`.
 *
 * What a range TAKES from a name is the engine's answer (`getMultiRenameRangeSamples`), never
 * computed here. This module only reads and writes the token's text, and turns the characters the
 * user picked in the range editor (on a text of known length) into the token that takes them. Pure.
 */
import type { MaskTokenGrammar } from './mask-tokens'

/** The text a range reads, as `RowFacts` in `mask.rs` names it. */
export type RangeField = 'name' | 'extension' | 'parent' | 'grandparent' | 'fullName'

/** The fields in the editor's order. */
export const RANGE_FIELDS: readonly RangeField[] = ['name', 'extension', 'parent', 'grandparent', 'fullName']

/** Positions count from 1; a negative one counts from the end (-1 is the last character). */
export type Range =
  | { kind: 'all' }
  | { kind: 'one'; at: number }
  | { kind: 'from'; at: number }
  | { kind: 'length'; at: number; length: number }
  | { kind: 'span'; at: number; to: number }

export interface RangeToken {
  field: RangeField
  range: Range
}

/** How the editor writes a range: start to end (`[N2-5]`), or start and length (`[N2,4]`). */
export type RangeForm = 'span' | 'length'

/** Which edge each end of the range counts from. */
export interface Anchors {
  startFromEnd: boolean
  endFromEnd: boolean
}

/** Characters `first`..`last` (from 0, both included) of a text `length` characters long. */
export interface Selection {
  first: number
  last: number
  length: number
}

const LETTERS: Record<RangeField, string> = {
  name: 'N',
  extension: 'E',
  parent: 'P',
  grandparent: 'G',
  fullName: '',
}

const FIELD_OF_LETTER: Partial<Record<string, RangeField>> = {
  N: 'name',
  E: 'extension',
  P: 'parent',
  G: 'grandparent',
}

const I64_MIN = -(2n ** 63n)
const I64_MAX = 2n ** 63n - 1n
const U64_MAX = 2n ** 64n - 1n

/** Rust's `i64::from_str`: an optional sign, then digits. */
function parseI64(text: string): number | null {
  if (!/^[+-]?\d+$/.test(text)) return null
  const value = BigInt(text)
  return value < I64_MIN || value > I64_MAX ? null : Number(value)
}

/** Rust's `usize::from_str` (64-bit): an optional `+`, then digits. */
function parseUsize(text: string): number | null {
  if (!/^\+?\d+$/.test(text)) return null
  const value = BigInt(text)
  return value > U64_MAX ? null : Number(value)
}

/** `range()` in `mask.rs`, step for step. */
function parseRange(text: string): Range | null {
  if (text === '') return { kind: 'all' }
  const comma = text.indexOf(',')
  if (comma !== -1) {
    const at = parseI64(text.slice(0, comma))
    const length = parseUsize(text.slice(comma + 1))
    return at === null || length === null ? null : { kind: 'length', at, length }
  }
  if (text.endsWith('-')) {
    const at = parseI64(text.slice(0, -1))
    if (at !== null) return { kind: 'from', at }
  }
  // The separating `-` is the first one after the start's own sign.
  const dash = text.indexOf('-', text.startsWith('-') ? 1 : 0)
  if (dash !== -1) {
    const at = parseI64(text.slice(0, dash))
    const to = parseI64(text.slice(dash + 1))
    return at === null || to === null ? null : { kind: 'span', at, to }
  }
  const at = parseI64(text)
  return at === null ? null : { kind: 'one', at }
}

/** The range `[<inner>]` spells, or `null` when it's another placeholder or none. Mirrors `placeholder()` in `mask.rs`. */
export function parseRangeToken(inner: string): RangeToken | null {
  const letterField = FIELD_OF_LETTER[inner.charAt(0)]
  if (letterField !== undefined) {
    const range = parseRange(inner.slice(1))
    return range === null ? null : { field: letterField, range }
  }
  if (/^[0-9-]/.test(inner)) {
    const range = parseRange(inner)
    return range === null ? null : { field: 'fullName', range }
  }
  return null
}

/**
 * The token's text without brackets. A whole field is its bare letter (`N`; `N1-` reads the same);
 * the whole full name is `1-`, since an empty `[]` is no placeholder.
 */
export function formatRangeToken({ field, range }: RangeToken): string {
  const letter = LETTERS[field]
  const whole = field === 'fullName' ? '1-' : ''
  switch (range.kind) {
    case 'all':
      return letter + whole
    case 'from':
      return letter + (range.at === 1 ? whole : `${String(range.at)}-`)
    case 'one':
      return letter + String(range.at)
    case 'length':
      return `${letter}${String(range.at)},${String(range.length)}`
    case 'span':
      return `${letter}${String(range.at)}-${String(range.to)}`
  }
}

export const rangeKind: MaskTokenGrammar<RangeToken> = {
  id: 'range',
  parse: parseRangeToken,
  format: formatRangeToken,
}

/**
 * The range that takes exactly `selection`, counting each end from the edge `anchors` say. A
 * start counted from the end takes the end with it: that's how the grammar reads `[N-8-5]`.
 * The whole text is the bare field (`all`).
 */
export function rangeForSelection(args: { selection: Selection; anchors: Anchors; form: RangeForm }): Range {
  const { first, last, length } = args.selection
  const startFromEnd = args.anchors.startFromEnd
  const endFromEnd = startFromEnd || args.anchors.endFromEnd
  const at = startFromEnd ? -(length - first) : first + 1
  if (args.form === 'length') return { kind: 'length', at, length: last - first + 1 }
  if (first === last && startFromEnd === endFromEnd) return { kind: 'one', at }
  if (endFromEnd && last === length - 1) return first === 0 && !startFromEnd ? { kind: 'all' } : { kind: 'from', at }
  if (startFromEnd) return { kind: 'span', at, to: length - last }
  return { kind: 'span', at, to: endFromEnd ? -(length - last) : last + 1 }
}

/**
 * Each end counts from the edge it's closer to (a tie goes to the start), and an end on the last
 * character counts from the end, so the range runs to the end of every name.
 */
export function autoAnchors({ first, last, length }: Selection): Anchors {
  const startFromEnd = length - 1 - first < first
  return { startFromEnd, endFromEnd: startFromEnd || length - 1 - last < last || last === length - 1 }
}

/** Which edges a range's ends count from, and its form. A length form's end follows its start. */
export function anchorsOf(range: Range): Anchors & { form: RangeForm } {
  switch (range.kind) {
    case 'all':
      return { startFromEnd: false, endFromEnd: true, form: 'span' }
    case 'one':
      return { startFromEnd: range.at < 0, endFromEnd: range.at < 0, form: 'span' }
    case 'from':
      return { startFromEnd: range.at < 0, endFromEnd: true, form: 'span' }
    case 'span':
      return { startFromEnd: range.at < 0, endFromEnd: range.at < 0 || range.to < 0, form: 'span' }
    case 'length':
      return { startFromEnd: range.at < 0, endFromEnd: range.at < 0, form: 'length' }
  }
}
