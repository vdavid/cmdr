import type { VolumeSpaceInfo } from '$lib/tauri-commands'

type FormatSize = (bytes: number) => string

export interface DiskUsageLevel {
  cssVar: string
  label: string
}

/** Returns the CSS variable name for the usage bar color based on percentage used. */
export function getDiskUsageLevel(usedPercent: number): DiskUsageLevel {
  if (usedPercent >= 95) return { cssVar: '--color-disk-danger', label: 'Critical' }
  if (usedPercent >= 80) return { cssVar: '--color-disk-warning', label: 'Warning' }
  return { cssVar: '--color-disk-ok', label: 'OK' }
}

/** Returns used percentage (0–100), clamped. */
export function getUsedPercent(space: VolumeSpaceInfo): number {
  if (space.totalBytes <= 0) return 0
  const used = space.totalBytes - space.availableBytes
  return Math.max(0, Math.min(100, Math.round((used / space.totalBytes) * 100)))
}

/** Formats the status bar text: "420 GB of 1 TB free (42%)" */
export function formatDiskSpaceStatus(space: VolumeSpaceInfo, formatSize: FormatSize): string {
  const freeText = formatSize(space.availableBytes)
  const totalText = formatSize(space.totalBytes)
  const freePercent = Math.max(0, Math.min(100, Math.round((space.availableBytes / space.totalBytes) * 100)))
  return `${freeText} of ${totalText} free (${String(freePercent)}%)`
}

/** Formats the short volume selector text: "420 GB free of 1 TB". */
export function formatDiskSpaceShort(space: VolumeSpaceInfo, formatSize: FormatSize): string {
  const freeText = formatSize(space.availableBytes)
  const totalText = formatSize(space.totalBytes)
  return `${freeText} free of ${totalText}`
}

/**
 * Formats the usage bar tooltip: sizes, percentage, a contextual warning when
 * space is low, and an optional trailing hint. `mtpHint` carries the
 * phone-storage explanation (resolved from the message catalog by the caller)
 * for MTP volumes, where the browsable folders add up to less than the used
 * space because apps and system data aren't reachable over USB.
 */
export function formatBarTooltip(space: VolumeSpaceInfo, formatSize: FormatSize, mtpHint?: string): string {
  const freeText = formatSize(space.availableBytes)
  const totalText = formatSize(space.totalBytes)
  const usedPercent = getUsedPercent(space)
  const freePercent = 100 - usedPercent
  const level = getDiskUsageLevel(usedPercent)
  const sentences: string[] = []
  if (level.label === 'Critical') sentences.push('This bar is red to indicate that the volume is low on space.')
  else if (level.label === 'Warning')
    sentences.push('This bar is yellow to indicate that the volume is somewhat low on space.')
  if (mtpHint) sentences.push(mtpHint)
  const base = `${freeText} of ${totalText} free (${String(freePercent)}%)`
  return sentences.length > 0 ? `${base}. ${sentences.join(' ')}` : base
}
