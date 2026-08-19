import { describe, expect, it } from 'vitest'
import { getBadgeStatus, getFeatureStatus } from './feature-status'
import featureStatusData from '../../../../feature-status.json'

describe('feature-status', () => {
  it('the JSON has unique ids and valid statuses', () => {
    const ids = featureStatusData.features.map((feature) => feature.id)
    expect(new Set(ids).size).toBe(ids.length)
    const validStatuses = new Set(['alpha', 'beta', 'stable', 'planned'])
    for (const feature of featureStatusData.features) {
      expect(validStatuses.has(feature.status), `${feature.id} has status ${feature.status}`).toBe(true)
      expect(feature.name.length, `${feature.id} has a name`).toBeGreaterThan(0)
      expect(feature.note.length, `${feature.id} has a note`).toBeGreaterThan(0)
    }
  })

  it('the in-app alpha surfaces stay pinned to the JSON', () => {
    // The Selection dialog, the Ask Cmdr settings section, and the Image search
    // settings card wear ALPHA badges in the app; Search wears BETA. If you
    // graduate them in feature-status.json, this test reminds you the badges
    // change with it (that's the point of the single source of truth).
    expect(getFeatureStatus('search')).toBe('beta')
    expect(getFeatureStatus('select-files')).toBe('alpha')
    expect(getFeatureStatus('ask-cmdr')).toBe('alpha')
    expect(getFeatureStatus('image-search')).toBe('alpha')
  })

  it('getFeatureStatus returns undefined for unknown ids', () => {
    expect(getFeatureStatus('not-a-feature')).toBeUndefined()
  })

  it('getBadgeStatus maps alpha and beta to badges, everything else to none', () => {
    expect(getBadgeStatus('image-search')).toBe('alpha')
    expect(getBadgeStatus('network-drives')).toBe('beta')
    expect(getBadgeStatus('file-operations')).toBeUndefined() // stable: no badge by policy
    expect(getBadgeStatus('plugins')).toBeUndefined() // planned: no in-app surface
    expect(getBadgeStatus('not-a-feature')).toBeUndefined()
  })
})
