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
  // The step starts at the first sign after the first character. A leading `+` is always the
  // step's; a leading `-` is the start's sign when another sign follows (`[C-5+1]`), else the step's.
  const signAfterFirst = body.slice(1).search(/[+-]/)
  const stepAfterFirst = signAfterFirst === -1 ? -1 : signAfterFirst + 1
  const stepAt = body.startsWith('+') ? 0 : body.startsWith('-') && stepAfterFirst === -1 ? 0 : stepAfterFirst
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

/** The lowest start the editor writes. */
export const MIN_COUNTER_START = -1_000_000

/** `counter` within what the editor writes. */
export function clampCounter(counter: Counter): Counter {
  return {
    start: clamp(counter.start, MIN_COUNTER_START, Number.MAX_SAFE_INTEGER),
    step: clamp(counter.step, -Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER),
    digits: clamp(counter.digits, 1, MAX_COUNTER_DIGITS),
  }
}

/**
 * The token's text without brackets, defaults left out: `C10+5:3`, or a bare `C`. A negative start
 * always writes its step (`C-5+1`): a lone `C-5` reads as a step of -5.
 */
export function formatCounter(counter: Counter): string {
  const { start, step, digits } = clampCounter(counter)
  const startText = start === COUNTER_DEFAULTS.start ? '' : String(start)
  const stepText = step === COUNTER_DEFAULTS.step && start >= 0 ? '' : step < 0 ? String(step) : `+${String(step)}`
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

/** The first three numbers the counter gives, as they land in the names. */
export function counterSamples(counter: Counter): string[] {
  return [0, 1, 2].map((i) => pad(counter.start + counter.step * i, counter.digits))
}
