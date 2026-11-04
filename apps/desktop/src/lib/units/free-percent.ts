/**
 * A drive's free percent as text: as precise as its distance from an end makes worth reading, and
 * never claiming an end it hasn't reached.
 *
 * - 5% up to 95% free: whole numbers ("42").
 * - 95% up to 99%, and 1% up to 5%: one decimal ("97.8", "4.2").
 * - 99% and above, and below 1%: two decimals ("99.64", "0.40").
 * - Truncated at the shown precision, so no figure rounds up into the next band. "100" only when
 *   nothing is used and "0" only when nothing is free: a sliver free reads as the smallest step
 *   ("0.01"), and the exact ends drop the decimals ("100", "0").
 *
 * ❗ `space_poller/readout.rs` (`displayed_free_percent`) mirrors this for the emit gate;
 * `free-percent-cases.json` tests both, so a band change on one side fails the other's test.
 */

import { getNumberFormatter } from '#lib/intl/number-format.js'

/** The free percent as drawn: its fraction digits and the value at that precision. */
interface FreePercentFigure {
  digits: number
  value: number
}

/**
 * Absorbs float noise like `97.8 * 10 = 977.9999…` before truncating. Far below a hundredth, and
 * the end guards below keep it from ever reaching 0 or 100.
 */
const TRUNCATE_EPSILON = 1e-9

function bandDigits(percent: number): number {
  if (percent >= 99 || percent < 1) return 2
  if (percent >= 95 || percent < 5) return 1
  return 0
}

/**
 * The figure {@link formatFreePercent} writes, for `percent` (0–100, clamped; `NaN` reads as 0).
 *
 * @param minDigits The fewest fraction digits to show (the low-space warning wants a tenth even
 *   at 40% free)
 */
function freePercentFigure(percent: number, minDigits = 0): FreePercentFigure {
  const clamped = Number.isNaN(percent) ? 0 : Math.max(0, Math.min(100, percent))
  if (clamped === 0 || clamped === 100) return { digits: minDigits, value: clamped }
  const digits = Math.max(bandDigits(clamped), minDigits)
  const scale = 10 ** digits
  const steps = Math.floor(clamped * scale + TRUNCATE_EPSILON)
  // Between the ends, the figure stays strictly between them too.
  const bounded = Math.max(1, Math.min(100 * scale - 1, steps))
  return { digits, value: bounded / scale }
}

/**
 * A drive's free percent in the UI language's digits ("99.64", German "99,64"), without the percent
 * sign: the catalog string places that. Bands and truncation: see the module doc.
 *
 * @param percent Free space as a percent of the drive, 0–100
 * @param minDigits The fewest fraction digits to show
 */
export function formatFreePercent(percent: number, minDigits = 0): string {
  const { digits, value } = freePercentFigure(percent, minDigits)
  return getNumberFormatter({ minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value)
}
