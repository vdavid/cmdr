/**
 * Client-safe: one fixed color per country, so a country reads the same in every chart, table dot, and map.
 *
 * The eight were assigned once, to the top eight getcmdr.com countries by 90-day visitors (Umami
 * `metrics?type=country`, 2026-11-10: US, DE, NL, HU, SG, GB, CZ, PL). Color follows the country, never its
 * rank: when the ranking shifts, the map stays as it is, and a country outside it is "Other" grey. Re-assign
 * deliberately (and re-validate) if the top eight drift far from this list.
 *
 * Validated with the dataviz skill's `validate_palette.js --mode dark --surface #1a1917` in this order plus the
 * "Other" grey last (the stack order of the country charts): all in the dark lightness band, worst adjacent CVD
 * ΔE 11.9, worst adjacent normal-vision ΔE 16.8, all ≥ 3:1 on the surface. Each sits ≥ 15 ΔE from gold,
 * Homebrew blue, and grey (the colors they share the new-installs chart with); the one near pair to an
 * existing property is PL's mint vs veszelovszki.com's olive (ΔE 9), which never share a chart.
 */
import { COLOR_UNKNOWN } from './colors.js'

export const countryColors = {
  US: '#1b7e2a',
  DE: '#df5e97',
  NL: '#9a6a17',
  HU: '#824eb4',
  SG: '#018d87',
  GB: '#de7515',
  CZ: '#ba343c',
  PL: '#39ad79',
} as const satisfies Record<string, string>

/** Every country without its own color, and the "Other" bucket of a breakdown: grey means "no identity". */
export const COLOR_OTHER_COUNTRIES = COLOR_UNKNOWN

/** The color for an ISO-2 code; the "Other" grey for anything unmapped (including `(unknown)`). */
export function countryColor(code: string): string {
  const upper = code.toUpperCase()
  return upper in countryColors ? countryColors[upper as keyof typeof countryColors] : COLOR_OTHER_COUNTRIES
}

/** `hex` mixed toward white by `share` (0 = unchanged, 1 = white). The lighter tints of a country's color. */
export function tint(hex: string, share: number): string {
  const channels = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16))
  return `#${channels
    .map((c) =>
      Math.round(c + (255 - c) * share)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`
}
