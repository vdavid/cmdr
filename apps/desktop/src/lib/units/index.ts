/**
 * Units: the one place a byte count, a transfer rate, or a duration turns into
 * text a person reads.
 *
 * Start here. `formatByteSize` and `formatByteRate` honor the user's
 * `appearance.fileSizeFormat` (binary KB / SI kB) automatically; the
 * `*WithFormat` variants take the base explicitly for pure code and tests. The
 * component form of a size is `<Size bytes>` (`$lib/ui/Size.svelte`), which
 * also carries the size-tier colors.
 *
 * ```ts
 * import { formatByteSize, formatByteRate, formatDuration } from '$lib/units'
 *
 * formatByteSize(87_654_321)   // "83.59 MB" (binary) or "87.65 MB" (SI)
 * formatByteRate(1_234_567)    // "1.18 MB/s" or "1.23 MB/s"
 * formatDuration(492)          // "8m 12s"
 * ```
 *
 * ❌ Don't write a private `formatBytes` / `formatSpeed` / `formatEta`. Four of
 * them once drifted apart and hardcoded base 1024 while labelling the result
 * "MB", which is how two Cmdr windows came to show different numbers for the
 * same transfer. `cmdr/no-private-unit-format` rejects new ones.
 *
 * Size-tier COLORING (the `size-bytes` … `size-tb` spans) is a separate layer
 * in `$lib/file-explorer/selection/selection-info-utils.ts`
 * (`formatSizeForDisplay`, `colorizeSizeString`), because the classes belong to
 * the list views' stylesheet. Dates have their own single source of truth in
 * `$lib/settings/format-utils.ts` + `<DateLabel>`.
 */

import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
import { formatFileSizeWithFormat, formatByteRateWithFormat, type BytesPerSecond } from './byte-size'

export {
  type ByteCount,
  type BytesPerSecond,
  bytes,
  bytesPerSecond,
  baseFor,
  unitLabel,
  fixedUnitFor,
  dynamicTierIndex,
  formatFileSizeWithFormat,
  formatByteRateWithFormat,
} from './byte-size'

export { type Seconds, seconds, formatDuration, formatMilliseconds, formatFilesPerSecond } from './duration'

/**
 * Format a byte count for display, honoring the user's binary/SI setting.
 * The default text form of a size; for the colored inline form use `<Size>`.
 *
 * @param byteCount Number of bytes
 * @param forceUnit Optional fixed unit (`'kB'` / `'MB'` / `'GB'`) instead of the friendliest one
 */
export function formatByteSize(byteCount: number, forceUnit?: 'kB' | 'MB' | 'GB'): string {
  return formatFileSizeWithFormat(byteCount, getFileSizeFormat(), forceUnit)
}

/**
 * Format a transfer rate as `"<size>/s"`, honoring the user's binary/SI
 * setting. The one definition of how a speed reads, so no two surfaces can
 * phrase the same rate differently.
 */
export function formatByteRate(rate: BytesPerSecond): string {
  return formatByteRateWithFormat(rate, getFileSizeFormat())
}
