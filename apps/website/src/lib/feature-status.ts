/**
 * Typed loader for the repo-root `feature-status.json`, the single source of truth
 * for per-feature stability. See `docs/feature-status.md` for the schema and the
 * status semantics. Importing root-level files into Astro is established by the
 * changelog page's `?raw` import of `CHANGELOG.md`; a plain JSON import bundles
 * the same way at build time.
 */
import featureStatusData from '../../../../feature-status.json'

export type FeatureStatus = 'alpha' | 'beta' | 'stable' | 'planned'

export interface Feature {
  id: string
  name: string
  status: FeatureStatus
  note: string
  issueUrl?: string
}

export const features: Feature[] = featureStatusData.features as Feature[]

/** Pill labels: the capitalized status name. Every status gets a pill on the website. */
export const statusToBadgeLabelMap: Record<FeatureStatus, string> = {
  alpha: 'Alpha',
  beta: 'Beta',
  stable: 'Stable',
  planned: 'Planned',
}

/** Canonical user-facing explanations, shared with the app's badge tooltips via the JSON. */
export const statusToDefinitionMap: Record<FeatureStatus, string> = featureStatusData.statusDefinitions as Record<
  FeatureStatus,
  string
>

export function getFeature(id: string): Feature | undefined {
  return features.find((feature) => feature.id === id)
}

/** The pill tooltip: "Alpha: Fresh feature. Should work. Might be broken." */
export function getStatusTooltip(status: FeatureStatus): string {
  return `${statusToBadgeLabelMap[status]}: ${statusToDefinitionMap[status]}`
}
