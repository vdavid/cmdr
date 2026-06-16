/**
 * Utility functions for SelectionInfo component.
 * Extracted for testability.
 */

import type { FileEntry } from '../types'
import type { FileSizeFormat, FileSizeUnit } from '$lib/settings/types'
import {
  formatFileSizeWithFormat,
  fixedUnitFor,
  dynamicTierIndex,
  type DateSegment,
  type FormattedDate,
} from '$lib/settings/format-utils'
import { formatInteger, getGroupSeparator } from '$lib/intl/number-format'
import { tString } from '$lib/intl/messages.svelte'

// Size tier colors for digit triads (indexed: 0=bytes, 1=kB, 2=MB, 3=GB, 4=TB+)
export const sizeTierClasses = ['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']

/** Formats a number into digit triads with separate styled spans */
export function formatSizeTriads(bytes: number): { value: string; tierClass: string }[] {
  const str = String(bytes)
  const triads: { value: string; tierClass: string }[] = []

  // Split into triads from right to left
  let remaining = str
  let tierIndex = 0
  while (remaining.length > 0) {
    const start = Math.max(0, remaining.length - 3)
    const triad = remaining.slice(start)
    remaining = remaining.slice(0, start)

    triads.unshift({
      value: triad,
      tierClass: sizeTierClasses[Math.min(tierIndex, sizeTierClasses.length - 1)],
    })
    tierIndex++
  }

  // Group separator follows the active locale (so byte triads agree with the
  // localized counts from `formatNumber`), sourced once from the number-format
  // layer. We keep the per-triad split + tier coloring (the reason this is
  // bespoke rather than `Intl.NumberFormat`) and only swap the separator.
  const separator = getGroupSeparator()
  return triads.map((t, i) => ({
    ...t,
    value: i < triads.length - 1 ? t.value + separator : t.value,
  }))
}

/**
 * Picks a size tier CSS class for a human-friendly size string like
 * "1.02 MB" or "512 bytes". Returns the closest of `sizeTierClasses` so the
 * unit-tagged span uses the same coloring as the raw-bytes triad mode.
 */
export function tierClassForUnit(unit: string): string {
  const lower = unit.toLowerCase()
  if (lower === 'bytes') return 'size-bytes'
  if (lower === 'kb') return 'size-kb' // matches KB (binary) and kB (SI)
  if (lower === 'mb') return 'size-mb'
  if (lower === 'gb') return 'size-gb'
  // TB, PB and anything beyond fall back to the highest defined tier
  return 'size-tb'
}

/**
 * Formats a byte count for display in views/status bar based on the user's
 * `listing.sizeUnit` preference. Returns an array of tier-tagged spans:
 * - `'bytes'`: delegates to {@link formatSizeTriads} (one span per digit triad).
 * - `'dynamic'`: picks the friendliest unit per file ("1.02 MB"), one span.
 * - `'kB' | 'MB' | 'GB'`: forces that unit for display, one span. The kilobyte
 *   label reflects binary (`KB`) vs SI (`kB`) via `opts.format`.
 *
 * Tier color in forced modes follows the **magnitude tier** of the underlying
 * byte count (what dynamic mode would have picked), not the displayed unit.
 * So a 349-byte file shown as `"0.00 MB"` still tier-colors as `size-bytes` —
 * the user's at-a-glance "how big is this" signal stays meaningful even when
 * every row uses the same fixed unit.
 */
export function formatSizeForDisplay(
  bytes: number,
  opts: { unit: FileSizeUnit; format: FileSizeFormat },
): { value: string; tierClass: string }[] {
  if (opts.unit === 'bytes') {
    return formatSizeTriads(bytes)
  }
  const forced = fixedUnitFor(opts.unit)
  const formatted = formatFileSizeWithFormat(bytes, opts.format, forced ?? undefined)
  if (forced) {
    return [{ value: formatted, tierClass: sizeTierClasses[dynamicTierIndex(bytes, opts.format)] }]
  }
  // Dynamic mode: tier from the chosen unit (the rendered unit IS the magnitude).
  // The formatter returns "<value> <unit>"; the unit is the last whitespace-separated token.
  const spaceIndex = formatted.lastIndexOf(' ')
  const unit = spaceIndex >= 0 ? formatted.slice(spaceIndex + 1) : ''
  return [{ value: formatted, tierClass: tierClassForUnit(unit) }]
}

/**
 * Wraps an already-formatted size string (e.g. `"1.02 MB"`, `"512 bytes"`) in a colored span
 * based on its unit suffix. Use when the value comes from a foreign formatter (like the legacy
 * `formatBytes` in `tauri-commands`) and you just need tier coloring on top, without re-formatting.
 */
export function colorizeSizeString(text: string): string {
  const spaceIndex = text.lastIndexOf(' ')
  const unit = spaceIndex >= 0 ? text.slice(spaceIndex + 1) : ''
  return `<span class="${tierClassForUnit(unit)}">${text}</span>`
}

