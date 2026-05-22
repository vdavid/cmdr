// Reactive search state for the search dialog.
// Uses Svelte 5 runes ($state, $derived) following existing codebase patterns.

import { getAppLogger } from '$lib/logging/logger'
import type { SearchResultEntry, PatternType, SearchQuery, HistoryEntry, HistoryFilters } from '$lib/tauri-commands'

const log = getAppLogger('search-state')
export type { PatternType }

export type SizeFilter = 'any' | 'gte' | 'lte' | 'between'
export type DateFilter = 'any' | 'after' | 'before' | 'between'
/**
 * Façade over the M2 core/extras split.
 *
 * Before M2 this file was a 713-line module-singleton. M2 carved it into:
 *   - `lib/query-ui/query-filter-state.svelte.ts`: the cross-consumer core
 *     (factory `createQueryFilterState`).
 *   - `lib/search/search-extras-state.svelte.ts`: Search-only fields
 *     (factory `createSearchExtrasState`).
 *   - `lib/search/build-search-query.ts`: the `SearchQuery` IPC payload builder.
 *
 * This file instantiates one of each and re-exports the legacy named-function API
 * so the ~15 Search call sites work unchanged during the rest of the milestones.
 * The intention is a transparent passthrough: no branching, no shape adaptation,
 * just delegation. M3 will rename the call sites to use the instances directly.
 *
 * The `recordAiTranslation({pattern, kind, label})` shape is preserved here as a
 * convenience that fans out to `core.recordAiTranslation({pattern, kind})` and
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
  PatternType,
  LastDialogEvent,
  EnterAction,
} from '$lib/query-ui/query-filter-state.svelte'

const core = createQueryFilterState({ defaultMode: 'filename' })
const extras = createSearchExtrasState()

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
export const setQuery = (value: string): void => core.setQuery(value)
export const setQueryFromUserInput = (value: string): void => core.setQueryFromUserInput(value)
export const getMode = (): ReturnType<typeof core.getMode> => core.getMode()
export const setMode = (value: Parameters<typeof core.setMode>[0]): void => core.setMode(value)
export const switchMode = (target: Parameters<typeof core.switchMode>[0]): void => core.switchMode(target)

// Size
export const getSizeFilter = (): ReturnType<typeof core.getSizeFilter> => core.getSizeFilter()
export const setSizeFilter = (v: Parameters<typeof core.setSizeFilter>[0]): void => core.setSizeFilter(v)
export const getSizeValue = (): string => core.getSizeValue()
export const setSizeValue = (v: string): void => core.setSizeValue(v)
export const getSizeUnit = (): ReturnType<typeof core.getSizeUnit> => core.getSizeUnit()
export const setSizeUnit = (v: Parameters<typeof core.setSizeUnit>[0]): void => core.setSizeUnit(v)
export const getSizeValueMax = (): string => core.getSizeValueMax()
export const setSizeValueMax = (v: string): void => core.setSizeValueMax(v)
export const getSizeUnitMax = (): ReturnType<typeof core.getSizeUnitMax> => core.getSizeUnitMax()
export const setSizeUnitMax = (v: Parameters<typeof core.setSizeUnitMax>[0]): void => core.setSizeUnitMax(v)

// Date
export const getDateFilter = (): ReturnType<typeof core.getDateFilter> => core.getDateFilter()
export const setDateFilter = (v: Parameters<typeof core.setDateFilter>[0]): void => core.setDateFilter(v)
export const getDateValue = (): string => core.getDateValue()
export const setDateValue = (v: string): void => core.setDateValue(v)
export const getDateValueMax = (): string => core.getDateValueMax()
export const setDateValueMax = (v: string): void => core.setDateValueMax(v)

// Case sensitivity
export const getCaseSensitive = (): boolean => core.getCaseSensitive()
export const setCaseSensitive = (v: boolean): void => core.setCaseSensitive(v)

// AI prompt + caveat (core)
export const getLastAiPrompt = (): string | null => core.getLastAiPrompt()
export const setLastAiPrompt = (v: string | null): void => core.setLastAiPrompt(v)
export const getLastAiCaveat = (): string | null => core.getLastAiCaveat()
export const setLastAiCaveat = (v: string | null): void => core.setLastAiCaveat(v)

// AI pattern + label (extras)
export const getLastAiLabel = (): string | null => extras.getLastAiLabel()
export const getLastAiPattern = (): string | null => extras.getLastAiPattern()
export const getLastAiPatternKind = (): 'glob' | 'regex' | null => extras.getLastAiPatternKind()
export const clearAiPattern = (): void => extras.clearAiPattern()

// Results + cursor
export const getResults = (): SearchResultEntry[] => core.getResults()
export const setResults = (v: SearchResultEntry[]): void => core.setResults(v)
export const getTotalCount = (): number => core.getTotalCount()
export const setTotalCount = (v: number): void => core.setTotalCount(v)
export const getCursorIndex = (): number => core.getCursorIndex()
export const setCursorIndex = (v: number): void => core.setCursorIndex(v)
export const getIsSearching = (): boolean => core.getIsSearching()
export const setIsSearching = (v: boolean): void => core.setIsSearching(v)

// Index availability (extras; Selection has no index)
export const getIsIndexReady = (): boolean => extras.getIsIndexReady()
export const setIsIndexReady = (v: boolean): void => extras.setIsIndexReady(v)
export const getIndexEntryCount = (): number => extras.getIndexEntryCount()
export const setIndexEntryCount = (v: number): void => extras.setIndexEntryCount(v)
export const getIsIndexAvailable = (): boolean => extras.getIsIndexAvailable()
export const setIsIndexAvailable = (v: boolean): void => extras.setIsIndexAvailable(v)

// Scope + system-dirs (extras)
export const getScope = (): string => extras.getScope()
export const setScope = (v: string): void => extras.setScope(v)
export const getExcludeSystemDirs = (): boolean => extras.getExcludeSystemDirs()
export const setExcludeSystemDirs = (v: boolean): void => extras.setExcludeSystemDirs(v)

// Dialog lifecycle (core)
export const getLastDialogEvent = (): ReturnType<typeof core.getLastDialogEvent> => core.getLastDialogEvent()
export const setLastDialogEvent = (v: Parameters<typeof core.setLastDialogEvent>[0]): void => core.setLastDialogEvent(v)
export const getRunOnMount = (): boolean => core.getRunOnMount()
export const setRunOnMount = (v: boolean): void => core.setRunOnMount(v)

/**
 * One-shot flag set by external openers (MCP `open_search_dialog`) to ask the dialog to run a
 * search after mount. The dialog reads + clears this in `onMount` so a manual reopen doesn't
 * re-run an old query. The flag is independent of `autoApplyEnabled`: even when auto-apply is
 * off, an explicit MCP `autoRun: true` honors the caller's intent.
 */
