import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import type { VolumeSpaceInfo } from '$lib/tauri-commands'

type FormatSize = (bytes: number) => string

/**
 * How full a volume is, as a typed band rather than a label. The colour is the
 * only thing the bar itself needs; the two low-space bands additionally get a
 * sentence in the tooltip. ❌ Never branch on the copy — that's what the
 * `severity` discriminant is for.
 */
export interface DiskUsageLevel {
  severity: 'ok' | 'warning' | 'critical'
  cssVar: string
}

/** Returns the usage band for the bar color and the tooltip's warning sentence. */
export function getDiskUsageLevel(usedPercent: number): DiskUsageLevel {
  if (usedPercent >= 95) return { severity: 'critical', cssVar: '--color-disk-danger' }
  if (usedPercent >= 80) return { severity: 'warning', cssVar: '--color-disk-warning' }
  return { severity: 'ok', cssVar: '--color-disk-ok' }
}

/** Returns used percentage (0–100), clamped. */
export function getUsedPercent(space: VolumeSpaceInfo): number {
  if (space.totalBytes <= 0) return 0
  const used = space.totalBytes - space.availableBytes
  return Math.max(0, Math.min(100, Math.round((used / space.totalBytes) * 100)))
}

/** Free percentage (0–100), clamped, as the catalog's preformatted string param. */
function freePercentText(space: VolumeSpaceInfo): string {
  const freePercent = Math.max(0, Math.min(100, Math.round((space.availableBytes / space.totalBytes) * 100)))
  return formatInteger(freePercent)
}

/** Formats the status bar text: "420 GB of 1 TB free (42%)" */
export function formatDiskSpaceStatus(space: VolumeSpaceInfo, formatSize: FormatSize): string {
  return tString('fileExplorer.diskSpace.free', {
    freeText: formatSize(space.availableBytes),
    totalText: formatSize(space.totalBytes),
    percentText: freePercentText(space),
  })
}

/** Formats the short volume selector text: "420 GB free of 1 TB". */
export function formatDiskSpaceShort(space: VolumeSpaceInfo, formatSize: FormatSize): string {
  return tString('fileExplorer.diskSpace.freeShort', {
    freeText: formatSize(space.availableBytes),
    totalText: formatSize(space.totalBytes),
  })
}

/**
 * Formats the usage bar tooltip: sizes, percentage, a contextual warning when
 * space is low, and an optional trailing hint. `mtpHint` carries the
 * phone-storage explanation (resolved from the message catalog by the caller)
 * for MTP volumes, where the browsable folders add up to less than the used
 * space because apps and system data aren't reachable over USB.
 *
 * The sizes and the notes after them are separate sentences, so the catalog
 * owns how they're joined: a language that ends a sentence with something other
 * than `. ` gets to say so.
 */
export function formatBarTooltip(space: VolumeSpaceInfo, formatSize: FormatSize, mtpHint?: string): string {
  const level = getDiskUsageLevel(getUsedPercent(space))
  const notes: string[] = []
  if (level.severity === 'critical') notes.push(tString('fileExplorer.diskSpace.lowNote'))
  else if (level.severity === 'warning') notes.push(tString('fileExplorer.diskSpace.somewhatLowNote'))
  if (mtpHint) notes.push(mtpHint)

  return tString('fileExplorer.diskSpace.barTooltip', {
    hasNotes: notes.length > 0 ? 'yes' : 'no',
    sizes: formatDiskSpaceStatus(space, formatSize),
    notes: notes.join(' '),
  })
}
