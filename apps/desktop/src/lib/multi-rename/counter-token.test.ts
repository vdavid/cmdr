import { describe, it, expect } from 'vitest'
import vectors from '../../../src-tauri/src/multi_rename/counter_token_vectors.json'
import {
  COUNTER_DEFAULTS,
  MAX_COUNTER_DIGITS,
  counterKind,
  counterSamples,
  formatCounter,
  parseCounterParts,
} from './counter-token'

/** `[C10]` → `C10`: the kinds read what sits between the brackets. */
const inner = (token: string): string => token.slice(1, -1)

describe('counter token, against the vectors shared with mask.rs', () => {
  it('uses the same width cap as the backend', () => {
    expect(MAX_COUNTER_DIGITS).toBe(vectors.maxDigits)
  })

  it.each(vectors.valid)('reads $token as the backend does', (vector) => {
    expect(parseCounterParts(inner(vector.token))).toEqual({
      start: vector.start,
      step: vector.step,
      digits: vector.digits,
    })
  })

  it.each(vectors.valid)('rewrites $token as $minimal', (vector) => {
    const counter = counterKind.parse(inner(vector.token))
    expect(counter).not.toBeNull()
    if (counter === null) return
    expect(`[${counterKind.format(counter)}]`).toBe(vector.minimal)
  })

  it.each(vectors.invalid)('rejects %s', (token) => {
    expect(parseCounterParts(inner(token))).toBeNull()
    expect(counterKind.parse(inner(token))).toBeNull()
  })
})

describe('counter token', () => {
  it('only reads a counter', () => {
    expect(parseCounterParts('N')).toBeNull()
    expect(parseCounterParts('')).toBeNull()
    expect(parseCounterParts('c')).toBeNull()
  })

  it('writes the defaults back as a bare [C]', () => {
    expect(formatCounter(COUNTER_DEFAULTS)).toBe('C')
  })

  it('writes each part only when it differs from its default', () => {
    expect(formatCounter({ start: 1, step: 2, digits: 1 })).toBe('C+2')
    expect(formatCounter({ start: 1, step: -1, digits: 1 })).toBe('C-1')
    expect(formatCounter({ start: 5, step: 1, digits: 3 })).toBe('C5:3')
  })

  it('clamps to what the grammar can write', () => {
    // A negative start always writes its step: `[C-3]` would read as a step of -3.
    expect(formatCounter({ start: -3, step: 1, digits: 1 })).toBe('C-3+1')
    expect(formatCounter({ start: -3, step: -2, digits: 1 })).toBe('C-3-2')
    expect(formatCounter({ start: -2000000, step: 1, digits: 1 })).toBe('C-1000000+1')
    expect(formatCounter({ start: 1, step: 1, digits: 999 })).toBe('C:64')
    expect(formatCounter({ start: 1, step: 1, digits: 0 })).toBe('C')
  })

  it('shows the first few numbers, padded as the backend pads them', () => {
    expect(counterSamples({ start: 1, step: 2, digits: 3 })).toEqual(['001', '003', '005'])
    expect(counterSamples({ start: 1, step: -1, digits: 3 })).toEqual(['001', '000', '-01'])
    expect(counterSamples({ start: 10, step: 1, digits: 1 })).toEqual(['10', '11', '12'])
  })
})
