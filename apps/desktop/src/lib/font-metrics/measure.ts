// The measuring core: code points in, pixel widths out.
//
// Runs inside `measure-worker.ts`, off the main thread. Kept free of Canvas
// construction and of any Tauri or DOM import so it can be unit-tested with a
// stub context.

/** The slice of a 2D context this module needs. Lets tests pass a stub. */
export interface TextMeasurer {
  font: string
  measureText(text: string): { width: number }
}

/** A font ID (`family-weight-size`) taken apart into what Canvas wants. */
export interface FontSpec {
  fontFamily: string
  fontWeight: number
  fontSize: number
}

/** What `'system'` means to Canvas. Matches the UI's own font stack. */
const SYSTEM_FONT_STACK = '-apple-system, BlinkMacSystemFont, system-ui, sans-serif'

/**
 * Parses a font ID of the form `family-weight-size` (for example
 * `system-400-12`) into a Canvas font spec, resolving `'system'` to the real
 * stack. Falls back per-component rather than throwing: a malformed ID should
 * still measure *something* rather than leave every column unsized.
 */
export function parseFontId(fontId: string): FontSpec {
  const parts = fontId.split('-')
  const family = parts[0] || 'system'
  const weight = Number.parseInt(parts[1] || '400', 10)
  const size = Number.parseInt(parts[2] || '12', 10)
  return {
    fontFamily: family === 'system' ? SYSTEM_FONT_STACK : family,
    fontWeight: Number.isFinite(weight) ? weight : 400,
    fontSize: Number.isFinite(size) ? size : 12,
  }
}

/** Builds the Canvas `font` shorthand for a spec. */
export function fontShorthand(spec: FontSpec): string {
  return `${String(spec.fontWeight)} ${String(spec.fontSize)}px ${spec.fontFamily}`
}

/**
 * Measures each code point's advance width, in order.
 *
 * Returns a `Float32Array` parallel to `codePoints`. The caller is responsible
 * for passing only measurable code points (`isMeasurable` in `ranges.ts`);
 * `String.fromCodePoint` throws on a lone surrogate.
 */
export function measureCodePoints(ctx: TextMeasurer, spec: FontSpec, codePoints: Uint32Array): Float32Array {
  ctx.font = fontShorthand(spec)

  const widths = new Float32Array(codePoints.length)
  for (let i = 0; i < codePoints.length; i++) {
    widths[i] = ctx.measureText(String.fromCodePoint(codePoints[i])).width
  }
  return widths
}

/**
 * Same measurement, in time-boxed slices, yielding to the event loop between
 * them.
 *
 * For the main-thread fallback, where running the loop to completion is what
 * froze the UI in the first place. The budget is checked against the clock
 * rather than a character count, so a machine where each `measureText` is 20×
 * slower yields just as promptly.
 *
 * @param yieldToEventLoop  How to give the UI a turn. Injected so a test can
 *   observe the yields without waiting on real timers.
 */
export async function measureCodePointsChunked(
  ctx: TextMeasurer,
  spec: FontSpec,
  codePoints: Uint32Array,
  sliceMs: number,
  yieldToEventLoop: () => Promise<void>,
): Promise<Float32Array> {
  ctx.font = fontShorthand(spec)

  const widths = new Float32Array(codePoints.length)
  let i = 0
  while (i < codePoints.length) {
    const sliceStart = performance.now()
    do {
      widths[i] = ctx.measureText(String.fromCodePoint(codePoints[i])).width
      i++
    } while (i < codePoints.length && performance.now() - sliceStart < sliceMs)

    if (i < codePoints.length) {
      await yieldToEventLoop()
    }
  }
  return widths
}
