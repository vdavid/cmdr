/**
 * Client-safe: Umami's per-site referrer rows (`{ x: domain, y: visitors }`) shaped into the Awareness
 * section's referrer pies (`DonutChart.svelte`), through `foldSlices` so they fold and color like every
 * other pie. A site's mobile and link-shim hosts (`m.facebook.com`, `l.facebook.com`) merge into one slice.
 */
import { foldSlices, type PieInput, type PieSlice } from './pie-slices.js'

/** The key of an empty referrer: neutral, so it wears the unknown grey and doesn't take a hue. */
const directKey = '(none)'

/** Host prefixes that mean "the same site": www, mobile, and Facebook's outbound-link shims. */
const variantPrefixes = ['www.', 'm.', 'l.', 'lm.', 'mobile.']

/** `m.facebook.com` → `facebook.com`; a host that would drop to one label (`m.com`) stays as it is. */
function siteOf(host: string): string {
  for (const prefix of variantPrefixes) {
    const rest = host.slice(prefix.length)
    if (host.startsWith(prefix) && rest.includes('.')) return siteOf(rest)
  }
  return host
}

export function referrerSlices(items: Array<{ x: string; y: number }>): PieSlice[] {
  const bySite = new Map<string, PieInput[]>()
  for (const item of items) {
    if (item.y <= 0) continue
    const host = item.x.toLowerCase()
    const site = host ? siteOf(host) : directKey
    const members = bySite.get(site) ?? []
    members.push({ key: host || directKey, label: host || '(direct)', count: item.y })
    bySite.set(site, members)
  }
  const inputs: PieInput[] = [...bySite].map(([site, members]) => ({
    key: site,
    label: site === directKey ? '(direct)' : site,
    count: members.reduce((sum, m) => sum + m.count, 0),
    ...(members.length > 1 && { members: members.sort((a, b) => b.count - a.count) }),
  }))
  return foldSlices(inputs, { neutral: [directKey] })
}
