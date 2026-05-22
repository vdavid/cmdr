/**
 * D8: `deriveEnterAction` pins the eight-permutation table that decides which
 * footer/bar button owns the `⏎` shortcut at any moment. The discriminator is
 * `lastDialogEvent` × `resultsCount > 0`.
 */
import { describe, expect, it } from 'vitest'
import { deriveEnterAction, type LastDialogEvent } from './enter-action'

const events: LastDialogEvent[] = ['opened', 'results-arrived', 'cursor-moved', 'query-edited', 'filter-edited']

describe('deriveEnterAction (D8 ⏎ ownership)', () => {
  it('returns "run-search" whenever there are no results, regardless of last event', () => {
    for (const lastEvent of events) {
      expect(deriveEnterAction({ lastEvent, resultsCount: 0 })).toBe('run-search')
    }
  })

  it('returns "go-to-file" on results-arrived with results present', () => {
    expect(deriveEnterAction({ lastEvent: 'results-arrived', resultsCount: 3 })).toBe('go-to-file')
  })

  it('returns "go-to-file" on cursor-moved with results present', () => {
    expect(deriveEnterAction({ lastEvent: 'cursor-moved', resultsCount: 3 })).toBe('go-to-file')
  })

  it('returns "run-search" when last event was query-edited even with stale results', () => {
    // The user just changed the query; ⏎ should re-run, not jump to a stale row.
    expect(deriveEnterAction({ lastEvent: 'query-edited', resultsCount: 3 })).toBe('run-search')
  })

  it('returns "run-search" when last event was filter-edited even with stale results', () => {
    expect(deriveEnterAction({ lastEvent: 'filter-edited', resultsCount: 3 })).toBe('run-search')
  })

  it('returns "run-search" on a fresh "opened" event', () => {
    expect(deriveEnterAction({ lastEvent: 'opened', resultsCount: 3 })).toBe('run-search')
  })
})
