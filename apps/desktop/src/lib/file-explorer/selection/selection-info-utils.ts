/**
 * Utility functions for SelectionInfo component.
 * Extracted for testability.
 */

import type { FileEntry } from '../types'
import type { FileSizeFormat } from '$lib/settings/types'
import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
import { tierClassForAge } from './age-tier-utils'

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

  // Add thousand separators between triads (space)
  return triads.map((t, i) => ({
    ...t,
    value: i < triads.length - 1 ? t.value + '\u2009' : t.value, // thin space separator
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
 * "human-friendly size units" preference. Returns an array of tier-tagged
 * spans:
 * - In human-friendly mode, returns one element like `{ value: '1.02 MB', tierClass: 'size-mb' }`.
 * - In raw-bytes mode, delegates to {@link formatSizeTriads} which returns one element per digit triad.
 */
export function formatSizeForDisplay(
  bytes: number,
  opts: { humanFriendly: boolean; format: FileSizeFormat },
): { value: string; tierClass: string }[] {
  if (!opts.humanFriendly) {
    return formatSizeTriads(bytes)
  }
  const formatted = formatFileSizeWithFormat(bytes, opts.format)
  // The formatter returns "<value> <unit>"; the unit is the last whitespace-separated token.
  const spaceIndex = formatted.lastIndexOf(' ')
  const unit = spaceIndex >= 0 ? formatted.slice(spaceIndex + 1) : ''
  return [{ value: formatted, tierClass: tierClassForUnit(unit) }]
}

/**
 * Renders a byte count as a colored HTML span string (e.g. `<span class="size-mb">1.02 MB</span>`).
 * For embedding inside HTML strings — tooltips, error messages, etc. Use the `<Size>` component
 * for inline rendering in Svelte templates.
 */
export function formatSizeHtmlColored(bytes: number, format: FileSizeFormat): string {
  return formatSizeForDisplay(bytes, { humanFriendly: true, format })
    .map((p) => `<span class="${p.tierClass}">${p.value}</span>`)
    .join('')
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

/**
 * Builds date tooltip content as HTML. Each timestamp is wrapped in its
 * age-tier span so the date portion picks up the active `data-date-colors`
 * palette. Returns `{ html }` for the tooltip action; an empty `html` means
 * no timestamps were available.
 */
export function buildDateTooltip(e: FileEntry, nowMs: number = Date.now()): { html: string } {
  const lines: string[] = []
  const line = (label: string, ts: number) => {
    const tier = tierClassForAge(ts, nowMs)
    const span = tier ? `<span class="${tier}">${formatDate(ts)}</span>` : formatDate(ts)
    lines.push(`${label}: ${span}`)
  }
  // `!= null` because IPC payloads serialize `Option::None` as JSON `null`.
  if (e.createdAt != null) line('Created', e.createdAt)
  if (e.openedAt != null) line('Last opened', e.openedAt)
  if (e.addedAt != null) line('Last moved ("added")', e.addedAt)
  if (e.modifiedAt != null) line('Last modified', e.modifiedAt)
  return { html: lines.join('<br>') }
}

/** Determines size display for an entry, using the display size based on the mode */
export function getSizeDisplay(
  entry: FileEntry | null,
  isBrokenSymlink: boolean,
  isPermissionDenied: boolean,
  displaySize?: number,
  formatOpts?: { humanFriendly: boolean; format: FileSizeFormat },
): { value: string; tierClass: string }[] | 'DIR' | null {
  if (!entry || isBrokenSymlink || isPermissionDenied) return null
  const opts = formatOpts ?? { humanFriendly: false, format: 'binary' as const }
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
  if (isBrokenSymlink) return '(broken symlink)'
  if (isPermissionDenied) return '(permission denied)'
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

/** Formats a count with proper singular/plural form */
export function pluralize(count: number, singular: string, plural: string): string {
  return count === 1 ? singular : plural
}

/** Formats a number with thousands separators using en-US locale */
export function formatNumber(n: number): string {
  return n.toLocaleString('en-US')
}

/** Calculates percentage, rounded to nearest integer */
export function calculatePercentage(part: number, total: number): number {
  if (total === 0) return 0
  return Math.round((part / total) * 100)
}
