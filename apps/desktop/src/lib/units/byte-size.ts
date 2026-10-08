/**
 * Byte-size and byte-rate formatting: the unit math, with the binary/SI base
 * passed in explicitly.
 *
 * Settings-free, so it stays testable and importable from anywhere. The
 * reactive wrappers that read the user's `appearance.fileSizeFormat` live in
 * `index.ts`; prefer those in UI code.
 *
 * The unit words ("MiB", "MB", "Mo", "bytes") are catalog copy in the UI
 * language (`common.sizeUnit.*`), so a translator owns them; the digits follow
 * the formatting locale like every other number. Each base has its own symbols:
 * base 1024 is IEC binary (KiB, MiB, GiB, TiB, PiB), base 1000 is SI decimal
 * (kB, MB, GB, TB, PB), so the symbol always names the divisor behind the digits.
 */

import type { FileSizeFormat, FileSizeUnit } from '$lib/settings/types'
import { getNumberFormatter } from '$lib/intl/number-format'
import { tString } from '$lib/intl/messages.svelte'

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

/** The index of the top unit (PiB / PB); larger values stay there. */
const TOP_UNIT_INDEX = 5

/** The catalog symbol per unit index 1–5 (KiB … PiB), base 1024. */
const BINARY_UNIT_KEYS = [
  'common.sizeUnit.kibibyte',
  'common.sizeUnit.mebibyte',
  'common.sizeUnit.gibibyte',
  'common.sizeUnit.tebibyte',
  'common.sizeUnit.pebibyte',
] as const

/** The catalog symbol per unit index 1–5 (kB … PB), base 1000. */
const SI_UNIT_KEYS = [
  'common.sizeUnit.kilobyte',
  'common.sizeUnit.megabyte',
  'common.sizeUnit.gigabyte',
  'common.sizeUnit.terabyte',
  'common.sizeUnit.petabyte',
] as const

/** The divisor between adjacent units under the chosen base. */
export function baseFor(format: FileSizeFormat): number {
  return format === 'binary' ? 1024 : 1000
}

/**
 * The unit word for a unit index (0 = bytes … 5 = PiB / PB), in the UI language.
 * Every tier above bytes has a binary and an SI symbol (`MiB` / `MB` in English),
 * and the byte word takes the plural form for `count`.
 */
function unitWord(unitIndex: number, format: FileSizeFormat, count: number): string {
  if (unitIndex === 0) return bytesLabel(count)
  const keys = format === 'binary' ? BINARY_UNIT_KEYS : SI_UNIT_KEYS
  return tString(keys[Math.min(unitIndex, TOP_UNIT_INDEX) - 1])
}

/**
 * The byte unit word for `count` bytes, in the UI language and its plural
 * rules ("1 byte", "2 bytes", French "0 octet"). For a raw byte count shown
 * beside its digits, like the "(1,234,567 bytes)" line in a size tooltip.
 */
export function bytesLabel(count: number): string {
  return tString('common.sizeUnit.byte', { count })
}

/**
 * The user-facing label for `kB`/`MB`/`GB` under the current binary/SI base,
 * in the UI language: English binary shows `KiB` / `MiB` / `GiB`, SI shows
 * `kB` / `MB` / `GB`. The tokens are the stable unit IDs, not the labels.
 * Every unit label goes through here (or the formatters below) so no caller
 * hand-picks the symbol or the language.
 */
export function unitLabel(unit: 'kB' | 'MB' | 'GB', format: FileSizeFormat): string {
  return unitWord(unitPower(unit), format, 0)
}

/** The power of the base a forced unit stands for. */
function unitPower(unit: 'kB' | 'MB' | 'GB'): number {
  return unit === 'kB' ? 1 : unit === 'MB' ? 2 : 3
}

/**
 * A formatted size plus its size tier, so a caller can color it without
 * reading the unit back out of the text (which is translated copy).
 */
export interface TieredSize {
  /** The size as shown, like "1.02 MiB" or "1,02 Mo". */
  text: string
  /** The magnitude tier, as {@link dynamicTierIndex} gives it (0 = bytes … 4 = TB and up). */
  tier: number
}

/**
 * Format bytes as a human-readable string.
 *
 * Without `forceUnit`, picks the friendliest unit per value (the "dynamic"
 * behavior). With `forceUnit` (`'kB'`/`'MB'`/`'GB'`), always renders in that
 * unit so sizes are apples-to-apples across a directory. The base (1024 vs
 * 1000) and the unit symbols (IEC vs SI) both come from `format`.
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
 * @param format 'binary' uses 1024-based (KiB/MiB/GiB), 'si' uses 1000-based (kB/MB/GB)
 * @param forceUnit Optional fixed unit to render in
 * @param rounded Render the live form (a tenth below ten, whole units above)
 */
