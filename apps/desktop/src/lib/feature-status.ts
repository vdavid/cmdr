/**
 * Typed accessor for the repo-root `feature-status.json`, the single source of
 * truth for per-feature stability (shared with the website). See
 * `docs/feature-status.md` for the schema and the status semantics.
 *
 * The JSON import resolves at compile time (Vite inlines it into the bundle).
 * In dev, Vite's `server.fs.allow` already covers the repo root because the
 * pnpm workspace root is the repo root.
 */
import featureStatusData from '../../../../feature-status.json'

export type FeatureStatus = 'alpha' | 'beta' | 'stable' | 'planned'

/** Statuses that render a badge in the app. Stable and planned never do. */
export type BadgeStatus = 'alpha' | 'beta'

interface Feature {
  id: string
  name: string
  status: FeatureStatus
  note: string
  issueUrl?: string
}

const features: Feature[] = featureStatusData.features as Feature[]

const idToStatusMap = new Map<string, FeatureStatus>(features.map((feature) => [feature.id, feature.status]))

/** The feature's status, or undefined for an unknown id. */
export function getFeatureStatus(id: string): FeatureStatus | undefined {
  return idToStatusMap.get(id)
}

/**
 * The badge the app should render for a feature: 'alpha' or 'beta', or undefined
 * when no badge belongs (stable features carry no badge by policy; planned
 * features have no in-app surface; unknown ids stay silent).
 */
export function getBadgeStatus(id: string): BadgeStatus | undefined {
  const status = idToStatusMap.get(id)
  return status === 'alpha' || status === 'beta' ? status : undefined
}
