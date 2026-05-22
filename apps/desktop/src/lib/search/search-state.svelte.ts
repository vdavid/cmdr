// Reactive search state for the search dialog.
// Uses Svelte 5 runes ($state, $derived) following existing codebase patterns.

import type { SearchResultEntry, PatternType, SearchQuery, HistoryEntry, HistoryFilters } from '$lib/tauri-commands'
export type { PatternType }

export type SizeFilter = 'any' | 'gte' | 'lte' | 'between'
export type DateFilter = 'any' | 'after' | 'before' | 'between'
/**
 * Size unit. `B` (bytes) was added in round 2 (D10) so the list-style popover can let the
 * user pick a byte-level filter without leaving the popover. The byte unit's label varies
 * between `byte` / `bytes` depending on the selected count (see `byteUnitLabel`); KB/kB
 * follows the user's binary-vs-SI setting (`kiloByteLabel`).
 */
export type SizeUnit = 'B' | 'KB' | 'MB' | 'GB'

/**
 * Unified search mode. M2 collapses the previous split (AI prompt row vs filename pattern row)
 * into a single `query` input with a `mode` discriminator. The chip row below the bar drives
 * `mode`; the input below it drives `query`. AI mode is hidden when the AI provider is off.
 */
export type SearchMode = 'ai' | 'filename' | 'regex'

/**
 * Debounce window for auto-applied filename/regex searches. The previous 200 ms felt twitchy on a
 * 10M-entry index: every typed character paid for a search round-trip. 1 s matches Spotlight's feel
 * and gives the user space to finish a word before we react. AI mode never auto-applies (see
 * `SearchDialog.svelte#scheduleSearch`), so this constant is filename/regex-only.
 */
export const SEARCH_AUTO_APPLY_DEBOUNCE_MS = 1000

// Module-level reactive state
let isIndexReady = $state(false)
let indexEntryCount = $state(0)
let isSearching = $state(false)

/**
 * Last interaction the dialog observed. Drives the `⏎` ownership swap per round-2 D8:
 *
 *   - 'results-arrived' or 'cursor-moved' (with results present): ⏎ activates "Go to file"
 *     on the cursor row.
 *   - 'opened', 'query-edited', 'filter-edited' (or no results yet): ⏎ runs the search.
 *
 * The discriminator lives in state (not derived) because "what the user did last" isn't
 * recoverable from the other fields alone (a cursor move and a results-arrived both leave
 * `results.length > 0` and `cursorIndex` in the same shape; only the temporal order
 * distinguishes them). Updated at the call sites that actually own each transition.
 */
export type LastDialogEvent = 'opened' | 'results-arrived' | 'cursor-moved' | 'query-edited' | 'filter-edited'
let lastDialogEvent = $state<LastDialogEvent>('opened')

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

/**
 * The LLM-produced label for the last AI translation (max ~40 chars). Read by the "Open in
 * pane" snapshot builder so the pane breadcrumb reflects the AI's summary instead of the
 * verbatim prompt. Null when no AI search has run or when the model omitted the field.
 */
let lastAiLabel = $state<string | null>(null)

/**
 * The pattern the last AI translation produced (the actual glob or regex string), separate
 * from the prompt the user typed. We keep this so:
 *   1. The "Pattern" chip in the filter strip can show the current pattern across all modes.
 *   2. Switching out of AI mode hands the matching mode the AI's pattern, while leaving
 *      the other mode's hand-typed buffer untouched.
 * Null when no AI search has run or when the AI returned no pattern.
 */
let lastAiPattern = $state<string | null>(null)
/**
 * Discriminator for `lastAiPattern`: which input kind it fits ('glob' goes to filename mode,
 * 'regex' to regex mode). Null when the AI hasn't run or returned no pattern.
 */
let lastAiPatternKind = $state<'glob' | 'regex' | null>(null)

/**
 * Per-mode hand-typed buffers. Switching modes (⌘1 / ⌘2 / ⌘3) restores the user's last
 * hand-typed value for the target mode, instead of carrying the AI pattern (or another
 * mode's pattern) across modes. AI translations don't write here; only direct user input
 * does. The active-mode value is mirrored into `query` so the input reads/writes one
 * field.
 */
const handTyped = $state<{ ai: string; filename: string; regex: string }>({
  ai: '',
  filename: '',
  regex: '',
})

// Scope (folder filter)
let scope = $state('')

