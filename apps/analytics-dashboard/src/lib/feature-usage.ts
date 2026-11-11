/**
 * Client-safe types and views for per-feature usage, from the api-server's `/admin/feature-usage`.
 * The feature → event mapping is NOT here: it lives in one place, beside the SQL that reads it
 * (`apps/api-server/src/admin/feature-usage.ts`, `featureUsageDefinitions`). The dashboard only renders
 * what comes back, so a new mapped event needs no change on this side.
 */

import { formatPercent } from './format.js'

/** One slice of a feature's events: an event name, or a property value like an Ask Cmdr `outcome`. */
export interface FeatureUsagePart {
  key: string
  installs: number
  events: number
}

/** Mirrors `FeatureUsage` in `apps/api-server/src/admin/feature-usage.ts`. */
export interface FeatureUsage {
  /** A `feature-status.json` id. */
  id: string
  name: string
  /** The mapping in words, like `pane_navigated[volume_kind=archive]`. */
  sources: string[]
  installs: number
  events: number
  /** Whether any of the feature's events has ever been stored; `false` reads "no data yet". */
  everSeen: boolean
  parts: FeatureUsagePart[]
}

/** Mirrors `FeatureUsageResponse` in `apps/api-server/src/admin/feature-usage.ts`. */
export interface FeatureUsageData {
  range: string
  since: string
  /** Installs that beat in the window: every share's denominator. */
  activeInstalls: number
  features: FeatureUsage[]
}

/** The label a feature with no stored events gets in place of a share. */
export const noDataYet = 'no data yet'

/**
 * The feature's installs as a share of the window's active installs, or "no data yet" when none of its
 * events has ever been stored (an event no release has shipped). A feature whose events exist but
 * nobody fired this window is a real 0%.
 */
export function formatFeatureShare(feature: FeatureUsage, activeInstalls: number): string {
  if (!feature.everSeen) return noDataYet
  if (activeInstalls === 0) return '–'
  return formatPercent(feature.installs / activeInstalls)
}

/** Most-used first (by installs, then events), keeping mapping order on ties; never-seen features last. */
export function sortFeatureUsage(features: FeatureUsage[]): FeatureUsage[] {
  return features
    .map((feature, index) => ({ feature, index }))
    .sort(
      (a, b) =>
        Number(b.feature.everSeen) - Number(a.feature.everSeen) ||
        b.feature.installs - a.feature.installs ||
        b.feature.events - a.feature.events ||
        a.index - b.index,
    )
    .map(({ feature }) => feature)
}

/** A feature's breakdown on one line, like "answered 2 (1 install), failed 1 (1 install)". */
export function formatParts(parts: FeatureUsagePart[]): string {
  return parts
    .map((part) => {
      const installs = `${part.installs.toLocaleString('en-US')} ${part.installs === 1 ? 'install' : 'installs'}`
      return `${part.key} ${part.events.toLocaleString('en-US')} (${installs})`
    })
    .join(', ')
}
