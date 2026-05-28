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

export interface SearchExtrasState {
  getScope(): string
  setScope(value: string): void
  getExcludeSystemDirs(): boolean
  setExcludeSystemDirs(value: boolean): void

  // Index lifecycle. Lives here (Search-only) because Selection has no whole-drive index.
  getIsIndexReady(): boolean
  setIsIndexReady(value: boolean): void
  getIndexEntryCount(): number
  setIndexEntryCount(value: number): void
  getIsIndexAvailable(): boolean
  setIsIndexAvailable(value: boolean): void

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
  /** Resets every extras field to defaults. Paired with `clearCore()` on the core. */
  clearExtras(): void
}

export function createSearchExtrasState(): SearchExtrasState {
  let scope = $state('')
  let excludeSystemDirs = $state(true)
  let isIndexReady = $state(false)
  let indexEntryCount = $state(0)
  let isIndexAvailable = $state(true)
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

    getIsIndexReady: () => isIndexReady,
    setIsIndexReady: (v) => {
      isIndexReady = v
    },
    getIndexEntryCount: () => indexEntryCount,
    setIndexEntryCount: (v) => {
      indexEntryCount = v
    },
    getIsIndexAvailable: () => isIndexAvailable,
    setIsIndexAvailable: (v) => {
      isIndexAvailable = v
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
      isIndexReady = false
      indexEntryCount = 0
      isIndexAvailable = true
      lastAiLabel = null
      lastAiPattern = null
      lastAiPatternKind = null
    },
  }
}