/** Formats timestamp as YYYY-MM-DD hh:mm:ss */
export function formatDate(timestamp: number | null | undefined): string {
  if (timestamp == null) return ''
  const date = new Date(timestamp * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  const year = date.getFullYear()
  const month = pad(date.getMonth() + 1)
  const day = pad(date.getDate())
  const hours = pad(date.getHours())
  const mins = pad(date.getMinutes())
  const secs = pad(date.getSeconds())
  return `${String(year)}-${month}-${day} ${hours}:${mins}:${secs}`
}

/** Escape `&`, `<`, `>` so segment text is safe inside the `{ html }` tooltip. */
function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/** Render an ordered segment list to HTML, wrapping colored segments in their age-tier span. */
function renderSegments(segments: DateSegment[]): string {
  let out = ''
  for (const seg of segments) {
    const text = escapeHtml(seg.text)
    out += seg.ageClass ? `<span class="${seg.ageClass}">${text}</span>` : text
  }
  return out
}

/**
 * Builds date tooltip content as HTML, with each colored segment wrapped in
 * its age-tier span so the active `data-date-colors` palette colors year,
 * month, day, and time independently. Takes a `formatter` callback so the
 * util stays pure: the caller passes `formattedDate` from
 * `reactive-settings.svelte.ts` to inherit the user's date format setting, or
 * a stub from tests.
 *
 * Returns `{ html }` for the `tooltip` action. An empty `html` means no
 * timestamps were available on the entry.
 */
export function buildDateTooltip(
  e: FileEntry,
  formatter: (ts: number | null | undefined) => FormattedDate,
): { html: string } {
  const lines: string[] = []
  const line = (label: string, ts: number) => {
    const d = formatter(ts)
    lines.push(`${label}: ${renderSegments(d.segments)}`)
  }
  // `!= null` because IPC payloads serialize `Option::None` as JSON `null`.
  if (e.createdAt != null) line(tString('fileExplorer.dateTooltip.created'), e.createdAt)
  if (e.openedAt != null) line(tString('fileExplorer.dateTooltip.lastOpened'), e.openedAt)
  if (e.addedAt != null) line(tString('fileExplorer.dateTooltip.lastMoved'), e.addedAt)
  if (e.modifiedAt != null) line(tString('fileExplorer.dateTooltip.lastModified'), e.modifiedAt)
  return { html: lines.join('<br>') }
}

/** Determines size display for an entry, using the display size based on the mode */
export function getSizeDisplay(
  entry: FileEntry | null,
  isBrokenSymlink: boolean,
  isPermissionDenied: boolean,
  displaySize?: number,
  formatOpts?: { unit: FileSizeUnit; format: FileSizeFormat },
): { value: string; tierClass: string }[] | 'DIR' | null {
  if (!entry || isBrokenSymlink || isPermissionDenied) return null
  const opts = formatOpts ?? { unit: 'bytes' as const, format: 'binary' as const }
  // `!= null` because the Rust wire format serializes Optional fields as `null`
  // (see Group A migration in `getDisplaySize` doc).
  if (entry.isDirectory) return displaySize != null ? formatSizeForDisplay(displaySize, opts) : 'DIR'
  const size = displaySize ?? entry.size
  if (size == null) return null
  return formatSizeForDisplay(size, opts)
}

/** Determines date display for an entry */
export function getDateDisplay(
  entry: FileEntry | null,
  isBrokenSymlink: boolean,
  isPermissionDenied: boolean,
  currentDirModifiedAt?: number,
): string {
  if (!entry) return ''
  if (isBrokenSymlink) return tString('fileExplorer.entry.brokenSymlink')
  if (isPermissionDenied) return tString('fileExplorer.entry.permissionDenied')
  // For ".." entry, use the current directory's modified time
  const timestamp = entry.name === '..' ? currentDirModifiedAt : entry.modifiedAt
  return formatDate(timestamp)
}

/** Checks if entry is a broken symlink */
export function isBrokenSymlink(entry: FileEntry | null): boolean {
  return entry !== null && entry.isSymlink && entry.iconId === 'symlink-broken'
}

/** Checks if entry has permission denied */
export function isPermissionDenied(entry: FileEntry | null): boolean {
  return entry !== null && !entry.isSymlink && entry.permissions === 0 && entry.size == null
}

// ============================================================================
// Selection summary utilities
// ============================================================================

/** Formats a count with the active locale's thousands grouping (en-US `1,234`, de-DE `1.234`). */
export function formatNumber(n: number): string {
  return formatInteger(n)
}

/** Calculates percentage, rounded to nearest integer */
export function calculatePercentage(part: number, total: number): number {
  if (total === 0) return 0
  return Math.round((part / total) * 100)
}
