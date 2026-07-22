import { describe, it, expect } from 'vitest'
import { imageIndexDriveState, imageIndexDriveCoverage } from './image-index-drive-state'
import type { MediaIndexVolumeState } from '$lib/tauri-commands'
import type { VolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'

function volumeState(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: false,
    enrichedCount: 0,
    qualifyingCount: 100,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    waitingForImportance: false,
    coveredQualifyingCount: 100,
    keptCount: 0,
    ...overrides,
  }
}

function activity(overrides: Partial<VolumeEnrichActivity> = {}): VolumeEnrichActivity {
  return {
    volumeId: 'root',
    done: 10,
    total: 100,
    bytesDone: 0,
    bytesTotal: 0,
    paused: null,
    startedAt: Date.now(),
    ...overrides,
  }
}

describe('imageIndexDriveState', () => {
  it('is off when the master toggle is off', () => {
    expect(imageIndexDriveState({ enabled: false, volumeState: volumeState(), enrichActivity: undefined })).toBe('off')
  })

  it('is off when the volume is not image-index-enabled', () => {
    expect(
      imageIndexDriveState({ enabled: true, volumeState: volumeState({ enabled: false }), enrichActivity: undefined }),
    ).toBe('off')
  })

  it('is off when the volume has no honest total yet (both counts null)', () => {
    const vs = volumeState({ qualifyingCount: null, coveredQualifyingCount: null })
    expect(imageIndexDriveState({ enabled: true, volumeState: vs, enrichActivity: undefined })).toBe('off')
  })

  it('is indexing while a pass is actively enriching, even if counts already look complete', () => {
    const vs = volumeState({ enrichedCount: 100, coveredQualifyingCount: 100 })
    expect(imageIndexDriveState({ enabled: true, volumeState: vs, enrichActivity: activity() })).toBe('indexing')
  })

  it('is indexing when the covered set is not fully enriched and nothing is running', () => {
    const vs = volumeState({ enrichedCount: 40, coveredQualifyingCount: 100 })
    expect(imageIndexDriveState({ enabled: true, volumeState: vs, enrichActivity: undefined })).toBe('indexing')
  })

  it('is indexing when a pass is paused mid-way (work remains)', () => {
    const vs = volumeState({ enrichedCount: 40, coveredQualifyingCount: 100 })
    const paused = activity({ paused: 'disconnected' })
    expect(imageIndexDriveState({ enabled: true, volumeState: vs, enrichActivity: paused })).toBe('indexing')
  })

  it('is done when idle and every covered image is enriched', () => {
    const vs = volumeState({ enrichedCount: 100, coveredQualifyingCount: 100 })
    expect(imageIndexDriveState({ enabled: true, volumeState: vs, enrichActivity: undefined })).toBe('done')
  })

  it('reaches done against the COVERED denominator, not the whole-volume qualifying total', () => {
    // A narrow scope: 100 qualify volume-wide, only 30 fall in covered folders, all enriched.
    const vs = volumeState({ enrichedCount: 30, qualifyingCount: 100, coveredQualifyingCount: 30 })
    expect(imageIndexDriveState({ enabled: true, volumeState: vs, enrichActivity: undefined })).toBe('done')
  })

  it('falls back to qualifyingCount when coveredQualifyingCount is null', () => {
    const vs = volumeState({ enrichedCount: 50, qualifyingCount: 100, coveredQualifyingCount: null })
    expect(imageIndexDriveState({ enabled: true, volumeState: vs, enrichActivity: undefined })).toBe('indexing')
  })
})

describe('imageIndexDriveCoverage', () => {
  it('returns null when there is no honest total', () => {
    expect(imageIndexDriveCoverage(volumeState({ qualifyingCount: null, coveredQualifyingCount: null }))).toBeNull()
  })

  it('prefers the covered denominator and clamps done to total', () => {
    const vs = volumeState({ enrichedCount: 250, qualifyingCount: 300, coveredQualifyingCount: 120 })
    expect(imageIndexDriveCoverage(vs)).toEqual({ done: 120, total: 120 })
  })

  it('reports the enriched numerator when below the total', () => {
    const vs = volumeState({ enrichedCount: 42, coveredQualifyingCount: 120 })
    expect(imageIndexDriveCoverage(vs)).toEqual({ done: 42, total: 120 })
  })
})
