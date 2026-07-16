/**
 * Façade over the cross-consumer core + Search-only extras split:
 *   - `lib/query-ui/query-filter-state.svelte.ts`: the cross-consumer core
 *     (factory `createQueryFilterState`).
 *   - `lib/search/search-extras-state.svelte.ts`: Search-only fields
 *     (factory `createSearchExtrasState`).
 *   - `lib/search/build-search-query.ts`: the `SearchQuery` IPC payload builder.
 *
 * This file instantiates one of each and re-exports a flat named-function API so
 * the Search call sites stay shallow. Transparent passthrough: no branching, no
 * shape adaptation, just delegation.
 *
 * The `recordAiTranslation({pattern, kind, label})` shape is the one place this
 * façade fans a single call out across both factories: it forwards to
 * `core.recordAiTranslation({pattern, kind})` and
 * `extras.recordAiPatternAndLabel({pattern, kind, label})`. The Selection wrapper
 * calls only the core directly and never reaches this file.
 */

import type { SearchResultEntry, SearchQuery, HistoryEntry, HistoryFilters } from '$lib/tauri-commands'
import { createQueryFilterState } from '$lib/query-ui/query-filter-state.svelte'
import { createSearchExtrasState } from './search-extras-state.svelte'
import { buildSearchQuery as buildSearchQueryImpl } from './build-search-query'

// Re-export the core's pure helpers + types so the call sites stay stable.
export {
  parseSizeToBytes,
  parseDateToTimestamp,
  deriveEnterAction,
  SEARCH_AUTO_APPLY_DEBOUNCE_MS,
} from '$lib/query-ui/query-filter-state.svelte'
export type {
  SearchMode,
  SizeFilter,
  SizeUnit,
  DateFilter,
  TypeFilter,
  PatternType,
  LastDialogEvent,
  EnterAction,
} from '$lib/query-ui/query-filter-state.svelte'

const core = createQueryFilterState({ defaultMode: 'filename' })
const extras = createSearchExtrasState()

/**
 * The underlying core state instance. Exposed so components that take a
 * `QueryFilterState` prop (like `FilterChips.svelte`) can be wired to Search's instance
 * without going through the per-setter façade. Use this only at the consumer-wrapper
 * boundary; the named getters/setters below are the long-term API the rest of Search
 * already speaks.
 */
export const searchQueryState = core

// Wire the core's AI-pattern probe to the extras module. The probe seeds the
// matching mode's hand-typed buffer on switchMode when (a) that buffer is empty
// and (b) the AI's last pattern kind matches.
core.setAiPatternProbe((forMode) => {
  const pattern = extras.getLastAiPattern()
  if (!pattern) return null
  const wantKind = forMode === 'regex' ? 'regex' : 'glob'
  return extras.getLastAiPatternKind() === wantKind ? pattern : null
})

// Query + mode
export const getQuery = (): string => core.getQuery()
export const setQuery = (value: string): void => {
  core.setQuery(value)
}
export const setQueryFromUserInput = (value: string): void => {
  core.setQueryFromUserInput(value)
}
export const getMode = (): ReturnType<typeof core.getMode> => core.getMode()
export const setMode = (value: Parameters<typeof core.setMode>[0]): void => {
  core.setMode(value)
}
export const switchMode = (target: Parameters<typeof core.switchMode>[0]): void => {
  core.switchMode(target)
}

// Size
export const getSizeFilter = (): ReturnType<typeof core.getSizeFilter> => core.getSizeFilter()
export const setSizeFilter = (v: Parameters<typeof core.setSizeFilter>[0]): void => {
  core.setSizeFilter(v)
}
export const getSizeValue = (): string => core.getSizeValue()
export const setSizeValue = (v: string): void => {
  core.setSizeValue(v)
}
export const getSizeUnit = (): ReturnType<typeof core.getSizeUnit> => core.getSizeUnit()
export const setSizeUnit = (v: Parameters<typeof core.setSizeUnit>[0]): void => {
  core.setSizeUnit(v)
}
export const setSizeValueMax = (v: string): void => {
  core.setSizeValueMax(v)
}
export const setSizeUnitMax = (v: Parameters<typeof core.setSizeUnitMax>[0]): void => {
  core.setSizeUnitMax(v)
}