export function formatFileSizeWithFormat(
  byteCount: number,
  format: FileSizeFormat,
  forceUnit?: 'kB' | 'MB' | 'GB',
  rounded = false,
): string {
  return formatTieredSize(byteCount, format, forceUnit, rounded).text
}

/**
 * {@link formatFileSizeWithFormat}, plus the size tier to color it by. Under a
 * forced unit the tier still follows the magnitude, so a 349-byte file shown as
 * "0.00 MiB" keeps the bytes tier.
 */
export function formatTieredSize(
  byteCount: number,
  format: FileSizeFormat,
  forceUnit?: 'kB' | 'MB' | 'GB',
  rounded = false,
): TieredSize {
  const base = baseFor(format)
  const formatScaled = rounded ? formatSizeLive : formatSizeDecimal
  const tier = dynamicTierIndex(byteCount, format)

  if (forceUnit) {
    const power = unitPower(forceUnit)
    const value = byteCount / base ** power
    return { text: `${formatScaled(value)} ${unitWord(power, format, value)}`, tier }
  }

  const { value, unitIndex } = scaleToFriendliestUnit(byteCount, base)
  // Sub-base values render as a bare integer (matching the old `String(value)`);
  // anything scaled into kB+ shows two fraction digits unless `rounded`.
  const valueStr = unitIndex === 0 ? formatSizeInteger(value) : formatScaled(value)
  return { text: `${valueStr} ${unitWord(unitIndex, format, value)}`, tier }
}

/** Divide down to the largest unit the value reaches, capped at PB. */
function scaleToFriendliestUnit(byteCount: number, base: number): { value: number; unitIndex: number } {
  let value = byteCount
  let unitIndex = 0
  while (value >= base && unitIndex < TOP_UNIT_INDEX) {
    value /= base
    unitIndex++
  }
  return { value, unitIndex }
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
 * A preset size, like a setting's option, in its friendliest unit with only the
 * fraction digits it has: "100 MiB", "3 GiB", "1.5 GiB". For fixed amounts a
 * person picks from, where "100.00 MiB" would be noise. The base and symbols come
 * from `format`, so a binary-valued preset passes `'binary'` whatever the user's
 * display setting is: the label has to name the base the value was built in.
 */
export function formatRoundSize(byteCount: number, format: FileSizeFormat): string {
  const { value, unitIndex } = scaleToFriendliestUnit(byteCount, baseFor(format))
  const text = getNumberFormatter({ maximumFractionDigits: 2, useGrouping: false }).format(value)
  return `${text} ${unitWord(unitIndex, format, value)}`
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
 * order: 0=bytes, 1=KiB/kB, 2=MiB/MB, 3=GiB/GB, 4=TiB+/TB+ (the top two units share a tier).
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

/**
 * How many steps a drive's free-space readout resolves the drive into: about one per pixel of a
 * pane's usage bar. The figure never claims more precision than that, so a write the bar can't
 * show doesn't change the text either.
 *
 * ❗ Mirrored by `src-tauri/src/space_poller/readout.rs` (`DRIVE_STEPS`), whose emit gate has to
 * know when this text changes. Both test against `drive-figure-cases.json`.
 */
const DRIVE_STEPS = 1000

/**
 * Format a figure on a drive of `driveBytes` (its free or total space) as precisely as the drive's
 * size makes worth reading: about one step per pixel of the usage bar ({@link DRIVE_STEPS}), and
 * never coarser than the live form ("2.3 GB", "261 GB").
 *
 * In the friendliest unit, the fraction digits are the most (up to two) whose step is still at
 * least 1/1000 of the drive, and at least one below ten. So an 8 MB card reads "3.20 MB", a 1 TB
 * SSD "261 GB", a 4 TB drive "1.23 TB", and the same SSD nearly full "2.3 GB", then "800 MB".
 */
export function formatDriveFigure(byteCount: number, driveBytes: number, format: FileSizeFormat): string {
  const base = baseFor(format)
  const { value, unitIndex } = scaleToFriendliestUnit(byteCount, base)
  if (unitIndex === 0) return `${formatSizeInteger(value)} ${unitWord(0, format, value)}`

  const unitBytes = base ** unitIndex
  const pixelBytes = driveBytes / DRIVE_STEPS
  let pixelDigits = 0
  for (const digits of [2, 1]) {
    if (unitBytes / 10 ** digits >= pixelBytes) {
      pixelDigits = digits
      break
    }
  }
  // The live form's rule, decided on the value as shown: a tenth below ten.
  const liveDigits = Math.round(value * 10) / 10 < 10 ? 1 : 0
  const fractionDigits = Math.max(pixelDigits, liveDigits)
  const text = getNumberFormatter({
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
    useGrouping: false,
  }).format(value)
  return `${text} ${unitWord(unitIndex, format, value)}`
}