let runOnMount = $state(false)

// Getters
export function getIsIndexReady(): boolean {
  return isIndexReady
}
export function getIndexEntryCount(): number {
  return indexEntryCount
}
export function getIsSearching(): boolean {
  return isSearching
}
export function getQuery(): string {
  return query
}
export function getMode(): SearchMode {
  return mode
}
export function getSizeFilter(): SizeFilter {
  return sizeFilter
}
export function getSizeValue(): string {
  return sizeValue
}
export function getSizeUnit(): SizeUnit {
  return sizeUnit
}
export function getSizeValueMax(): string {
  return sizeValueMax
}
export function getSizeUnitMax(): SizeUnit {
  return sizeUnitMax
}
export function getDateFilter(): DateFilter {
  return dateFilter
}
export function getDateValue(): string {
  return dateValue
}
export function getDateValueMax(): string {
  return dateValueMax
}
export function getResults(): SearchResultEntry[] {
  return results
}
export function getTotalCount(): number {
  return totalCount
}
export function getCursorIndex(): number {
  return cursorIndex
}
export function getIsIndexAvailable(): boolean {
  return isIndexAvailable
}
export function getCaseSensitive(): boolean {
  return caseSensitive
}
export function getScope(): string {
  return scope
}
export function getExcludeSystemDirs(): boolean {
  return excludeSystemDirs
}
export function getLastAiPrompt(): string | null {
  return lastAiPrompt
}
export function getLastAiCaveat(): string | null {
  return lastAiCaveat
}
export function getLastAiLabel(): string | null {
  return lastAiLabel
}
export function getLastAiPattern(): string | null {
  return lastAiPattern
}
export function getLastAiPatternKind(): 'glob' | 'regex' | null {
  return lastAiPatternKind
}
export function getRunOnMount(): boolean {
  return runOnMount
}
export function getLastDialogEvent(): LastDialogEvent {
  return lastDialogEvent
}
export function setLastDialogEvent(value: LastDialogEvent): void {
  lastDialogEvent = value
}

/**
 * Pure helper for D8: derives which action `⏎` owns right now.
 *
 *   - `'go-to-file'` when results are present AND the last event was either
 *     `results-arrived` (the user just landed on a populated list) or
 *     `cursor-moved` (the user is browsing the list).
 *   - `'run-search'` otherwise (empty results, freshly opened, query / filter just
 *     edited): pressing ⏎ runs the search instead.
 */
export type EnterAction = 'go-to-file' | 'run-search'
export function deriveEnterAction(input: { lastEvent: LastDialogEvent; resultsCount: number }): EnterAction {
  if (input.resultsCount <= 0) return 'run-search'
  if (input.lastEvent === 'results-arrived' || input.lastEvent === 'cursor-moved') {
    return 'go-to-file'
  }
  return 'run-search'
}

// Setters
export function setIsIndexReady(value: boolean): void {
  isIndexReady = value
}
export function setIndexEntryCount(value: number): void {
  indexEntryCount = value
}
export function setIsSearching(value: boolean): void {
  isSearching = value
}
export function setQuery(value: string): void {
  query = value
}
/**
 * Sets the query AND mirrors it into the active mode's hand-typed buffer. Used by the
 * search bar's `oninput` so the user's typing per mode is preserved across mode switches
 * (see `switchMode`). AI translations write to `query` directly via `setQuery`, not via
 * this helper — the AI pattern lives in its own slot.
 */
