/**
 * The counter placeholder, `[C]` with an optional start, signed step, and width: `[C10+5:3]`, `[C10]`,
 * `[C+5]`, `[C:3]`, `[C100-10]`. Reads exactly what `counter()` in `src-tauri/src/multi_rename/mask.rs`
 * reads; both are tested against `counter_token_vectors.json`. Pure.
 */
import type { MaskTokenGrammar } from './mask-tokens'

/** The parts a token spells out; `null` where it leaves one to its default. */
export interface CounterParts {
  start: number | null
  step: number | null
  digits: number | null
}

export interface Counter {
  start: number
  step: number
  digits: number
}

/** What a bare `[C]` counts: 1, 2, 3, unpadded. */
export const COUNTER_DEFAULTS: Counter = { start: 1, step: 1, digits: 1 }

/** The widest counter, `MAX_COUNTER_DIGITS` in `mask.rs`. */
export const MAX_COUNTER_DIGITS = 64

const I64_MIN = -(2n ** 63n)
const I64_MAX = 2n ** 63n - 1n
const U32_MAX = 2n ** 32n - 1n

/** Rust's `i64::from_str`: an optional sign, then digits. */
function parseI64(text: string): number | null {
  if (!/^[+-]?\d+$/.test(text)) return null
  const value = BigInt(text)
  return value < I64_MIN || value > I64_MAX ? null : Number(value)
}

/** Rust's `u32::from_str`: an optional `+`, then digits. */
function parseU32(text: string): number | null {
  if (!/^\+?\d+$/.test(text)) return null
  const value = BigInt(text)
  return value > U32_MAX ? null : Number(value)
}

/** What `[<inner>]` spells out, or `null` when it isn't a counter. Mirrors `mask.rs` step for step. */
export function parseCounterParts(inner: string): CounterParts | null {
  if (!inner.startsWith('C')) return null
  const text = inner.slice(1)
  const colon = text.indexOf(':')
  const body = colon === -1 ? text : text.slice(0, colon)
  let digits: number | null = null
  if (colon !== -1) {
    digits = parseU32(text.slice(colon + 1))
    if (digits === null) return null
  }
  // The step starts at a leading sign, else at the first sign after the start.
  const signAfterStart = body.slice(1).search(/[+-]/)
  const stepAt = /^[+-]/.test(body) ? 0 : signAfterStart === -1 ? -1 : signAfterStart + 1
  let step: number | null = null
  if (stepAt !== -1) {
    step = parseI64(body.slice(stepAt).replace(/^\++/, ''))
    if (step === null) return null
  }
  const startText = stepAt === -1 ? body : body.slice(0, stepAt)
  const start = startText === '' ? null : parseI64(startText)
  if (startText !== '' && start === null) return null
  return { start, step, digits }
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Math.trunc(value)))
}

/** `counter` within what a token can write: a start can't carry a sign (`[C-5]` is a step). */
export function clampCounter(counter: Counter): Counter {
  return {
    start: clamp(counter.start, 0, Number.MAX_SAFE_INTEGER),
    step: clamp(counter.step, -Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER),
    digits: clamp(counter.digits, 1, MAX_COUNTER_DIGITS),
  }
}

/** The token's text without brackets, defaults left out: `C10+5:3`, or a bare `C`. */
export function formatCounter(counter: Counter): string {
  const { start, step, digits } = clampCounter(counter)
  const startText = start === COUNTER_DEFAULTS.start ? '' : String(start)
  const stepText = step === COUNTER_DEFAULTS.step ? '' : step < 0 ? String(step) : `+${String(step)}`
  const digitsText = digits === COUNTER_DEFAULTS.digits ? '' : `:${String(digits)}`
  return `C${startText}${stepText}${digitsText}`
}

export const counterKind: MaskTokenGrammar<Counter> = {
  id: 'counter',
  parse(inner) {
    const parts = parseCounterParts(inner)
    if (parts === null) return null
    return clampCounter({
      start: parts.start ?? COUNTER_DEFAULTS.start,
      step: parts.step ?? COUNTER_DEFAULTS.step,
      digits: parts.digits ?? COUNTER_DEFAULTS.digits,
    })
  },
  format: formatCounter,
}

/** `pad` in `mask.rs`: zeros to `digits` wide, a minus sign counting toward the width. */
function pad(value: number, digits: number): string {
  const width = clamp(digits, 1, MAX_COUNTER_DIGITS)
  if (value < 0) return `-${String(-value).padStart(Math.max(width - 1, 1), '0')}`
  return String(value).padStart(width, '0')
}

/** The first three numbers the counter gives, plain and as they land in the names. */
export function counterSamples(counter: Counter): { values: string[]; shown: string[] } {
  const numbers = [0, 1, 2].map((i) => counter.start + counter.step * i)
  return { values: numbers.map(String), shown: numbers.map((n) => pad(n, counter.digits)) }
}
