// Reactive search state for the search dialog.
// Uses Svelte 5 runes ($state, $derived) following existing codebase patterns.

import type { SearchResultEntry, PatternType, SearchQuery, HistoryEntry, HistoryFilters } from '$lib/tauri-commands'
export type { PatternType }

export type SizeFilter = 'any' | 'gte' | 'lte' | 'between'
export type DateFilter = 'any' | 'after' | 'before' | 'between'
export type SizeUnit = 'KB' | 'MB' | 'GB'

/**
 * Unified search mode. M2 collapses the previous split (AI prompt row vs filename pattern row)
 * into a single `query` input with a `mode` discriminator. The chip row below the bar drives
 * `mode`; the input below it drives `query`. AI mode is hidden when the AI provider is off.
 */
export type SearchMode = 'ai' | 'filename' | 'regex'

// Module-level reactive state
let isIndexReady = $state(false)
let indexEntryCount = $state(0)
let isSearching = $state(false)

// Unified query field. M2: replaces the separate `namePattern` and `aiPrompt` fields.
let query = $state('')
// Active search mode. Drives placeholder, chip styling, and how Enter dispatches.
let mode = $state<SearchMode>('filename')

let sizeFilter = $state<SizeFilter>('any')
let sizeValue = $state('')
let sizeUnit = $state<SizeUnit>('MB')
let sizeValueMax = $state('')
let sizeUnitMax = $state<SizeUnit>('MB')
let dateFilter = $state<DateFilter>('any')
let dateValue = $state('')
let dateValueMax = $state('')

// Results
let results = $state<SearchResultEntry[]>([])
let totalCount = $state(0)
let cursorIndex = $state(0)

// Index availability: false when indexing is disabled or not started
let isIndexAvailable = $state(true)

// Case sensitivity
let caseSensitive = $state(false)

/**
 * The original natural-language prompt the user typed before the AI translated it. Preserved so
 * the AI transparency strip can show what was actually asked, even after the AI overwrites `query`
 * with the translated pattern. Null when no AI search has run this session.
 */
let lastAiPrompt = $state<string | null>(null)
/**
 * The caveat returned alongside the last AI translation (for example "I ignored the file size you
 * mentioned because the request didn't include a unit"). Null when no AI search has run, or when
 * the AI returned no caveat. Shown by the transparency strip below the prompt.
 */
let lastAiCaveat = $state<string | null>(null)

// Scope (folder filter)
let scope = $state('')

// System/build directory exclusion toggle (on by default).
// The actual exclude list lives in Rust: `SYSTEM_DIR_EXCLUDES` in `search/query.rs`.
// The frontend only controls the boolean; Rust does the filtering.
let excludeSystemDirs = $state(true)

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

/** Converts size input + unit to bytes. Returns undefined if empty or invalid. */
export function parseSizeToBytes(value: string, unit: SizeUnit): number | undefined {
  const num = parseFloat(value)
  if (isNaN(num) || num <= 0) return undefined
  const multipliers: Record<SizeUnit, number> = { KB: 1024, MB: 1024 * 1024, GB: 1024 * 1024 * 1024 }
  return Math.round(num * multipliers[unit])
}

/** Converts ISO date string to unix timestamp (seconds). Returns undefined if empty/invalid. */
export function parseDateToTimestamp(value: string): number | undefined {
  if (!value) return undefined
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state, just parsing a timestamp
  const date = new Date(value)
  if (isNaN(date.getTime())) return undefined
  return Math.floor(date.getTime() / 1000)
}

/** Applies the current size filter state to the query. */
function applySizeQuery(q: SearchQuery): void {
  if (sizeFilter === 'any') return
  const minBytes = parseSizeToBytes(sizeValue, sizeUnit)
  if (sizeFilter === 'gte' && minBytes !== undefined) {
    q.minSize = minBytes
  } else if (sizeFilter === 'lte' && minBytes !== undefined) {
    q.maxSize = minBytes
  } else if (sizeFilter === 'between') {
    if (minBytes !== undefined) q.minSize = minBytes
    const maxBytes = parseSizeToBytes(sizeValueMax, sizeUnitMax)
    if (maxBytes !== undefined) q.maxSize = maxBytes
  }
}

/** Applies the current date filter state to the query. */
function applyDateQuery(q: SearchQuery): void {
  if (dateFilter === 'any') return
  const ts = parseDateToTimestamp(dateValue)
  if (dateFilter === 'after' && ts !== undefined) {
    q.modifiedAfter = ts
  } else if (dateFilter === 'before' && ts !== undefined) {
    q.modifiedBefore = ts
  } else if (dateFilter === 'between') {
    if (ts !== undefined) q.modifiedAfter = ts
    const tsMax = parseDateToTimestamp(dateValueMax)
    if (tsMax !== undefined) q.modifiedBefore = tsMax
  }
}

/**
 * Builds a `SearchQuery` from the current state. Used by filename and regex modes; AI mode goes
 * through `translateSearchQuery` first and then populates state before this is called. The backend
 * `patternType` is derived from `mode`: filename maps to glob, regex to regex. AI mode (which only
 * reaches here after AI translation flipped the mode) maps to glob as a safe default.
 */
export function buildSearchQuery(): SearchQuery {
  const patternType: PatternType = mode === 'regex' ? 'regex' : 'glob'
  const q: SearchQuery = {
    namePattern: query.trim() || null,
    patternType,
    minSize: null,
    maxSize: null,
    modifiedAfter: null,
    modifiedBefore: null,
    isDirectory: null,
    limit: 30,
  }

  // Only include caseSensitive when explicitly set, so Rust uses the platform default (None)
  if (caseSensitive) {
    q.caseSensitive = true
  }

  if (!excludeSystemDirs) {
    q.excludeSystemDirs = false
  }

  applySizeQuery(q)
  applyDateQuery(q)

  return q
}

