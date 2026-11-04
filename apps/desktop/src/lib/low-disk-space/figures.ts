import { formatDriveSize, formatFreePercent } from '#lib/units/index.js'

/**
 * The low-space warning's two figures, shared by the in-app toast and the macOS notification: the
 * free space as precise as the drive's size makes worth reading (the same figure the status bar
 * shows), and the free percent to at least a tenth (`formatFreePercent`, so a sliver free reads
 * "0.01", never "0.0"), both in the active locale.
 *
 * An unknown total (0) reads as 100% free, mirroring the backend's `free_percent`, so a bogus
 * fetch can't render a nonsense percentage.
 */
export function lowSpaceFigures(availableBytes: number, totalBytes: number): { freeText: string; percentText: string } {
  const freePercent = totalBytes === 0 ? 100 : (availableBytes / totalBytes) * 100
  return {
    freeText: formatDriveSize(availableBytes, totalBytes),
    percentText: formatFreePercent(freePercent, 1),
  }
}
