/**
 * The overall "~X left" on a drive row: the active step's live estimate plus the
 * backend's remembered time for the steps after it. The backend owns the sum and
 * its honesty gate (`lifecycle/steps_ahead.rs`); these pin the display rules on
 * top: which entry belongs to which step, and when the row says nothing at all.
 */
import { describe, it, expect } from 'vitest'
import { deriveOverallEta } from './overall-eta'
import type { StepsAheadMs } from '$lib/ipc/bindings'

const remembered: StepsAheadMs = {
  findFiles: 61_000,
  saveFileList: 21_000,
  computeFolderSizes: 2_000,
  catchUp: 0,
}

describe('deriveOverallEta', () => {
  it('adds the remembered steps ahead to the active step’s live estimate', () => {
    expect(deriveOverallEta('findFiles', 180, remembered)).toEqual({ kind: 'known', seconds: 241 })
    expect(deriveOverallEta('computeFolderSizes', 10, remembered)).toEqual({ kind: 'known', seconds: 12 })
  })

  it('reads a change check’s update step from the save entry', () => {
    expect(deriveOverallEta('updateFileList', 30, remembered)).toEqual({ kind: 'known', seconds: 51 })
    expect(deriveOverallEta('saveFileList', 30, remembered)).toEqual({ kind: 'known', seconds: 51 })
  })

  it('shows nothing when a step ahead has no history', () => {
    const gap: StepsAheadMs = { ...remembered, findFiles: null }
    expect(deriveOverallEta('findFiles', 180, gap)).toEqual({ kind: 'none' })
  })

  it('shows nothing for a run with no plan (a first index, or a reload that missed it)', () => {
    expect(deriveOverallEta('findFiles', 180, undefined)).toEqual({ kind: 'none' })
  })

  it('shows nothing for an event-log roll-on: its one step already says it', () => {
    expect(deriveOverallEta('updateIndex', 5, remembered)).toEqual({ kind: 'none' })
  })

  it('keeps its line while the active step has no estimate yet and work is still ahead', () => {
    // The tooltip measures its height once, so the line holds its place rather
    // than appearing a second later.
    expect(deriveOverallEta('computeFolderSizes', null, { ...remembered, computeFolderSizes: 2_000 })).toEqual({
      kind: 'estimating',
    })
  })

  it('says nothing on the last step without an estimate: there is nothing to add', () => {
    expect(deriveOverallEta('catchUp', null, remembered)).toEqual({ kind: 'none' })
  })

  it('shows nothing once every step is done', () => {
    expect(deriveOverallEta(undefined, null, remembered)).toEqual({ kind: 'none' })
  })
})
