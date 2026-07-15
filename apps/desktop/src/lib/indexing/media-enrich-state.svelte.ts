/**
 * Reactive state for image-enrichment activity — the SECOND publisher on the top-right
 * indexing indicator (plan M5), alongside the drive indexer's `index-state.svelte`.
 *
 * A per-volume map keyed by `volumeId`, fed by the throttled `media-enrich-progress`
 * event and cleared / re-voiced by the `media-enrich-terminal` event. The corner
 * hourglass ORs `isAnyVolumeEnriching()` into its visibility gate, and the tooltip
 * renders an "Image indexing" row per actively-enriching volume.
 *
 * Mirrors `index-state.svelte`'s discipline: `$state` lives here (a `.svelte.ts` file),
 * a FRESH object is stored on every tick (so `SvelteMap` notifies — see the gotcha in
 * `index-state` DETAILS), a TERMINAL event (never freshness) clears the row, and init
 * follows "listen first, THEN query" so a pass already running at mount still renders.
 */

import { SvelteMap } from 'svelte/reactivity'
import {
  mediaIndexVolumeState,
  onMediaEnrichProgress,
  onMediaEnrichTerminal,
  type UnlistenFn,
} from '$lib/tauri-commands'
import { ROOT_VOLUME_ID } from './index-state.svelte'

/** A paused enrichment's reason, or `null` while it's actively working. `waitingForIdle`
 *  = a network pass yielded because the app is in use; `disconnected` = the volume went
 *  away mid-pass (resumes on reconnect). */
export type EnrichPauseState = 'waitingForIdle' | 'disconnected'

/** Live image-enrichment activity for one volume. */
export interface VolumeEnrichActivity {
  volumeId: string
  /** Subset images processed so far (the honest numerator). */
  done: number
  /** Total images in the enrichable subset (never the full walked set). */
  total: number
  bytesDone: number
  bytesTotal: number
  /** The paused reason, or `null` while actively enriching. A paused row is voiced but
   *  does NOT keep the corner hourglass up on its own (see `isAnyVolumeEnriching`). */
  paused: EnrichPauseState | null
  /** `Date.now()` when this volume's row first appeared, for the images/min + ETA clock. */
  startedAt: number
}

// Per-volume enrichment activity. An entry exists while a volume is actively enriching
// OR paused mid-pass; a terminal completion / cancel / failure removes it. Reactive.
const activity = new SvelteMap<string, VolumeEnrichActivity>()

// Event listener cleanup handles.
const unlistenHandles: UnlistenFn[] = []

/** Every volume with live enrichment activity (actively enriching or paused), in
 *  insertion order. Reactive. */
export function getEnrichingVolumes(): VolumeEnrichActivity[] {
  return [...activity.values()]
}

/** One volume's enrichment activity, or `undefined`. Reactive. */
export function getVolumeEnrichActivity(volumeId: string): VolumeEnrichActivity | undefined {
  return activity.get(volumeId)
}

/** Whether ANY volume is ACTIVELY enriching (not merely paused). The corner hourglass's
 *  visibility gate ORs this in; a paused-only volume doesn't light the hourglass on its
 *  own (so a disconnected NAS never pins it up forever), but its row still shows while the
 *  hourglass is up for another reason. Reactive. */
export function isAnyVolumeEnriching(): boolean {
  for (const a of activity.values()) {
    if (a.paused === null) return true
  }
  return false
}

/** Set up listeners for image-enrichment events, THEN seed from a snapshot. Call once at
 *  app mount. */
export async function initMediaEnrichState(): Promise<void> {
  const unlistenProgress = await onMediaEnrichProgress((payload) => {
    const prev = activity.get(payload.volumeId)
    // A FRESH object every tick so `SvelteMap.set` notifies (the fresh-object gotcha);
    // a progress tick always means "actively enriching", so it clears any paused flag
    // (the pass resumed). Preserve `startedAt` so the rate window keeps its clock.
    activity.set(payload.volumeId, {
      volumeId: payload.volumeId,
      done: payload.done,
      total: payload.total,
      bytesDone: payload.bytesDone,
      bytesTotal: payload.bytesTotal,
      paused: null,
      startedAt: prev?.startedAt ?? Date.now(),
    })
  })
  unlistenHandles.push(unlistenProgress)

  const unlistenTerminal = await onMediaEnrichTerminal((payload) => {
    const prev = activity.get(payload.volumeId)
    // Branch on the typed discriminant, never wording. Completion / cancel / failure
    // CLEAR the row; the two pause reasons re-voice it paused (so it never sticks at
    // "enriching"), keeping the last progress values.
    switch (payload.reason.kind) {
      case 'completed':
      case 'cancelled':
      case 'failed':
        activity.delete(payload.volumeId)
        break
      case 'pausedWaitingForIdle':
        if (prev) activity.set(payload.volumeId, { ...prev, paused: 'waitingForIdle' })
        break
      case 'pausedDisconnected':
        if (prev) activity.set(payload.volumeId, { ...prev, paused: 'disconnected' })
        break
    }
  })
  unlistenHandles.push(unlistenTerminal)

  // Listen-first-then-query (the module invariant): with M1, enrichment can start at
  // backend setup BEFORE the frontend mounts, so the pass-start event is lost. Seed the
  // root volume from its snapshot if it's enriching, so an in-flight pass renders at
  // mount. Root-only, mirroring `initIndexState`; network volumes hydrate from their next
  // progress tick. Skip if a progress event already seeded the entry.
  try {
    const state = await mediaIndexVolumeState(ROOT_VOLUME_ID)
    const total = state.coveredQualifyingCount ?? state.qualifyingCount ?? 0
    if (state.enabled && state.indexing && total > 0 && !activity.has(ROOT_VOLUME_ID)) {
      activity.set(ROOT_VOLUME_ID, {
        volumeId: ROOT_VOLUME_ID,
        done: Math.min(state.enrichedCount, total),
        total,
        // Bytes are unknown at snapshot; the next progress tick fills them in.
        bytesDone: 0,
        bytesTotal: 0,
        paused: null,
        startedAt: Date.now(),
      })
    }
  } catch {
    // Media index not ready / unavailable: no-op (the next progress event seeds it).
  }
}

/** Clean up all listeners. Call at app teardown. */
export function destroyMediaEnrichState(): void {
  for (const unlisten of unlistenHandles) {
    unlisten()
  }
  unlistenHandles.length = 0
  activity.clear()
}
