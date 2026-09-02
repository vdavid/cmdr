/**
 * Byte-size and byte-rate formatting: the unit math, with the binary/SI base
 * passed in explicitly.
 *
 * Pure and settings-free, so it stays testable and importable from anywhere.
 * The reactive wrappers that read the user's `appearance.fileSizeFormat` live
 * in `index.ts`; prefer those in UI code.
 */

import type { FileSizeFormat, FileSizeUnit } from '$lib/settings/types'
import { getNumberFormatter } from '$lib/intl/number-format'

/**
 * A count of bytes, distinct from any other number.
 *
 * The brand is what stops `filesDone` from being rendered where `bytesDone`
 * belongs, and a duration in seconds from being formatted as a size — the two
 * mix-ups that produce a plausible-looking wrong number. It's structural, not
 * a runtime wrapper: `bytes(n)` compiles away, so there's no cost.
 *
 * Numbers arriving from IPC are plain `number` (the generated bindings can't
 * carry the brand), so a display surface brands at its edge with `bytes(...)`.
 * The lint that keeps private byte formatters from reappearing is
 * `cmdr/no-private-unit-format`.
 */
declare const byteCountBrand: unique symbol
export type ByteCount = number & { readonly [byteCountBrand]: 'bytes' }

/** Brand a plain number as a byte count. Compiles away; no runtime cost. */
export function bytes(count: number): ByteCount {
  return count as ByteCount
}

/**
 * A transfer rate in bytes per second. Separate from {@link ByteCount} because
 * a rate and a size are not interchangeable even though both count bytes: a
 * speed readout must never be handed a file size, and a size column must never
 * be handed a rate. The per-second marker itself is user-facing copy and lives
 * in the i18n catalog (`fileOperations.shared.byteRate`), not here.
 */
declare const byteRateBrand: unique symbol
export type BytesPerSecond = number & { readonly [byteRateBrand]: 'bytes/s' }

/** Brand a plain number as a byte rate. Compiles away; no runtime cost. */
export function bytesPerSecond(rate: number): BytesPerSecond {
  return rate as BytesPerSecond
}

/** Binary units (base 1024), the traditional computing ones. */
const binaryUnits = ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB']

/** SI units (base 1000), the International System of Units. */
const siUnits = ['bytes', 'kB', 'MB', 'GB', 'TB', 'PB']

/** The divisor between adjacent units under the chosen base. */
export function baseFor(format: FileSizeFormat): number {
  return format === 'binary' ? 1024 : 1000
}

/**
 * The user-facing label for `kB`/`MB`/`GB` under the current binary/SI base.
 * Binary mode shows `KB` (uppercase), SI shows `kB`. `MB` and `GB` are the same
 * in both, but we route them through one helper so callers never hand-pick the
 * casing.
 */
export function unitLabel(unit: 'kB' | 'MB' | 'GB', format: FileSizeFormat): string {
  if (unit === 'kB') return format === 'binary' ? 'KB' : 'kB'
  return unit
}

/**
 * Format bytes as a human-readable string.
 *
 * Without `forceUnit`, picks the friendliest unit per value (the "dynamic"
 * behavior). With `forceUnit` (`'kB'`/`'MB'`/`'GB'`), always renders in that
 * unit so sizes are apples-to-apples across a directory. The base (1024 vs
 * 1000) and the kilobyte label casing both come from `format`.
 *
 * `bytes` mode is not handled here — callers route raw-byte rendering through
 * `formatSizeTriads` for the colored triad treatment.
 *
 * `rounded` is the LIVE form: one fraction digit below ten and none above
 * ("1.7 GB", "24 GB", not "1.70 GB" / "24.41 GB"). A number that changes several
 * times a second is easier to read coarse, but not so coarse that two different
 * sizes print the same — the transfer progress bars are the case it exists for.
 * A size someone compares or copies keeps its two decimals.
 *
 * @param byteCount Number of bytes
 * @param format 'binary' uses 1024-based (KB/MB/GB), 'si' uses 1000-based (kB/MB/GB)
 * @param forceUnit Optional fixed unit to render in
 * @param rounded Render the live form (a tenth below ten, whole units above)
 */