export function setQueryFromUserInput(value: string): void {
  query = value
  handTyped[mode] = value
}
export function setMode(value: SearchMode): void {
  mode = value
}
export function setSizeFilter(value: SizeFilter): void {
  sizeFilter = value
}
export function setSizeValue(value: string): void {
  sizeValue = value
}
export function setSizeUnit(value: SizeUnit): void {
  sizeUnit = value
}
export function setSizeValueMax(value: string): void {
  sizeValueMax = value
}
export function setSizeUnitMax(value: SizeUnit): void {
  sizeUnitMax = value
}
export function setDateFilter(value: DateFilter): void {
  dateFilter = value
}
export function setDateValue(value: string): void {
  dateValue = value
}
export function setDateValueMax(value: string): void {
  dateValueMax = value
}
export function setResults(value: SearchResultEntry[]): void {
  results = value
}
export function setTotalCount(value: number): void {
  totalCount = value
}
export function setCursorIndex(value: number): void {
  cursorIndex = value
}
export function setIsIndexAvailable(value: boolean): void {
  isIndexAvailable = value
}
export function setCaseSensitive(value: boolean): void {
  caseSensitive = value
}
export function setScope(value: string): void {
  scope = value
}
export function setExcludeSystemDirs(value: boolean): void {
  excludeSystemDirs = value
}
export function setLastAiPrompt(value: string | null): void {
  lastAiPrompt = value
}
export function setLastAiCaveat(value: string | null): void {
  lastAiCaveat = value
}
// Note: `setLastAiLabel` and `setLastAiPattern` aren't exported separately;
// callers use `recordAiTranslation` (below) to update all three slots
// atomically.
export function setRunOnMount(value: boolean): void {
  runOnMount = value
}

/**
 * Switches the active mode and swaps the displayed query (`query`) to match the new mode's
 * buffer. The current mode's input value is preserved in its hand-typed buffer first (so a
 * future switch back can restore it). When moving to filename/regex mode and that buffer is
 * empty, the AI-produced pattern of matching kind is loaded in instead (per
 * search-fixup-brief clarification 2).
 *
 * No-op when `targetMode === mode`.
 */
export function switchMode(targetMode: SearchMode): void {
  log.debug(
    'switchMode: from={from} to={to} query={query} handTyped.ai={ai} handTyped.filename={filename} handTyped.regex={regex} lastAiPattern={lastAiPattern} lastAiPatternKind={lastAiPatternKind}',
    {
      from: mode,
      to: targetMode,
      query,
      ai: handTyped.ai,
      filename: handTyped.filename,
      regex: handTyped.regex,
      lastAiPattern: lastAiPattern ?? '(null)',
      lastAiPatternKind: lastAiPatternKind ?? '(null)',
    },
  )
  if (mode === targetMode) {
    log.debug('switchMode: no-op (same mode)')
    return
  }
  // Preserve the user's current typing under the previous mode's slot before swapping.
  handTyped[mode] = query
  mode = targetMode
  // Restore the target mode's hand-typed value, or fall back to the AI pattern when it
  // matches the kind. The "other" mode's buffer stays whatever the user last typed.
  let next = handTyped[targetMode]
  if (!next && lastAiPattern) {
    const wantKind = targetMode === 'regex' ? 'regex' : 'glob'
    if (lastAiPatternKind === wantKind && targetMode !== 'ai') {
      next = lastAiPattern
    }
  }
  query = next
  log.debug('switchMode: done, mode={mode} query={query}', { mode, query })
}

/**
 * Records an AI translation's outputs: the LLM-produced pattern (`pattern` + `kind`, may be
 * null) and the short LLM-friendly label. The caller is expected to set `lastAiPrompt`
 * separately via `setLastAiPrompt`.
 *
 * R3 B2: the AI's pattern OVERWRITES the matching hand-typed buffer. The user
 * just asked the AI to take over: if it produces a glob, that's the new filename
 * pattern; if it produces a regex, that's the new regex pattern. Round 2 kept
 * the old hand-typed buffer untouched and only loaded the AI pattern when the
 * target buffer was empty (via `switchMode`); David hit cases where a stale
 * `*.foo` survived a fresh AI request for `*.pdf`. Per the brief, the takeover
 * is opinionated: AI's output is the new truth for the matching mode.
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

  // `applyHistoryFilters` resets both size and date, so we batch them into one call.
  const touchesSize = prefill.sizeMin !== undefined || prefill.sizeMax !== undefined
  const touchesDate = prefill.modifiedAfter !== undefined || prefill.modifiedBefore !== undefined
  if (touchesSize || touchesDate) {
    const combined: HistoryFilters = {
      sizeMin: prefill.sizeMin,
      sizeMax: prefill.sizeMax,
      modifiedAfter: prefill.modifiedAfter,
      modifiedBefore: prefill.modifiedBefore,
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
