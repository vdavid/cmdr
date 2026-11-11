import { describe, expect, it } from 'vitest'
import { formatFeatureShare, formatParts, noDataYet, sortFeatureUsage, type FeatureUsage } from './feature-usage.js'

function feature(id: string, installs: number, events: number, everSeen = true): FeatureUsage {
  return { id, name: id, sources: [], installs, events, everSeen, parts: [] }
}

describe('formatFeatureShare', () => {
  it('reads an event no release has sent as "no data yet", never as 0%', () => {
    expect(formatFeatureShare(feature('multi-rename', 0, 0, false), 100)).toBe(noDataYet)
  })

  it('reads a shipped event nobody fired as a real 0%', () => {
    expect(formatFeatureShare(feature('tabs', 0, 0), 100)).toBe('0%')
  })

  it('shows a dash when there are no active installs to divide by', () => {
    expect(formatFeatureShare(feature('search', 0, 0), 0)).toBe('–')
  })

  it('shares installs over active installs', () => {
    expect(formatFeatureShare(feature('search', 25, 90), 100)).toBe('25%')
  })
})

describe('sortFeatureUsage', () => {
  it('puts the most-used first, keeps mapping order on ties, and never-seen features last', () => {
    const sorted = sortFeatureUsage([
      feature('never', 0, 0, false),
      feature('tie-a', 2, 5),
      feature('top', 9, 1),
      feature('tie-b', 2, 5),
      feature('zero', 0, 0),
    ])
    expect(sorted.map((f) => f.id)).toEqual(['top', 'tie-a', 'tie-b', 'zero', 'never'])
  })
})

describe('formatParts', () => {
  it('names each part with its events and installs', () => {
    expect(
      formatParts([
        { key: 'answered', installs: 3, events: 1_200 },
        { key: 'failed', installs: 1, events: 2 },
      ]),
    ).toBe('answered 1,200 (3 installs), failed 2 (1 install)')
  })
})
