import { describe, it, expect } from 'vitest'
import vectors from '../../../src-tauri/src/multi_rename/range_token_vectors.json'
import {
  anchorsOf,
  autoAnchors,
  rangeForSelection,
  rangeKind,
  type RangeField,
  type RangeForm,
  type RangeToken,
} from './range-token'

/** `[N2-5]` → `N2-5`: the kinds read what sits between the brackets. */
const inner = (token: string): string => token.slice(1, -1)

/** A vector's numbers as the frontend's value: `null` where the kind has none. */
function expected(vector: (typeof vectors.valid)[number]): RangeToken {
  const field = vector.field as RangeField
  switch (vector.kind) {
    case 'all':
      return { field, range: { kind: 'all' } }
    case 'one':
      return { field, range: { kind: 'one', at: vector.at ?? NaN } }
    case 'from':
      return { field, range: { kind: 'from', at: vector.at ?? NaN } }
    case 'length':
      return { field, range: { kind: 'length', at: vector.at ?? NaN, length: vector.length ?? NaN } }
    default:
      return { field, range: { kind: 'span', at: vector.at ?? NaN, to: vector.to ?? NaN } }
  }
}

describe('range token, against the vectors shared with mask.rs', () => {
  it.each(vectors.valid)('reads $token as the backend does', (vector) => {
    expect(rangeKind.parse(inner(vector.token))).toEqual(expected(vector))
  })

  it.each(vectors.valid)('rewrites $token as $minimal', (vector) => {
    const token = rangeKind.parse(inner(vector.token))
    expect(token).not.toBeNull()
    if (token === null) return
    expect(`[${rangeKind.format(token)}]`).toBe(vector.minimal)
  })

  it.each(vectors.invalid)('rejects %s', (token) => {
    expect(rangeKind.parse(inner(token))).toBeNull()
  })

  it.each(vectors.notRanges)('leaves %s to its own kind', (token) => {
    expect(rangeKind.parse(inner(token))).toBeNull()
  })

  it.each(vectors.selections)('writes $token for $first..$last of $length', (vector) => {
    const range = rangeForSelection({
      selection: { first: vector.first, last: vector.last, length: vector.length },
      anchors: { startFromEnd: vector.startFromEnd, endFromEnd: vector.endFromEnd },
      form: vector.form as RangeForm,
    })
    expect(`[${rangeKind.format({ field: vector.field as RangeField, range })}]`).toBe(vector.token)
  })
})

describe('range token', () => {
  it('writes a whole field as its bare letter, and the whole full name as 1-', () => {
    expect(rangeKind.format({ field: 'name', range: { kind: 'all' } })).toBe('N')
    expect(rangeKind.format({ field: 'grandparent', range: { kind: 'from', at: 1 } })).toBe('G')
    expect(rangeKind.format({ field: 'fullName', range: { kind: 'all' } })).toBe('1-')
  })

  it('reads numbers past what an i64 holds as no range', () => {
    expect(rangeKind.parse('N9223372036854775808')).toBeNull()
    expect(rangeKind.parse('N-9223372036854775809')).toBeNull()
  })
})

describe('anchors', () => {
  it('counts each end from the edge it is closer to', () => {
    // "Beach day": the first six; their end is nearer the end, so it's the 4th-to-last.
    expect(autoAnchors({ first: 0, last: 5, length: 9 })).toEqual({ startFromEnd: false, endFromEnd: true })
    expect(autoAnchors({ first: 1, last: 2, length: 9 })).toEqual({ startFromEnd: false, endFromEnd: false })
    expect(autoAnchors({ first: 6, last: 7, length: 9 })).toEqual({ startFromEnd: true, endFromEnd: true })
  })

  it('counts an end touching the last character from the end, so the range runs to the end', () => {
    expect(autoAnchors({ first: 1, last: 8, length: 9 })).toEqual({ startFromEnd: false, endFromEnd: true })
  })

  it('breaks a tie toward the start', () => {
    expect(autoAnchors({ first: 4, last: 4, length: 9 })).toEqual({ startFromEnd: false, endFromEnd: false })
  })

  it('reads them back from a token', () => {
    expect(anchorsOf({ kind: 'all' })).toEqual({ startFromEnd: false, endFromEnd: true, form: 'span' })
    expect(anchorsOf({ kind: 'one', at: -2 })).toEqual({ startFromEnd: true, endFromEnd: true, form: 'span' })
    expect(anchorsOf({ kind: 'one', at: 2 })).toEqual({ startFromEnd: false, endFromEnd: false, form: 'span' })
    expect(anchorsOf({ kind: 'from', at: 3 })).toEqual({ startFromEnd: false, endFromEnd: true, form: 'span' })
    expect(anchorsOf({ kind: 'span', at: 1, to: -5 })).toEqual({ startFromEnd: false, endFromEnd: true, form: 'span' })
    // A start from the end takes the end with it, as `[N-8-5]` reads.
    expect(anchorsOf({ kind: 'span', at: -8, to: 5 })).toEqual({ startFromEnd: true, endFromEnd: true, form: 'span' })
    expect(anchorsOf({ kind: 'length', at: -3, length: 3 })).toEqual({
      startFromEnd: true,
      endFromEnd: true,
      form: 'length',
    })
  })

  it('round-trips: a token written for a selection reads back the anchors it was written with', () => {
    const selection = { first: 0, last: 4, length: 9 }
    const anchors = { startFromEnd: false, endFromEnd: true }
    expect(anchorsOf(rangeForSelection({ selection, anchors, form: 'span' }))).toEqual({ ...anchors, form: 'span' })
  })
})