// Date
export const getDateFilter = (): ReturnType<typeof core.getDateFilter> => core.getDateFilter()
export const setDateFilter = (v: Parameters<typeof core.setDateFilter>[0]): void => {
  core.setDateFilter(v)
}
export const getDateValue = (): string => core.getDateValue()
export const setDateValue = (v: string): void => {
  core.setDateValue(v)
}
export const setDateValueMax = (v: string): void => {
  core.setDateValueMax(v)
}

// Case sensitivity
export const getCaseSensitive = (): boolean => core.getCaseSensitive()
export const setCaseSensitive = (v: boolean): void => {
  core.setCaseSensitive(v)
}

// AI prompt + caveat (core)
export const getLastAiPrompt = (): string | null => core.getLastAiPrompt()
export const setLastAiPrompt = (v: string | null): void => {
  core.setLastAiPrompt(v)
}
export const getLastAiCaveat = (): string | null => core.getLastAiCaveat()

// AI pattern + label (extras)
export const getLastAiLabel = (): string | null => extras.getLastAiLabel()
export const getLastAiPattern = (): string | null => extras.getLastAiPattern()
export const getLastAiPatternKind = (): 'glob' | 'regex' | null => extras.getLastAiPatternKind()
export const clearAiPattern = (): void => {
  extras.clearAiPattern()
}

// Results + cursor
export const getResults = (): SearchResultEntry[] => core.getResults()
export const setResults = (v: SearchResultEntry[]): void => {
  core.setResults(v)
}
export const getTotalCount = (): number => core.getTotalCount()
export const setTotalCount = (v: number): void => {
  core.setTotalCount(v)
}
export const getCursorIndex = (): number => core.getCursorIndex()
export const setCursorIndex = (v: number): void => {
  core.setCursorIndex(v)
}

// Index availability (extras; Selection has no index)
export const getIsIndexReady = (): boolean => extras.getIsIndexReady()
export const setIsIndexReady = (v: boolean): void => {
  extras.setIsIndexReady(v)
}
export const getIndexEntryCount = (): number => extras.getIndexEntryCount()
export const setIndexEntryCount = (v: number): void => {
  extras.setIndexEntryCount(v)
}
export const getIsIndexAvailable = (): boolean => extras.getIsIndexAvailable()
export const setIsIndexAvailable = (v: boolean): void => {
  extras.setIsIndexAvailable(v)
}

// Scope + system-dirs (extras)
export const getScope = (): string => extras.getScope()
export const setScope = (v: string): void => {
  extras.setScope(v)
}
export const getCountOnly = (): boolean => extras.getCountOnly()
export const setCountOnly = (v: boolean): void => {
  extras.setCountOnly(v)
}
export const getExcludeSystemDirs = (): boolean => extras.getExcludeSystemDirs()
export const setExcludeSystemDirs = (v: boolean): void => {
  extras.setExcludeSystemDirs(v)
}

// Dialog lifecycle: SearchDialog reaches `searchQueryState` directly for `setRunOnMount`
// etc. The few call sites that need lastDialogEvent / runOnMount go through the
// underlying core instance.

/**
 * Records an AI translation. Fans a single call out across the core + extras split:
 * `core.recordAiTranslation` updates the matching hand-typed buffer;
 * `extras.recordAiPatternAndLabel` updates Search's Pattern chip + label slots.
 * Selection's wrapper bypasses this façade and calls only `core.recordAiTranslation`.
 */
export function recordAiTranslation(input: {
  pattern: string | null
  kind: 'glob' | 'regex' | null
  label: string | null
}): void {
  core.recordAiTranslation({ pattern: input.pattern, kind: input.kind })
  extras.recordAiPatternAndLabel(input)
}

