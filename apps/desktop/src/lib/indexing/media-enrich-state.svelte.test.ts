/**
 * Unit tests for the image-enrichment activity store (`media-enrich-state.svelte`),
 * the second publisher on the top-right indexing indicator.
 *
 * Mirrors `index-state.svelte.test.ts`: mock the typed event wrappers to capture the
 * registered callbacks, fire them directly, and read the reactive getters. Pins the
 * terminal-clears-the-row rule, the paused re-voicing, listen-first-then-query seeding,
 * and the fresh-object reactivity the frozen-counter bug would drop.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { flushSync } from 'svelte'
import type {
  MediaEnrichProgressEvent,
  MediaEnrichTerminalEvent,
  MediaEnrichTerminalReason,
  MediaIndexVolumeState,
} from '$lib/ipc/bindings'

let progressCb: ((p: MediaEnrichProgressEvent) => void) | undefined
let terminalCb: ((p: MediaEnrichTerminalEvent) => void) | undefined

const noopUnlisten = () => {}

// The snapshot the store queries at init (listen-first-then-query). Tests set it before
// calling `initMediaEnrichState`.
let volumeStateSnapshot: MediaIndexVolumeState = notIndexing()

function notIndexing(): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: false,
    enrichedCount: 0,
    qualifyingCount: 0,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    waitingForImportance: false,
    coveredQualifyingCount: 0,
    keptCount: 0,
  }
}

vi.mock('$lib/tauri-commands', () => ({
  onMediaEnrichProgress: (cb: (p: MediaEnrichProgressEvent) => void) => {
    progressCb = cb
    return Promise.resolve(noopUnlisten)
  },
  onMediaEnrichTerminal: (cb: (p: MediaEnrichTerminalEvent) => void) => {
    terminalCb = cb
    return Promise.resolve(noopUnlisten)
  },
  mediaIndexVolumeState: () => Promise.resolve(volumeStateSnapshot),
}))

import {
  initMediaEnrichState,
  destroyMediaEnrichState,
  getEnrichingVolumes,
  getVolumeEnrichActivity,
  isAnyVolumeEnriching,
} from './media-enrich-state.svelte'

function emitProgress(volumeId: string, done: number, total: number, bytesDone = 0, bytesTotal = 0): void {
  if (!progressCb) throw new Error('progress callback not registered')
  progressCb({ volumeId, done, total, bytesDone, bytesTotal })
}

function emitTerminal(volumeId: string, reason: MediaEnrichTerminalReason): void {
  if (!terminalCb) throw new Error('terminal callback not registered')
  terminalCb({ volumeId, reason })
}

describe('media-enrich-state', () => {
  beforeEach(async () => {
    destroyMediaEnrichState()
    progressCb = undefined
    terminalCb = undefined
    volumeStateSnapshot = notIndexing()
    await initMediaEnrichState()
  })

  it('creates a per-volume row on a progress event', () => {
    emitProgress('root', 40, 100, 4_000, 10_000)
    expect(getVolumeEnrichActivity('root')).toMatchObject({
      done: 40,
      total: 100,
      bytesDone: 4_000,
      bytesTotal: 10_000,
      paused: null,
    })
    expect(isAnyVolumeEnriching()).toBe(true)
    expect(getEnrichingVolumes().map((a) => a.volumeId)).toEqual(['root'])
  })

  it('keeps each volume independent', () => {
    emitProgress('root', 10, 100)
    emitProgress('smb-nas', 5, 50)
    emitProgress('root', 90, 100)
    expect(getVolumeEnrichActivity('root')).toMatchObject({ done: 90 })
    expect(getVolumeEnrichActivity('smb-nas')).toMatchObject({ done: 5, total: 50 })
  })

  it('clears the row on a completed terminal (never sticks at enriching)', () => {
    emitProgress('root', 100, 100)
    emitTerminal('root', { kind: 'completed', enriched: 100, gcCount: 2 })
    expect(getVolumeEnrichActivity('root')).toBeUndefined()
    expect(isAnyVolumeEnriching()).toBe(false)
  })

  it('clears the row on a cancelled or failed terminal', () => {
    emitProgress('root', 10, 100)
    emitTerminal('root', { kind: 'cancelled' })
    expect(getVolumeEnrichActivity('root')).toBeUndefined()

    emitProgress('smb-nas', 3, 50)
    emitTerminal('smb-nas', { kind: 'failed' })
    expect(getVolumeEnrichActivity('smb-nas')).toBeUndefined()
  })

  it('re-voices a paused-disconnected row instead of clearing it', () => {
    emitProgress('smb-nas', 20, 50, 2_000, 5_000)
    emitTerminal('smb-nas', { kind: 'pausedDisconnected' })
    // The row survives (voices "paused, resumes on reconnect"), keeping its last counts.
    expect(getVolumeEnrichActivity('smb-nas')).toMatchObject({ done: 20, total: 50, paused: 'disconnected' })
    // ...but a paused-only volume does NOT count as actively enriching (no forever-hourglass).
    expect(isAnyVolumeEnriching()).toBe(false)
  })

  it('re-voices a paused-waiting-for-idle row', () => {
    emitProgress('smb-nas', 5, 50)
    emitTerminal('smb-nas', { kind: 'pausedWaitingForIdle' })
    expect(getVolumeEnrichActivity('smb-nas')).toMatchObject({ paused: 'waitingForIdle' })
    expect(isAnyVolumeEnriching()).toBe(false)
  })

  it('clears the paused flag when a pass resumes (a fresh progress tick)', () => {
    emitProgress('smb-nas', 5, 50)
    emitTerminal('smb-nas', { kind: 'pausedWaitingForIdle' })
    expect(getVolumeEnrichActivity('smb-nas')?.paused).toBe('waitingForIdle')
    emitProgress('smb-nas', 6, 50)
    expect(getVolumeEnrichActivity('smb-nas')?.paused).toBeNull()
    expect(isAnyVolumeEnriching()).toBe(true)
  })

  it('a terminal for an unknown volume is a no-op', () => {
    emitTerminal('never-seen', { kind: 'pausedDisconnected' })
    expect(getVolumeEnrichActivity('never-seen')).toBeUndefined()
  })
})

describe('media-enrich-state listen-first-then-query seeding', () => {
  beforeEach(() => {
    destroyMediaEnrichState()
    progressCb = undefined
    terminalCb = undefined
  })

  it('seeds the root row from the snapshot when a pass is already running at mount', async () => {
    // A pass started at backend setup before the frontend mounted, so the pass-start
    // event was lost; the snapshot query recovers it.
    volumeStateSnapshot = { ...notIndexing(), indexing: true, enrichedCount: 1_200, coveredQualifyingCount: 5_000 }
    await initMediaEnrichState()
    expect(getVolumeEnrichActivity('root')).toMatchObject({ done: 1_200, total: 5_000, paused: null })
    expect(isAnyVolumeEnriching()).toBe(true)
  })

  it('does not seed a row when nothing is enriching at mount', async () => {
    volumeStateSnapshot = { ...notIndexing(), indexing: false }
    await initMediaEnrichState()
    expect(getVolumeEnrichActivity('root')).toBeUndefined()
  })
})

// Guards the live-counter REACTIVITY (not just the stored value): a real `$effect` over
// the getter must re-fire on the SECOND progress tick — the notification a mutate-in-place
// `SvelteMap.set` would drop (the fresh-object gotcha shared with `index-state`).
describe('media-enrich-state reactivity', () => {
  beforeEach(async () => {
    destroyMediaEnrichState()
    progressCb = undefined
    terminalCb = undefined
    volumeStateSnapshot = notIndexing()
    await initMediaEnrichState()
  })

  it('re-fires reactive consumers on every progress tick', () => {
    const seen: number[] = []
    const cleanup = $effect.root(() => {
      $effect(() => {
        seen.push(getVolumeEnrichActivity('root')?.done ?? -1)
      })
    })
    flushSync()
    emitProgress('root', 100, 1_000)
    flushSync()
    emitProgress('root', 950, 1_000)
    flushSync()
    cleanup()
    expect(seen).toEqual([-1, 100, 950])
  })
})
