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

/** Card-badge labels for the feature cards (Features component + features page). */
export const statusToBadgeLabelMap: Record<FeatureStatus, string | null> = {
  alpha: 'Works but early stage',
  beta: 'Beta',
  stable: null, // Stable features carry no badge, anywhere.
  planned: 'Coming soon',
}

/** Section headings + intros for the feature status page, in display order. */
export const statusOrder: FeatureStatus[] = ['alpha', 'beta', 'stable', 'planned']

export function getFeature(id: string): Feature | undefined {
  return features.find((feature) => feature.id === id)
}

/** The badge label for a feature, or null when it shouldn't carry one (stable, unknown id). */
export function getBadgeLabel(id: string): string | null {
  const feature = getFeature(id)
  return feature ? statusToBadgeLabelMap[feature.status] : null
}
