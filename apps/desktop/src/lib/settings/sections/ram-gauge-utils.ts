import type { SystemMemoryInfo } from '$lib/tauri-commands'

export interface GaugeSegments {
  systemPercent: number
  otherAppsPercent: number
  retainedAiPercent: number
  addedPercent: number
  freedPercent: number
  totalProjectedUsageRatio: number
  systemBytes: number
  otherAppsBytes: number
  freeBytes: number
}

/**
 * Computes RAM gauge segment percentages from system memory info and AI memory estimates.
 * All segments sum to <= 100%; the bar background represents free memory.
 */
export function computeGaugeSegments(
  systemMemory: SystemMemoryInfo,
  currentAiMemoryBytes: number,
  projectedAiMemoryBytes: number,
): GaugeSegments | null {
  if (systemMemory.totalBytes === 0) return null

  const total = systemMemory.totalBytes
  const systemBytes = systemMemory.wiredBytes
  // Other apps = app memory reported by OS minus our AI estimate
  const otherAppsBytes = Math.max(0, systemMemory.appBytes - currentAiMemoryBytes)
  const freeBytes = systemMemory.freeBytes

  const systemPercent = (systemBytes / total) * 100
  const otherAppsPercent = (otherAppsBytes / total) * 100
  const delta = projectedAiMemoryBytes - currentAiMemoryBytes

  // When shrinking: split current AI into "retained" (projected) + "freed" (|delta|)
  // When growing: current AI stays, delta is added after it
  // When unchanged: just current AI
  let retainedAiPercent: number
  let addedPercent: number
  let freedPercent: number

  if (delta > 0) {
    retainedAiPercent = (currentAiMemoryBytes / total) * 100
    addedPercent = (delta / total) * 100
    freedPercent = 0
  } else if (delta < 0) {
    retainedAiPercent = (projectedAiMemoryBytes / total) * 100
    addedPercent = 0
    freedPercent = (Math.abs(delta) / total) * 100
  } else {
    retainedAiPercent = (currentAiMemoryBytes / total) * 100
    addedPercent = 0
    freedPercent = 0
  }

  // Clamp so segments never exceed 100% total
  const segmentTotal = systemPercent + otherAppsPercent + retainedAiPercent + addedPercent + freedPercent
  const scale = segmentTotal > 100 ? 100 / segmentTotal : 1

  const totalProjectedUsage = systemBytes + otherAppsBytes + projectedAiMemoryBytes

  return {
    systemPercent: systemPercent * scale,
    otherAppsPercent: otherAppsPercent * scale,
    retainedAiPercent: retainedAiPercent * scale,
    addedPercent: addedPercent * scale,
    freedPercent: freedPercent * scale,
    totalProjectedUsageRatio: totalProjectedUsage / total,
    systemBytes,
    otherAppsBytes,
    freeBytes,
  }
}