/**
 * Builds the SearchQuery IPC payload. Pass-through to `build-search-query.ts`.
 */
export function buildSearchQuery(): SearchQuery {
  return buildSearchQueryImpl(core, extras)
}

/**
 * Builds a `HistoryEntry`-ready filter object from the current state.
 */
export function buildHistoryFilters(): HistoryFilters {
  return core.readHistoryFilters()
}

/**
 * Loads a persisted history entry into the dialog's live state. Used by the
 * recent-searches footer and popover.
 */
export function applyHistoryEntry(entry: HistoryEntry): void {
  core.setQuery(entry.query)
  core.setMode(entry.mode)
  extras.setScope(entry.scope)
  core.setCaseSensitive(entry.caseSensitive)
  extras.setExcludeSystemDirs(entry.excludeSystemDirs)
  core.applyHistoryFilters(entry.filters)
  // Reset per-mode buffers; mirror the entry into its own mode slot so a
  // follow-up mode switch sees a sensible value.
  core.clearHandTypedBuffers()
  core.setHandTypedBuffer(entry.mode, entry.query)
  // Clear any prior AI transparency state; the next AI run will repopulate it.
  core.setLastAiPrompt(null)
  core.setLastAiCaveat(null)
  extras.recordAiPatternAndLabel({ pattern: null, kind: null, label: null })
  core.setResults([])
  core.setTotalCount(0)
  core.setCursorIndex(0)
}

/**
 * Prefill payload coming from the MCP `open_search_dialog` tool.
 */
export interface SearchPrefill {
  query?: string
  mode?: ReturnType<typeof core.getMode>
  sizeMin?: number
  sizeMax?: number
  modifiedAfter?: string
  modifiedBefore?: string
  /** Type filter: `true` = folders only, `false` = files only, `null`/omitted = both. */
  isDirectory?: boolean | null
  scope?: string
  caseSensitive?: boolean
  excludeSystemDirs?: boolean
  autoRun?: boolean
}

/**
 * Applies an MCP prefill payload onto the live search state. Called BEFORE the
 * dialog mounts.
 */
export function applySearchPrefill(prefill: SearchPrefill): void {
  if (prefill.query !== undefined) core.setQuery(prefill.query)
  if (prefill.mode !== undefined) core.setMode(prefill.mode)
  if (prefill.scope !== undefined) extras.setScope(prefill.scope)
  if (prefill.caseSensitive !== undefined) core.setCaseSensitive(prefill.caseSensitive)
  if (prefill.excludeSystemDirs !== undefined) extras.setExcludeSystemDirs(prefill.excludeSystemDirs)

  // `applyHistoryFilters` resets size, date, and type, so we batch them into one call.
  const touchesSize = prefill.sizeMin !== undefined || prefill.sizeMax !== undefined
  const touchesDate = prefill.modifiedAfter !== undefined || prefill.modifiedBefore !== undefined
  const touchesType = prefill.isDirectory !== undefined
  if (touchesSize || touchesDate || touchesType) {
    const combined: HistoryFilters = {
      sizeMin: prefill.sizeMin,
      sizeMax: prefill.sizeMax,
      modifiedAfter: prefill.modifiedAfter,
      modifiedBefore: prefill.modifiedBefore,
      isDirectory: prefill.isDirectory ?? null,
    }
    core.applyHistoryFilters(combined)
  }

  // Clear any prior AI transparency strip; a new AI run from prefill will repopulate it.
  if (prefill.mode === 'ai' || prefill.query !== undefined) {
    core.setLastAiPrompt(null)
    core.setLastAiCaveat(null)
  }

  core.setResults([])
  core.setTotalCount(0)
  core.setCursorIndex(0)

  core.setRunOnMount(prefill.autoRun ?? true)
}

/**
 * Clears all dialog state to defaults. Triggered explicitly by the user via `⌘N`
 * ("new search") inside the dialog.
 */
export function clearSearchState(): void {
  core.clearCore()
  extras.clearExtras()
}