/**
 * Picks the friendliest size unit + value pair for a given byte count. Mirrors the helper in
 * `SearchDialog.svelte` so callers outside the dialog (history loader) can reuse it.
 */
function bytesToSize(bytes: number): { value: string; unit: SizeUnit } {
  if (bytes >= 1024 * 1024 * 1024) {
    return { value: String(Math.round((bytes / (1024 * 1024 * 1024)) * 100) / 100), unit: 'GB' }
  }
  if (bytes >= 1024 * 1024) {
    return { value: String(Math.round((bytes / (1024 * 1024)) * 100) / 100), unit: 'MB' }
  }
  return { value: String(Math.round((bytes / 1024) * 100) / 100), unit: 'KB' }
}

function applyHistoryFilters(filters: HistoryFilters | undefined): void {
  // Reset to "any" first; we'll set the chip below if the history actually carries a filter.
  sizeFilter = 'any'
  sizeValue = ''
  sizeUnit = 'MB'
  sizeValueMax = ''
  sizeUnitMax = 'MB'
  dateFilter = 'any'
  dateValue = ''
  dateValueMax = ''

  if (!filters) return

  if (filters.sizeMin != null && filters.sizeMax != null) {
    sizeFilter = 'between'
    const min = bytesToSize(filters.sizeMin)
    const max = bytesToSize(filters.sizeMax)
    sizeValue = min.value
    sizeUnit = min.unit
    sizeValueMax = max.value
    sizeUnitMax = max.unit
  } else if (filters.sizeMin != null) {
    sizeFilter = 'gte'
    const min = bytesToSize(filters.sizeMin)
    sizeValue = min.value
    sizeUnit = min.unit
  } else if (filters.sizeMax != null) {
    sizeFilter = 'lte'
    const max = bytesToSize(filters.sizeMax)
    sizeValue = max.value
    sizeUnit = max.unit
  }

  if (filters.modifiedAfter != null && filters.modifiedBefore != null) {
    dateFilter = 'between'
    dateValue = filters.modifiedAfter
    dateValueMax = filters.modifiedBefore
  } else if (filters.modifiedAfter != null) {
    dateFilter = 'after'
    dateValue = filters.modifiedAfter
  } else if (filters.modifiedBefore != null) {
    dateFilter = 'before'
    dateValue = filters.modifiedBefore
  }
}

/**
 * Loads a persisted history entry into the dialog's live state. Used by the recent-searches
 * footer (chip click) and popover (item activation). Resets the AI transparency strip; if the
 * caller wants to run an AI search from this entry, they can re-trigger it via the dialog's
 * AI path (which captures the prompt into `lastAiPrompt` itself).
 */
export function applyHistoryEntry(entry: HistoryEntry): void {
  query = entry.query
  mode = entry.mode
  scope = entry.scope
  caseSensitive = entry.caseSensitive
  excludeSystemDirs = entry.excludeSystemDirs
  applyHistoryFilters(entry.filters)
  // Clear any prior AI transparency state; the next AI run will repopulate it.
  lastAiPrompt = null
  lastAiCaveat = null
  results = []
  totalCount = 0
  cursorIndex = 0
}

function readSizeFilters(): { sizeMin?: number; sizeMax?: number } {
  if (sizeFilter === 'any') return {}
  const minBytes = parseSizeToBytes(sizeValue, sizeUnit)
  if (sizeFilter === 'gte') return minBytes !== undefined ? { sizeMin: minBytes } : {}
  if (sizeFilter === 'lte') return minBytes !== undefined ? { sizeMax: minBytes } : {}
  // between
  const maxBytes = parseSizeToBytes(sizeValueMax, sizeUnitMax)
  const out: { sizeMin?: number; sizeMax?: number } = {}
  if (minBytes !== undefined) out.sizeMin = minBytes
  if (maxBytes !== undefined) out.sizeMax = maxBytes
  return out
}

function readDateFilters(): { modifiedAfter?: string; modifiedBefore?: string } {
  if (dateFilter === 'after') return dateValue ? { modifiedAfter: dateValue } : {}
  if (dateFilter === 'before') return dateValue ? { modifiedBefore: dateValue } : {}
  if (dateFilter === 'between') {
    const out: { modifiedAfter?: string; modifiedBefore?: string } = {}
    if (dateValue) out.modifiedAfter = dateValue
    if (dateValueMax) out.modifiedBefore = dateValueMax
    return out
  }
  return {}
}

/**
 * Builds a `HistoryEntry`-ready filter object from the current state. Pure helper for the
 * "Open in pane" call site that will land in M8.
 */
export function buildHistoryFilters(): HistoryFilters {
  return { ...readSizeFilters(), ...readDateFilters() }
}

/**
 * Clears all dialog state to defaults. Triggered explicitly by the user via `⌘N` ("new search")
 * inside the dialog. The module-level `$state` survives dialog unmount/remount, so close-then-reopen
 * never calls this. The only reset path is the user pressing `⌘N`.
 */
export function clearSearchState(): void {
  query = ''
  mode = 'filename'
  sizeFilter = 'any'
  sizeValue = ''
  sizeUnit = 'MB'
  sizeValueMax = ''
  sizeUnitMax = 'MB'
  dateFilter = 'any'
  dateValue = ''
  dateValueMax = ''
  results = []
  totalCount = 0
  cursorIndex = 0
  isIndexAvailable = true
  caseSensitive = false
  scope = ''
  excludeSystemDirs = true
  isSearching = false
  lastAiPrompt = null
  lastAiCaveat = null
}
