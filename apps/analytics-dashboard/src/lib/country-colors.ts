/**
 * Client-safe: one fixed color per country, so a country reads the same in every chart, table dot, and map.
 *
 * Assigned once, to the top getcmdr.com countries by 90-day visitors (Umami `metrics?type=country`,
 * 2026-11-10). Color follows the country, never its rank: when the ranking shifts, the colors stay as they
 * are, and a country without one is "Other" grey. Add a country deliberately (and re-validate) when one
 * without a color climbs into the charts' top `topCountryCount`.
 *
 * - The first eight (US through PL) were validated with the dataviz skill's `validate_palette.js --mode dark
 *   --surface #1a1917` in stack order: worst adjacent CVD ΔE 11.9, worst adjacent normal-vision ΔE 16.8, all
 *   ≥ 3:1 on the surface, each ≥ 15 ΔE from gold, Homebrew blue, and grey.
 * - The next 16 (ES through FI) were picked greedily from an OKLCH grid (L 0.58/0.68/0.78, C 0.10/0.14, hue
 *   every 15°) for the largest OKLab distance from every color already taken plus the dashboard's property
 *   colors (`colors.ts`), keeping ≥ 3:1 on the surface. With 24 hues on one dark surface, any two now sit
 *   ≥ 7.3 ΔE apart and neighbors in the 2026-11-10 rank order ≥ 10.8; the tooltip names each slice, so a
 *   near pair never stays ambiguous.
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
  ES: '#f09da3',
  IT: '#88ca95',
  SE: '#9eb5f8',
  UA: '#a46188',
  CA: '#00aec8',
  FR: '#ddae6c',
  CH: '#e995e2',
  RU: '#bb81b6',
  AU: '#7471b3',
  BE: '#708500',
  IL: '#cf8073',
  BY: '#ba9300',
  AT: '#0089b8',
  JP: '#ae6256',
  RO: '#c5bc42',
  FI: '#0fd4bb',
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
