/**
 * The search analytics vocabulary: buckets, the ending map, and the prop set.
 *
 * The PII rule is the load-bearing one here — a props object that ever carried a
 * query, a pattern, or a path would ship it to PostHog — so the last test asserts
 * over the VALUES rather than the keys.
 */

import { describe, it, expect } from 'vitest'
import { durationBucket, endingOf, searchUsedProps, type SearchRunFacts } from './search-analytics'
import type { SearchRunCoverage } from '$lib/tauri-commands'

function coverage(overrides: Partial<SearchRunCoverage> = {}): SearchRunCoverage {
  return {
    walk: 'completed',
    kind: 'live',
    permissionDenied: [],
    declined: [],
    stillCovering: [],
    unresolvedScopes: [],
    abandonedGround: false,
    capped: false,
    targetVolumeId: 'root',
    ...overrides,
  }
}

function facts(overrides: Partial<SearchRunFacts> = {}): SearchRunFacts {
  return {
    mode: 'filename',
    trigger: 'run',
    ending: 'completed',
    coverage: 'live',
    durationMs: 2000,
    abandonedGround: false,
    capped: false,
    ...overrides,
  }
}

describe('durationBucket', () => {
  it('names each step where the waiting experience changes', () => {
    expect(durationBucket(0)).toBe('<1s')
    expect(durationBucket(999)).toBe('<1s')
    expect(durationBucket(1000)).toBe('1-5s')
    expect(durationBucket(4999)).toBe('1-5s')
    expect(durationBucket(5000)).toBe('5-30s')
    expect(durationBucket(29_999)).toBe('5-30s')
    expect(durationBucket(30_000)).toBe('30s-2m')
    expect(durationBucket(119_999)).toBe('30s-2m')
    expect(durationBucket(120_000)).toBe('2m+')
  })
})

describe('endingOf', () => {
  it('keeps cancel and disconnect apart, and calls the rest completed', () => {
    expect(endingOf(coverage({ walk: 'cancelled' }))).toBe('cancelled')
    expect(endingOf(coverage({ walk: 'interrupted' }))).toBe('interrupted')
    expect(endingOf(coverage({ walk: 'completed' }))).toBe('completed')
    // Nothing to walk is a run that finished; the coverage prop says it needed no walk.
    expect(endingOf(coverage({ walk: 'nothingToWalk' }))).toBe('completed')
  })

  it('reports a walk that finished having abandoned folders as completed', () => {
    // `abandonedGround` is its own prop precisely because it's true ALONGSIDE an
    // ending rather than instead of one.
    expect(endingOf(coverage({ walk: 'completed', abandonedGround: true }))).toBe('completed')
  })
})

describe('searchUsedProps', () => {
  it('carries the run facts, with the duration as a bucket', () => {
    expect(searchUsedProps(facts({ durationMs: 7000 }))).toEqual({
      mode: 'filename',
      trigger: 'run',
      ending: 'completed',
      coverage: 'live',
      abandoned_ground: false,
      capped: false,
      duration_bucket: '5-30s',
    })
  })

  it('omits the duration entirely when the run was not timed', () => {
    // An index-only answer arrives inside one promise; timing it would measure
    // the IPC round trip rather than a wait anybody felt.
    expect(searchUsedProps(facts({ trigger: 'autoApply', durationMs: null }))).not.toHaveProperty('duration_bucket')
  })

  it('emits nothing but categorical values, whatever it is handed', () => {
    const props = searchUsedProps(
      facts({ mode: 'ai', trigger: 'run', ending: 'superseded', coverage: 'unknown', durationMs: 300_000 }),
    )
    const allowed = new Set([
      'filename',
      'ai',
      'run',
      'autoApply',
      'completed',
      'interrupted',
      'cancelled',
      'superseded',
      'covered',
      'live',
      'mixed',
      'unknown',
      '<1s',
      '1-5s',
      '5-30s',
      '30s-2m',
      '2m+',
    ])
    for (const value of Object.values(props)) {
      if (typeof value === 'boolean') continue
      expect(allowed.has(value)).toBe(true)
    }
  })
})
