import type { VolumeSpaceInfo } from '$lib/tauri-commands/storage'

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

/** Formats the usage bar tooltip: sizes, percentage, and a contextual warning when space is low. */
export function formatBarTooltip(space: VolumeSpaceInfo, formatSize: FormatSize): string {
    const freeText = formatSize(space.availableBytes)
    const totalText = formatSize(space.totalBytes)
    const usedPercent = getUsedPercent(space)
    const freePercent = 100 - usedPercent
    const level = getDiskUsageLevel(usedPercent)
    const base = `${freeText} of ${totalText} free (${String(freePercent)}%)`
    if (level.label === 'Critical') return `${base}. This bar is red to indicate that it's low on space.`
    if (level.label === 'Warning') return `${base}. This bar is yellow to indicate that it's somewhat low on space.`
    return base
}
