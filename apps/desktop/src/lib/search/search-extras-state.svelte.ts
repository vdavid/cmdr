/**
 * Search-only state alongside the cross-consumer core (see
 * `lib/query-ui/query-filter-state.svelte.ts`).
 *
 * This module carries Search-only fields (`scope`, `excludeSystemDirs`, `lastAiLabel`,
 * `lastAiPattern`, `lastAiPatternKind`) so Selection's instance doesn't lug fields it
 * would never read. The Search wrapper composes a core instance + this extras instance.
 *
 * The AI-pattern + label setter (`recordAiPatternAndLabel`) is called by the Search
 * wrapper right after `core.recordAiTranslation(...)`, completing the contract that
 * lived in the old single function. Two calls in sequence from Search; one core call
 * from Selection (no Pattern chip, no AI label breadcrumb).
 */

import { SvelteMap } from 'svelte/reactivity'
import type { CoverageNote } from './coverage-note'

export interface SearchExtrasState {
  getScope(): string
  setScope(value: string): void
  getExcludeSystemDirs(): boolean
  setExcludeSystemDirs(value: boolean): void
  /** Count-only mode: run the search and show just the total, no result list. */
  getCountOnly(): boolean
  setCountOnly(value: boolean): void

  // Index lifecycle. Lives here (Search-only) because Selection has no whole-drive index.
  // Readiness is PER VOLUME: a search covers one volume, so "ready" is only ever true
  // of a particular one, and the dialog waits for its target rather than for root.
  /** Whether that volume's search arena has landed. Reactive per volume id. */
  isVolumeReady(volumeId: string): boolean
  /** That volume's indexed entry count, or 0 when its arena hasn't landed. */
  getVolumeEntryCount(volumeId: string): number
  /** Records a landed arena (the `search-index-ready` event, or a warm pre-load). */
  markVolumeReady(volumeId: string, entryCount: number): void
  /** The volume a background pre-load is in flight for; `null` when nothing is coming. */
  getPendingVolumeId(): string | null
  setPendingVolumeId(volumeId: string | null): void
  getIsIndexAvailable(): boolean
  setIsIndexAvailable(value: boolean): void

  /** What the last run couldn't cover, or `null` when it covered everything asked of it. */
  getCoverageNote(): CoverageNote | null
  setCoverageNote(value: CoverageNote | null): void

  getLastAiLabel(): string | null
  getLastAiPattern(): string | null
  getLastAiPatternKind(): 'glob' | 'regex' | null
  /** Wipes only the AI pattern + kind. Used by the Pattern chip's clear button. */
  clearAiPattern(): void
  /**
   * Stores the LLM-produced pattern + kind + label. Search calls this right after
   * `core.recordAiTranslation({pattern, kind})` so the Pattern chip and the snapshot
   * breadcrumb both see the fresh values. Selection doesn't need this; it never
   * surfaces an AI pattern or an AI label.
   */
  recordAiPatternAndLabel(input: { pattern: string | null; kind: 'glob' | 'regex' | null; label: string | null }): void
  /**
   * Resets what the USER typed, back to defaults. Paired with `clearCore()` on the
   * core; ⌘N is the one sanctioned caller.
   *
   * ❌ It deliberately leaves what the MACHINE reported (which arenas have landed,
   * whether the backend is available) alone: those describe this dialog session, not
   * the query. Wiping readiness here meant the next search silently did nothing,
   * because the gate went back to "waiting" and no second `search-index-ready` was
   * ever coming.
   */
  clearExtras(): void
}

export function createSearchExtrasState(): SearchExtrasState {
  let scope = $state('')
  let excludeSystemDirs = $state(true)
  let countOnly = $state(false)
  /**
   * Volume id → indexed entry count for every arena that has landed. One mutated
   * `SvelteMap` for the instance's whole life: it's reactive per key, so a reader
   * asking about one volume doesn't re-run when another lands, and swapping in a
   * fresh map would leave every reader subscribed to the old one.
   */
  const readyVolumes = new SvelteMap<string, number>()
  let pendingVolumeId = $state<string | null>(null)
  let isIndexAvailable = $state(true)
  let coverageNote = $state<CoverageNote | null>(null)
  let lastAiLabel = $state<string | null>(null)
  let lastAiPattern = $state<string | null>(null)
  let lastAiPatternKind = $state<'glob' | 'regex' | null>(null)

  return {
    getScope: () => scope,
    setScope: (v) => {
      scope = v
    },
    getExcludeSystemDirs: () => excludeSystemDirs,
    setExcludeSystemDirs: (v) => {
      excludeSystemDirs = v
    },
    getCountOnly: () => countOnly,
    setCountOnly: (v) => {
      countOnly = v
    },

    isVolumeReady: (volumeId) => readyVolumes.has(volumeId),
    getVolumeEntryCount: (volumeId) => readyVolumes.get(volumeId) ?? 0,
    markVolumeReady: (volumeId, entryCount) => {
      readyVolumes.set(volumeId, entryCount)
    },
    getPendingVolumeId: () => pendingVolumeId,
    setPendingVolumeId: (v) => {
      pendingVolumeId = v
    },
    getIsIndexAvailable: () => isIndexAvailable,
    setIsIndexAvailable: (v) => {
      isIndexAvailable = v
    },

    getCoverageNote: () => coverageNote,
    setCoverageNote: (v) => {
      coverageNote = v
    },

    getLastAiLabel: () => lastAiLabel,
    getLastAiPattern: () => lastAiPattern,
    getLastAiPatternKind: () => lastAiPatternKind,

    clearAiPattern: () => {
      lastAiPattern = null
      lastAiPatternKind = null
    },

    recordAiPatternAndLabel: (input) => {
      lastAiPattern = input.pattern
      lastAiPatternKind = input.pattern ? input.kind : null
      lastAiLabel = input.label
    },

    clearExtras: () => {
      scope = ''
      excludeSystemDirs = true
      countOnly = false
      coverageNote = null
      lastAiLabel = null
      lastAiPattern = null
      lastAiPatternKind = null
    },
  }
}
