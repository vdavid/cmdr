/**
 * Units: the one place a byte count, a transfer rate, or a duration turns into
 * text a person reads.
 *
 * Start here. `formatByteSize` honors the user's `appearance.fileSizeFormat`
 * (binary KB / SI kB) automatically; `formatFileSizeWithFormat` takes the base
 * explicitly for pure code and tests. The component form of a size is
 * `<Size bytes>` (`$lib/ui/Size.svelte`), which also carries the size-tier
 * colors.
 *
 * ```ts
 * import { formatByteSize, formatDuration, seconds } from '$lib/units'
 *
 * formatByteSize(87_654_321)   // "83.59 MB" (binary) or "87.65 MB" (SI)
 * formatDuration(seconds(492)) // "8m 12s"
 * ```
 *
 * A transfer RATE is a size plus a per-second marker, and that marker is
 * user-facing copy, so it lives in the i18n catalog, not here: render
 * `<Trans key="fileOperations.shared.byteRate" snippets={{ size }} />` with a
 * `<Size bytes={rate}>` snippet, as the copy dialog and the operation queue
 * both do. Brand the rate with `bytesPerSecond(...)` so it can't be mistaken
 * for a file size on the way there.
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
import { formatFileSizeWithFormat } from './byte-size'

export {
  type ByteCount,
  type BytesPerSecond,
  bytes,
  bytesPerSecond,
  baseFor,
  unitLabel,
  dynamicTierIndex,
  formatFileSizeWithFormat,
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