export function formatFileSizeWithFormat(
  byteCount: number,
  format: FileSizeFormat,
  forceUnit?: 'kB' | 'MB' | 'GB',
  rounded = false,
): string {
  const base = baseFor(format)
  const units = format === 'binary' ? binaryUnits : siUnits
  const formatScaled = rounded ? formatSizeLive : formatSizeDecimal

  if (forceUnit) {
    const power = forceUnit === 'kB' ? 1 : forceUnit === 'MB' ? 2 : 3
    const value = byteCount / base ** power
    return `${formatScaled(value)} ${unitLabel(forceUnit, format)}`
  }

  let value = byteCount
  let unitIndex = 0
  while (value >= base && unitIndex < units.length - 1) {
    value /= base
    unitIndex++
  }

  // Sub-base values render as a bare integer (matching the old `String(value)`);
  // anything scaled into kB+ shows two fraction digits unless `rounded`.
  const valueStr = unitIndex === 0 ? formatSizeInteger(value) : formatScaled(value)
  return `${valueStr} ${units[unitIndex]}`
}

/**
 * Format the NUMERIC part of a human-friendly size with the active locale's
 * decimal separator (en-US `1.02`, de-DE `1,02`), two fraction digits, and NO
 * grouping. Grouping is suppressed so en-US stays byte-identical to the old
 * `toFixed(2)` (which never grouped); a forced-unit value like `10000.00 MB`
 * must not become `10,000.00 MB`. The value↔unit ASCII space is added by the
 * caller, never by Intl, so `colorizeSizeString`'s last-space parse survives.
 */
function formatSizeDecimal(value: number): string {
  return getNumberFormatter({
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
    useGrouping: false,
  }).format(value)
}

/** Format an integer size value (bytes mode) with no grouping, matching the old `String(value)`. */
function formatSizeInteger(value: number): string {
  return getNumberFormatter({ maximumFractionDigits: 0, useGrouping: false }).format(value)
}

/**
 * The numeric part of a LIVE size: coarse enough not to flicker, precise enough
 * to stay true. One fraction digit below ten ("1.7 GB"), none from ten up
 * ("24 GB").
 *
 * ⚠️ Whole units alone are not coarse, they're WRONG: a 1.7 GB / 2.4 GB transfer
 * renders as "2 GB / 2 GB" beside a percentage saying 70%, and every transfer in
 * the 1-10 GB range steps a whole gigabyte at a time. The tenth is what keeps
 * the two numbers and the percentage telling the same story. It costs no column
 * width, because a single-digit value has a digit to spare against the "999 GB"
 * worst case the readout is sized for.
 *
 * The digit count is decided on the value AS SHOWN, so 9.97 becomes "10", never
 * "10.0". `minimumFractionDigits` matches the maximum below ten so the tenth
 * doesn't appear and vanish as a live number crosses a whole unit.
 */
function formatSizeLive(value: number): string {
  const shown = Math.round(value * 10) / 10
  const fractionDigits = Math.abs(shown) < 10 ? 1 : 0
  return getNumberFormatter({
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
    useGrouping: false,
  }).format(shown)
}

/**
 * Resolve a `FileSizeUnit` to the fixed unit token (or `null` for the dynamic
 * mode). Bytes mode also returns `null` here because the raw-byte path is not
 * a "human-friendly with forced unit" case; it goes through `formatSizeTriads`
 * upstream.
 */
export function fixedUnitFor(unit: FileSizeUnit): 'kB' | 'MB' | 'GB' | null {
  if (unit === 'kB' || unit === 'MB' || unit === 'GB') return unit
  return null
}

/**
 * Magnitude tier of `byteCount` under the chosen base — the tier dynamic mode
 * would settle on for this value. Returns an index into the canonical tier
 * order: 0=bytes, 1=kB/KB, 2=MB, 3=GB, 4=TB+ (TB and PB share the top tier).
 *
 * Forced-unit display modes use this so the tier color still tracks the
 * file's real size, even though the rendered label is fixed (a 349-byte file
 * shown as `"0.00 MB"` still gets the bytes-tier color, the same green a user
 * would expect from dynamic mode).
 */
export function dynamicTierIndex(byteCount: number, format: FileSizeFormat): number {
  const base = baseFor(format)
  let value = byteCount
  let tier = 0
  while (value >= base && tier < 4) {
    value /= base
    tier++
  }
  return tier
}
