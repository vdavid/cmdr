/**
 * The fixed log color scale for per-country counts (the world map fill and the country table's count
 * chips). Zero is a hueless neutral; from one up, the hue walks red → orange → yellow → green → cyan →
 * blue → purple, evenly in log10 space, and stops at `COUNT_SCALE_PURPLE_AT`.
 *
 * ❌ Never auto-scale this to the data: the point is that a color always means the same count, so David
 * learns "purple = 1,000" once and reads every map at a glance.
 */

/**
 * The count that reaches purple, the top of the scale. Everything above clamps to purple.
 *
 * Hand-set on purpose, and David revisits it by hand: when real counts outgrow it, the section shows a
 * yellow callout asking for the next power of ten. Changing it recolors every count, so do it in one step.
 */
export const COUNT_SCALE_PURPLE_AT = 1000

/** Zero: a mid-dark gray with no hue, so "nobody" never passes for a small count. */
export const COUNT_ZERO_COLOR = '#4a4946'

/**
 * The hue anchors, evenly spaced in log space from 1 (red) to `COUNT_SCALE_PURPLE_AT` (purple), in OKLCH.
 * Lightness and chroma vary per anchor so each hue reads as its name (a yellow has to be light to be
 * yellow, a red darker to not turn salmon), and every one clears 4.5:1 on the dark surface, so the colors
 * can carry text. The tests pin that contrast.
 */
const anchors = [
  { name: 'red', l: 0.66, c: 0.21, hue: 24 },
  { name: 'orange', l: 0.79, c: 0.16, hue: 64 },
  { name: 'yellow', l: 0.88, c: 0.18, hue: 98 },
  { name: 'green', l: 0.77, c: 0.19, hue: 145 },
  { name: 'cyan', l: 0.8, c: 0.13, hue: 205 },
  { name: 'blue', l: 0.7, c: 0.16, hue: 258 },
  { name: 'purple', l: 0.68, c: 0.21, hue: 312 },
] as const

/** Where `value` sits on the scale, 0 (one) to 1 (the purple end), clamped. */
function position(value: number): number {
  if (value <= 1) return 0
  return Math.min(1, Math.log10(value) / Math.log10(COUNT_SCALE_PURPLE_AT))
}

/** The nearest named hue for a count of at least one, for tests and accessible labels. */
export function countHue(value: number): (typeof anchors)[number]['name'] {
  return anchors[Math.round(position(value) * (anchors.length - 1))].name
}

/** The color for a count: `COUNT_ZERO_COLOR` for zero (or less), else its place on the hue walk. */
export function countColor(value: number): string {
  if (value <= 0) return COUNT_ZERO_COLOR
  const scaled = position(value) * (anchors.length - 1)
  const i = Math.min(Math.floor(scaled), anchors.length - 2)
  const [a, b] = [anchors[i], anchors[i + 1]]
  const t = scaled - i
  const mix = (x: number, y: number) => x + (y - x) * t
  return oklchToHex(mix(a.l, b.l), mix(a.c, b.c), mix(a.hue, b.hue))
}

/** The legend's stops: zero, then one count per hue anchor, rounded to whole numbers. */
export function countScaleStops(): { value: number; color: string }[] {
  const steps = anchors.length - 1
  return [
    { value: 0, color: COUNT_ZERO_COLOR },
    ...anchors.map((_, k) => {
      const value = Math.round(COUNT_SCALE_PURPLE_AT ** (k / steps))
      return { value, color: countColor(value) }
    }),
  ]
}

/** Whether any count passes the purple end, which is when the scale needs raising. */
export function exceedsCountScale(values: Iterable<number | null>): boolean {
  for (const v of values) if (v !== null && v > COUNT_SCALE_PURPLE_AT) return true
  return false
}

/** OKLCH → sRGB hex, lowering chroma until the color fits the sRGB gamut. */
function oklchToHex(l: number, c: number, hueDeg: number): string {
  const h = (hueDeg * Math.PI) / 180
  for (let cc = c; cc >= 0; cc -= 0.005) {
    const rgb = oklabToLinearSrgb(l, cc * Math.cos(h), cc * Math.sin(h))
    if (rgb.every((v) => v >= -1e-4 && v <= 1 + 1e-4)) return toHex(rgb)
  }
  return toHex(oklabToLinearSrgb(l, 0, 0))
}

function oklabToLinearSrgb(l: number, a: number, b: number): [number, number, number] {
  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3
  return [
    4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
    -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
    -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
  ]
}

function toHex(linear: [number, number, number]): string {
  return `#${linear
    .map((v) => {
      const x = Math.min(1, Math.max(0, v))
      const srgb = x <= 0.0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - 0.055
      return Math.round(srgb * 255)
        .toString(16)
        .padStart(2, '0')
    })
    .join('')}`
}
