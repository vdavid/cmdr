/**
 * Client-safe: Umami's per-site referrer rows (`{ x: domain, y: visitors }`) shaped into the Awareness
 * section's referrer pies (`DonutChart.svelte`), through `foldSlices` so they fold and color like every
 * other pie.
 */
import { foldSlices, type PieSlice } from './pie-slices.js'

/** The key of an empty referrer: neutral, so it wears the unknown grey and doesn't take a hue. */
const directKey = '(none)'

export function referrerSlices(items: Array<{ x: string; y: number }>): PieSlice[] {
  return foldSlices(
    items.map((i) => ({ key: i.x || directKey, label: i.x || '(direct)', count: i.y })),
    { neutral: [directKey] },
  )
}
