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

  it('the in-app badge surfaces stay pinned to the JSON', () => {
    // The Image search settings card wears ALPHA; Search, Ask Cmdr (rail and settings), and
    // the Operation log dialog wear BETA; the Selection dialog and the Multi-rename sheet are
    // stable, so they wear nothing. The E2E specs `ask-cmdr.spec.ts` and `operation-log.spec.ts`
    // assert the rendered badge text too. If you change a status in feature-status.json, this
    // test reminds you the badges change with it (that's the point of the single source of truth).
    expect(getFeatureStatus('image-search')).toBe('alpha')
    expect(getFeatureStatus('search')).toBe('beta')
    expect(getFeatureStatus('ask-cmdr')).toBe('beta')
    expect(getFeatureStatus('operation-log')).toBe('beta')
    expect(getFeatureStatus('select-files')).toBe('stable')
    expect(getFeatureStatus('multi-rename')).toBe('stable')
  })

  it('getFeatureStatus returns undefined for unknown ids', () => {
    expect(getFeatureStatus('not-a-feature')).toBeUndefined()
  })

  it('getBadgeStatus maps alpha and beta to badges, everything else to none', () => {
    expect(getBadgeStatus('image-search')).toBe('alpha')
    expect(getBadgeStatus('webdav')).toBe('beta')
    expect(getBadgeStatus('file-operations')).toBeUndefined() // stable: no badge by policy
    expect(getBadgeStatus('select-files')).toBeUndefined()
    expect(getBadgeStatus('plugins')).toBeUndefined() // planned: no in-app surface
    expect(getBadgeStatus('not-a-feature')).toBeUndefined()
  })
})
