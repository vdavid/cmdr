// Reactive search state for the search dialog.
// Uses Svelte 5 runes ($state, $derived) following existing codebase patterns.

import type { SearchResultEntry, PatternType, SearchQuery } from '$lib/tauri-commands'
export type { PatternType }

export type SizeFilter = 'any' | 'gte' | 'lte' | 'between'
export type DateFilter = 'any' | 'after' | 'before' | 'between'
export type SizeUnit = 'KB' | 'MB' | 'GB'

// Module-level reactive state
let isIndexReady = $state(false)
let indexEntryCount = $state(0)
let isSearching = $state(false)

// Query fields
let namePattern = $state('')
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

// Index availability — false when indexing is disabled or not started
let isIndexAvailable = $state(true)

// Pattern type (glob vs regex)
let patternType = $state<PatternType>('glob')

// Case sensitivity
let caseSensitive = $state(false)

// AI state
let aiStatus = $state('')
let aiPrompt = $state('')
let caveat = $state('')

// Scope (folder filter)
let scope = $state('')

// System/build directory exclusion toggle (on by default).
// The actual exclude list lives in Rust: `SYSTEM_DIR_EXCLUDES` in `search/query.rs`.
// The frontend only controls the boolean — Rust does the filtering.
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
export function getNamePattern(): string {
  return namePattern
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
export function getPatternType(): PatternType {
  return patternType
}
export function getAiStatus(): string {
  return aiStatus
}
export function getAiPrompt(): string {
  return aiPrompt
}
export function getCaseSensitive(): boolean {
  return caseSensitive
}
export function getScope(): string {
  return scope
}
export function getCaveat(): string {
  return caveat
}
export function getExcludeSystemDirs(): boolean {
  return excludeSystemDirs
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
export function setNamePattern(value: string): void {
  namePattern = value
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
export function setPatternType(value: PatternType): void {
  patternType = value
}
export function setAiStatus(value: string): void {
  aiStatus = value
}
export function setAiPrompt(value: string): void {
  aiPrompt = value
}
export function setCaseSensitive(value: boolean): void {
  caseSensitive = value
}
export function setScope(value: string): void {
  scope = value
}
export function setCaveat(text: string): void {
  caveat = text
}
export function setExcludeSystemDirs(value: boolean): void {
  excludeSystemDirs = value
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
function applySizeQuery(query: SearchQuery): void {
  if (sizeFilter === 'any') return
  const minBytes = parseSizeToBytes(sizeValue, sizeUnit)
  if (sizeFilter === 'gte' && minBytes !== undefined) {
    query.minSize = minBytes
  } else if (sizeFilter === 'lte' && minBytes !== undefined) {
    query.maxSize = minBytes
  } else if (sizeFilter === 'between') {
    if (minBytes !== undefined) query.minSize = minBytes
    const maxBytes = parseSizeToBytes(sizeValueMax, sizeUnitMax)
    if (maxBytes !== undefined) query.maxSize = maxBytes
  }
}

/** Applies the current date filter state to the query. */
function applyDateQuery(query: SearchQuery): void {
  if (dateFilter === 'any') return
  const ts = parseDateToTimestamp(dateValue)
  if (dateFilter === 'after' && ts !== undefined) {
    query.modifiedAfter = ts
  } else if (dateFilter === 'before' && ts !== undefined) {
    query.modifiedBefore = ts
  } else if (dateFilter === 'between') {
    if (ts !== undefined) query.modifiedAfter = ts
    const tsMax = parseDateToTimestamp(dateValueMax)
    if (tsMax !== undefined) query.modifiedBefore = tsMax
  }
}

/** Builds a SearchQuery from the current state. */
export function buildSearchQuery(): SearchQuery {
  const query: SearchQuery = {
    patternType,
    limit: 30,
  }

  // Only include caseSensitive when explicitly set, so Rust uses the platform default (None)
  if (caseSensitive) {
    query.caseSensitive = true
  }

  if (namePattern.trim()) {
    query.namePattern = namePattern.trim()
  }

  if (!excludeSystemDirs) {
    query.excludeSystemDirs = false
  }

  applySizeQuery(query)
  applyDateQuery(query)

  return query
}

/** Resets all search state to defaults (for dialog close). */
export function resetSearchState(): void {
  namePattern = ''
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
  patternType = 'glob'
  caseSensitive = false
  aiStatus = ''
  aiPrompt = ''
  caveat = ''
  scope = ''
  excludeSystemDirs = true
  isSearching = false
}