// System/build directory exclusion toggle (on by default).
// The actual exclude list lives in Rust: `SYSTEM_DIR_EXCLUDES` in `search/query.rs`.
// The frontend only controls the boolean; Rust does the filtering.
let excludeSystemDirs = $state(true)

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
  if (mode === targetMode) return
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
  lastAiPattern = input.pattern
  lastAiPatternKind = input.pattern ? input.kind : null
  lastAiLabel = input.label
  // Mirror the produced pattern into the matching mode's hand-typed buffer.
  // (The "other" buffer stays whatever the user typed; the AI only speaks to
  // one of filename / regex per translation.) Empty patterns leave the buffers
  // alone so a no-op translation doesn't wipe the user's typed-by-hand value.
  if (input.pattern && input.pattern.trim()) {
    if (input.kind === 'regex') {
      handTyped.regex = input.pattern
    } else if (input.kind === 'glob') {
      handTyped.filename = input.pattern
    }
  }
}

/**
 * Clears the AI pattern + label + caveat (but leaves `lastAiPrompt` and the transparency
 * strip intact). Called when the user changes the pattern via the Pattern chip's clear
 * button (per search-fixup-brief clarification 5: clearing the chip doesn't hide the
 * strip).
 */
export function clearAiPattern(): void {
  lastAiPattern = null
  lastAiPatternKind = null
}

/**
 * Converts size input + unit to bytes. Returns `undefined` if empty or invalid. A value of
 * exactly `0` is honored (the user explicitly picked 0 bytes from the D10 grid preset and
 * the engine should pin the lower / upper bound to zero rather than silently skip the
 * filter).
 */
export function parseSizeToBytes(value: string, unit: SizeUnit): number | undefined {
  const num = parseFloat(value)
  if (isNaN(num) || num < 0) return undefined
  const multipliers: Record<SizeUnit, number> = {
    B: 1,
    KB: 1024,
    MB: 1024 * 1024,
    GB: 1024 * 1024 * 1024,
  }
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
  // Mirror into the per-mode buffer so a follow-up mode switch sees a sensible value.
  handTyped.ai = ''
  handTyped.filename = ''
  handTyped.regex = ''
  handTyped[entry.mode] = entry.query
  // Clear any prior AI transparency state; the next AI run will repopulate it.
  lastAiPrompt = null
  lastAiCaveat = null
  lastAiLabel = null
  lastAiPattern = null
  lastAiPatternKind = null
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
 * Prefill payload coming from the MCP `open_search_dialog` tool. Mirrors the shape of the tool
 * schema in `src-tauri/src/mcp/tools.rs`. All fields optional; the listener strips nulls before
 * forwarding here so an absent field stays absent.
 */
export interface SearchPrefill {
  query?: string
  mode?: SearchMode
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
 * Applies an MCP prefill payload onto the live search state. Called BEFORE the dialog mounts
 * (the listener flips `showSearchDialog = true` after running this). The dialog's `onMount`
 * reads `runOnMount` and dispatches the right run path (AI or filename/regex) based on `mode`.
 *
 * Behavior notes:
 *   - Missing fields are left at their current value (no implicit reset). Callers that want a
 *     clean slate should call `clearSearchState()` first.
 *   - `mode` falls back to current state if absent; the listener decides the default based on
 *     whether AI is enabled (mirrors the tool docs: "default 'ai' if AI on, else 'filename'").
 *   - `runOnMount` defaults to true when the caller didn't pass `autoRun` (matches the tool
 *     schema's default-true contract).
 */
export function applySearchPrefill(prefill: SearchPrefill): void {
  if (prefill.query !== undefined) query = prefill.query
  if (prefill.mode !== undefined) mode = prefill.mode
  if (prefill.scope !== undefined) scope = prefill.scope
  if (prefill.caseSensitive !== undefined) caseSensitive = prefill.caseSensitive
  if (prefill.excludeSystemDirs !== undefined) excludeSystemDirs = prefill.excludeSystemDirs

  // `applyHistoryFilters` resets both size and date, so we batch them into one call. Only set
  // when the caller passed at least one size or date field; otherwise leave the current filters
  // alone (matches "missing fields preserve current state").
  const touchesSize = prefill.sizeMin !== undefined || prefill.sizeMax !== undefined
  const touchesDate = prefill.modifiedAfter !== undefined || prefill.modifiedBefore !== undefined
  if (touchesSize || touchesDate) {
    const combined: HistoryFilters = {
      sizeMin: prefill.sizeMin,
      sizeMax: prefill.sizeMax,
      modifiedAfter: prefill.modifiedAfter,
      modifiedBefore: prefill.modifiedBefore,
    }
    applyHistoryFilters(combined)
  }

  // Clear any prior AI transparency strip; a new AI run from prefill will repopulate it.
  if (prefill.mode === 'ai' || prefill.query !== undefined) {
    lastAiPrompt = null
    lastAiCaveat = null
  }

  // Reset cursor + results so the dialog opens with a clean slate visually.
  results = []
  totalCount = 0
  cursorIndex = 0

  runOnMount = prefill.autoRun ?? true
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
  lastAiLabel = null
  lastAiPattern = null
  lastAiPatternKind = null
  handTyped.ai = ''
  handTyped.filename = ''
  handTyped.regex = ''
  runOnMount = false
}
