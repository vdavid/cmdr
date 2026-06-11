/**
 * Reactive cross-consumer filter state for the Query dialog (Search + Selection).
 *
 * Two consumers (Search dialog and Selection dialog) each get their own instance, so
 * one dialog's state can't leak into the other. The Search façade in
 * `lib/search/search-state.svelte.ts` keeps the legacy named-export API on top of this
 * factory for the dialog's existing call sites.
 *
 * Scope: ONLY cross-consumer fields live here. Search-only fields (`scope`,
 * `excludeSystemDirs`, `lastAiLabel`, `lastAiPattern`, `lastAiPatternKind`) live in
 * `lib/search/search-extras-state.svelte.ts`. The Search wrapper composes both
 * instances; Selection's wrapper only uses the core.
 *
 * Per the plan's Decision log: `recordAiTranslation` on the core writes ONLY to
 * `handTyped[mode]`. It does NOT touch `lastAiLabel` / `lastAiPattern` /
 * `lastAiPatternKind` — those are Search-extras concerns. The Search wrapper calls
 * `extras.recordAiPatternAndLabel(...)` right after `core.recordAiTranslation(...)`
 * to keep both in sync. Selection's wrapper calls only the core method (it has no
 * Pattern chip to surface a pattern through).
 */

import type { SearchResultEntry, PatternType, SearchQuery, HistoryFilters } from '$lib/tauri-commands'
export type { PatternType }

/**
 * `eq` is a UI/chip-summary concern only: it round-trips through the matcher and history as
 * `between` with `sizeMin == sizeMax` (the matcher's `between` already matches exactly one
 * value). No `SizePredicate` or Rust `HistoryFilters` change carries it. See `applySizeQuery`,
 * `readSizeFilters`, and `applyHistoryFilters` (which rehydrates `size_min == size_max` as `eq`).
 */
export type SizeFilter = 'any' | 'gte' | 'lte' | 'eq' | 'between'
export type DateFilter = 'any' | 'after' | 'before' | 'between'

/**
 * Three-way type filter, named after the UI's `Both | Files | Folders` toggle. Maps to the
 * existing IPC `SearchQuery.isDirectory: Option<bool>` in `buildBaseSearchQuery`
 * (`both → null`, `file → false`, `folder → true`); there is no separate IPC field. The
 * Selection matcher reads it directly via `getIsDirFor`. Cross-consumer, so it lives in the
 * core factory (both dialogs show the toggle).
 */
export type TypeFilter = 'both' | 'file' | 'folder'
/**
 * Size unit. `B` (bytes) was added in round 2 (D10) so the list-style popover can let the
 * user pick a byte-level filter without leaving the popover. The byte unit's label varies
 * between `byte` / `bytes` depending on the selected count (see `byteUnitLabel`); KB/kB
 * follows the user's binary-vs-SI setting (`kiloByteLabel`).
 */
export type SizeUnit = 'B' | 'KB' | 'MB' | 'GB'

/**
 * Unified query mode. Search uses all three. Selection uses ai / filename / regex too
 * (no Content). The chip row below the bar drives `mode`; the input below it drives
 * `query`. AI mode is hidden when the AI provider is off (and, for Selection, also when
 * the provider is local).
 */
export type SearchMode = 'ai' | 'filename' | 'regex'

/**
 * Debounce window for auto-applied filename/regex searches. The previous 200 ms felt twitchy
 * on a 10M-entry index: every typed character paid for a search round-trip. 1 s matches
 * Spotlight's feel and gives the user space to finish a word before we react. AI mode never
 * auto-applies (see `SearchDialog.svelte#scheduleSearch`), so this constant is
 * filename/regex-only.
 */
export const SEARCH_AUTO_APPLY_DEBOUNCE_MS = 1000

/**
 * Last interaction the dialog observed. Drives the `⏎` ownership swap per round-2 D8.
 * The discriminator lives in state (not derived) because "what the user did last" isn't
 * recoverable from the other fields alone (a cursor move and a results-arrived both leave
 * `results.length > 0` and `cursorIndex` in the same shape; only the temporal order
 * distinguishes them). Updated at the call sites that actually own each transition.
 *
 * `LastDialogEvent` and `deriveEnterAction` live in `./enter-action` so consumers can
 * import the helper without pulling the factory; we re-export them here so the factory
 * stays the one-stop import for the dialog's state surface.
 */
import { deriveEnterAction, type LastDialogEvent, type EnterAction } from './enter-action'
export { deriveEnterAction, type LastDialogEvent, type EnterAction }

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

/**
 * Picks the friendliest size unit + value pair for a given byte count. Used by history
 * appliers to restore size filters in a human-readable form.
 */
export function bytesToSize(bytes: number): { value: string; unit: SizeUnit } {
  if (bytes >= 1024 * 1024 * 1024) {
    return { value: String(Math.round((bytes / (1024 * 1024 * 1024)) * 100) / 100), unit: 'GB' }
  }
  if (bytes >= 1024 * 1024) {
    return { value: String(Math.round((bytes / (1024 * 1024)) * 100) / 100), unit: 'MB' }
  }
  // Sub-kilobyte bounds read as raw bytes ("= 0 B", "512 B") rather than a fractional "0.5 KB".
  if (bytes < 1024) {
    return { value: String(bytes), unit: 'B' }
  }
  return { value: String(Math.round((bytes / 1024) * 100) / 100), unit: 'KB' }
}

/**
 * The shape returned by `createQueryFilterState()`. All getters/setters mirror the named
 * exports the Search wrapper's call sites use, so the wrapper can stay unchanged.
 */
export interface QueryFilterState {
  // Query + mode
  getQuery(): string
  setQuery(value: string): void
  setQueryFromUserInput(value: string): void
  getMode(): SearchMode
  setMode(value: SearchMode): void
  switchMode(targetMode: SearchMode): void

  // Size filter
  getSizeFilter(): SizeFilter
  setSizeFilter(value: SizeFilter): void
  getSizeValue(): string
  setSizeValue(value: string): void
  getSizeUnit(): SizeUnit
  setSizeUnit(value: SizeUnit): void
  getSizeValueMax(): string
  setSizeValueMax(value: string): void
  getSizeUnitMax(): SizeUnit
  setSizeUnitMax(value: SizeUnit): void

  // Date filter
  getDateFilter(): DateFilter
  setDateFilter(value: DateFilter): void
  getDateValue(): string
  setDateValue(value: string): void
  getDateValueMax(): string
  setDateValueMax(value: string): void

  // Type filter (file / folder / both)
  getTypeFilter(): TypeFilter
  setTypeFilter(value: TypeFilter): void

  // Case sensitivity
  getCaseSensitive(): boolean
  setCaseSensitive(value: boolean): void

  // AI transparency (prompt + caveat; pattern + label live in extras)
  getLastAiPrompt(): string | null
  setLastAiPrompt(value: string | null): void
  getLastAiCaveat(): string | null
  setLastAiCaveat(value: string | null): void

  // Results + cursor
  getResults(): SearchResultEntry[]
  setResults(value: SearchResultEntry[]): void
  getTotalCount(): number
  setTotalCount(value: number): void
  getCursorIndex(): number
  setCursorIndex(value: number): void
  getIsSearching(): boolean
  setIsSearching(value: boolean): void

  // Dialog lifecycle bookkeeping
  getLastDialogEvent(): LastDialogEvent
  setLastDialogEvent(value: LastDialogEvent): void
  getRunOnMount(): boolean
  setRunOnMount(value: boolean): void
  getLastRunQuery(): string | null
  setLastRunQuery(value: string | null): void

  /**
   * Records an AI translation's pattern outputs into the matching hand-typed buffer.
   * Does NOT touch `lastAiLabel`, `lastAiPattern`, or `lastAiPatternKind`: those live in
   * the Search-extras module so Selection's instance doesn't carry unused fields. The
   * Search wrapper calls this AND `extras.recordAiPatternAndLabel(...)` in sequence;
   * Selection's wrapper calls only this.
   */
  recordAiTranslation(input: { pattern: string | null; kind: 'glob' | 'regex' | null }): void

  /** Resets the per-mode buffers. Used by consumers' `applyHistoryEntry` helpers. */
  clearHandTypedBuffers(): void
  /**
   * Mirrors the AI pattern (when its kind matches the target mode) into the active
   * buffer if the target buffer is empty. Used by `switchMode`. The Search wrapper
   * also hands the AI pattern here at recordAi time when the producer's
   * `lastAiPatternKind` is present in extras; this hook lets the wrapper pre-seed
   * the matching buffer without core knowing about extras.
   */
  setHandTypedBuffer(mode: 'ai' | 'filename' | 'regex', value: string): void
  getHandTypedBuffer(mode: 'ai' | 'filename' | 'regex'): string

  /**
   * AI-pattern lookup hook injected by the consumer. The core delegates to this on
   * `switchMode` to seed an empty target buffer with the AI's last pattern (filename
   * gets a glob; regex gets a regex). Returns null when the consumer has no AI
   * pattern to offer. Search wires this to its extras module; Selection wires it
   * to null (Selection has no Pattern chip).
   */
  setAiPatternProbe(probe: (forMode: 'filename' | 'regex') => string | null): void

  /** Pure builder for the SearchQuery payload. Read by Search's `buildSearchQuery`. */
  buildBaseSearchQuery(): SearchQuery

  /** Apply size+date filters from a HistoryFilters object. Resets size/date first. */
  applyHistoryFilters(filters: HistoryFilters | undefined): void
  /** Reads size+date filters out as a HistoryFilters object. */
  readHistoryFilters(): HistoryFilters

  /** Resets all core fields to defaults. Extras have their own reset. */
  clearCore(): void
}

export interface CreateQueryFilterStateOptions {
  /** Default mode applied on reset. Search defaults to 'filename'; Selection same. */
  defaultMode?: SearchMode
}

export function createQueryFilterState(options: CreateQueryFilterStateOptions = {}): QueryFilterState {
  const defaultMode: SearchMode = options.defaultMode ?? 'filename'

  let query = $state('')
  let mode = $state<SearchMode>(defaultMode)

  let sizeFilter = $state<SizeFilter>('any')
  let sizeValue = $state('')
  let sizeUnit = $state<SizeUnit>('MB')
  let sizeValueMax = $state('')
  let sizeUnitMax = $state<SizeUnit>('MB')
  let dateFilter = $state<DateFilter>('any')
  let dateValue = $state('')
  let dateValueMax = $state('')

  let typeFilter = $state<TypeFilter>('both')

  let caseSensitive = $state(false)

  let lastAiPrompt = $state<string | null>(null)
  let lastAiCaveat = $state<string | null>(null)

  const handTyped = $state<{ ai: string; filename: string; regex: string }>({
    ai: '',
    filename: '',
    regex: '',
  })

  let results = $state<SearchResultEntry[]>([])
  let totalCount = $state(0)
  let cursorIndex = $state(0)
  let isSearching = $state(false)

  let lastDialogEvent = $state<LastDialogEvent>('opened')
  let runOnMount = $state(false)
  let lastRunQuery = $state<string | null>(null)

  // Injected by the consumer (Search wires this to its extras module; Selection leaves it null).
  let aiPatternProbe: (forMode: 'filename' | 'regex') => string | null = () => null

  function switchMode(targetMode: SearchMode): void {
    if (mode === targetMode) return
    // Preserve the user's current typing under the previous mode's slot before swapping.
    handTyped[mode] = query
    mode = targetMode
    // Restore the target mode's hand-typed value, or fall back to the AI pattern when it
    // matches the kind. The "other" mode's buffer stays whatever the user last typed.
    let next = handTyped[targetMode]
    if (!next && targetMode !== 'ai') {
      const probed = aiPatternProbe(targetMode)
      if (probed) next = probed
    }
    query = next
  }

  function applySizeQuery(q: SearchQuery): void {
    if (sizeFilter === 'any') return
    const minBytes = parseSizeToBytes(sizeValue, sizeUnit)
    if (sizeFilter === 'gte' && minBytes !== undefined) {
      q.minSize = minBytes
    } else if (sizeFilter === 'lte' && minBytes !== undefined) {
      q.maxSize = minBytes
    } else if (sizeFilter === 'eq' && minBytes !== undefined) {
      // Exact match: pin both bounds to the same value (the engine's min..max range collapses).
      q.minSize = minBytes
      q.maxSize = minBytes
    } else if (sizeFilter === 'between') {
      if (minBytes !== undefined) q.minSize = minBytes
      const maxBytes = parseSizeToBytes(sizeValueMax, sizeUnitMax)
      if (maxBytes !== undefined) q.maxSize = maxBytes
    }
  }

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
   * Maps the UI-named `typeFilter` to the existing IPC `isDirectory: Option<bool>`:
   * `both → null` (no constraint), `file → false`, `folder → true`. No separate IPC field.
   */
  function typeFilterToIsDirectory(): boolean | null {
    if (typeFilter === 'file') return false
    if (typeFilter === 'folder') return true
    return null
  }

  function buildBaseSearchQuery(): SearchQuery {
    const patternType: PatternType = mode === 'regex' ? 'regex' : 'glob'
    const q: SearchQuery = {
      namePattern: query.trim() || null,
      patternType,
      minSize: null,
      maxSize: null,
      modifiedAfter: null,
      modifiedBefore: null,
      isDirectory: typeFilterToIsDirectory(),
      limit: 30,
    }

    if (caseSensitive) {
      q.caseSensitive = true
    }

    applySizeQuery(q)
    applyDateQuery(q)

    return q
  }

  function applyHistoryFilters(filters: HistoryFilters | undefined): void {
    sizeFilter = 'any'
    sizeValue = ''
    sizeUnit = 'MB'
    sizeValueMax = ''
    sizeUnitMax = 'MB'
    dateFilter = 'any'
    dateValue = ''
    dateValueMax = ''
    typeFilter = 'both'

    if (!filters) return

    // `isDirectory` round-trips the type filter: `true → folder`, `false → file`,
    // `null`/absent → both (the reset above).
    if (filters.isDirectory === true) typeFilter = 'folder'
    else if (filters.isDirectory === false) typeFilter = 'file'

    if (filters.sizeMin != null && filters.sizeMax != null && filters.sizeMin === filters.sizeMax) {
      // `between x x` and `eq x` match exactly the same set; rehydrate the friendlier `= x`
      // label (deliberate: there's no stored comparator kind, so we always collapse to eq).
      sizeFilter = 'eq'
      const exact = bytesToSize(filters.sizeMin)
      sizeValue = exact.value
      sizeUnit = exact.unit
    } else if (filters.sizeMin != null && filters.sizeMax != null) {
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

  function readSizeFilters(): { sizeMin?: number; sizeMax?: number } {
    if (sizeFilter === 'any') return {}
    const minBytes = parseSizeToBytes(sizeValue, sizeUnit)
    if (sizeFilter === 'gte') return minBytes !== undefined ? { sizeMin: minBytes } : {}
    if (sizeFilter === 'lte') return minBytes !== undefined ? { sizeMax: minBytes } : {}
    // `eq` persists as `size_min == size_max`; it rehydrates as `eq` (not `between`) in
    // `applyHistoryFilters` by deliberate decision.
    if (sizeFilter === 'eq') return minBytes !== undefined ? { sizeMin: minBytes, sizeMax: minBytes } : {}
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

  function readTypeFilter(): { isDirectory?: boolean } {
    if (typeFilter === 'folder') return { isDirectory: true }
    if (typeFilter === 'file') return { isDirectory: false }
    return {}
  }

  function readHistoryFilters(): HistoryFilters {
    return { ...readSizeFilters(), ...readDateFilters(), ...readTypeFilter() }
  }

  return {
    getQuery: () => query,
    setQuery: (v) => {
      query = v
    },
    setQueryFromUserInput: (v) => {
      query = v
      handTyped[mode] = v
    },
    getMode: () => mode,
    setMode: (v) => {
      mode = v
    },
    switchMode,

    getSizeFilter: () => sizeFilter,
    setSizeFilter: (v) => {
      sizeFilter = v
    },
    getSizeValue: () => sizeValue,
    setSizeValue: (v) => {
      sizeValue = v
    },
    getSizeUnit: () => sizeUnit,
    setSizeUnit: (v) => {
      sizeUnit = v
    },
    getSizeValueMax: () => sizeValueMax,
    setSizeValueMax: (v) => {
      sizeValueMax = v
    },
    getSizeUnitMax: () => sizeUnitMax,
    setSizeUnitMax: (v) => {
      sizeUnitMax = v
    },

    getDateFilter: () => dateFilter,
    setDateFilter: (v) => {
      dateFilter = v
    },
    getDateValue: () => dateValue,
    setDateValue: (v) => {
      dateValue = v
    },
    getDateValueMax: () => dateValueMax,
    setDateValueMax: (v) => {
      dateValueMax = v
    },

    getTypeFilter: () => typeFilter,
    setTypeFilter: (v) => {
      typeFilter = v
    },

    getCaseSensitive: () => caseSensitive,
    setCaseSensitive: (v) => {
      caseSensitive = v
    },

    getLastAiPrompt: () => lastAiPrompt,
    setLastAiPrompt: (v) => {
      lastAiPrompt = v
    },
    getLastAiCaveat: () => lastAiCaveat,
    setLastAiCaveat: (v) => {
      lastAiCaveat = v
    },

    getResults: () => results,
    setResults: (v) => {
      results = v
    },
    getTotalCount: () => totalCount,
    setTotalCount: (v) => {
      totalCount = v
    },
    getCursorIndex: () => cursorIndex,
    setCursorIndex: (v) => {
      cursorIndex = v
    },
    getIsSearching: () => isSearching,
    setIsSearching: (v) => {
      isSearching = v
    },

    getLastDialogEvent: () => lastDialogEvent,
    setLastDialogEvent: (v) => {
      lastDialogEvent = v
    },
    getRunOnMount: () => runOnMount,
    setRunOnMount: (v) => {
      runOnMount = v
    },
    getLastRunQuery: () => lastRunQuery,
    setLastRunQuery: (v) => {
      lastRunQuery = v
    },

    /**
     * The AI's pattern OVERWRITES the matching hand-typed buffer. The user just asked
     * the AI to take over: if it produces a glob, that's the new filename pattern; if
     * it produces a regex, that's the new regex pattern. Empty patterns leave the
     * buffers alone so a no-op translation doesn't wipe the user's typed-by-hand value.
     *
     * This does NOT write the Search-only pattern/label fields — those are extras
     * concerns. Search's wrapper calls `extras.recordAiPatternAndLabel(...)` right
     * after this method.
     */
    recordAiTranslation: (input) => {
      if (input.pattern && input.pattern.trim()) {
        if (input.kind === 'regex') {
          handTyped.regex = input.pattern
        } else if (input.kind === 'glob') {
          handTyped.filename = input.pattern
        }
      }
    },

    clearHandTypedBuffers: () => {
      handTyped.ai = ''
      handTyped.filename = ''
      handTyped.regex = ''
    },
    setHandTypedBuffer: (m, v) => {
      handTyped[m] = v
    },
    getHandTypedBuffer: (m) => handTyped[m],

    setAiPatternProbe: (probe) => {
      aiPatternProbe = probe
    },

    buildBaseSearchQuery,
    applyHistoryFilters,
    readHistoryFilters,

    clearCore: () => {
      query = ''
      mode = defaultMode
      sizeFilter = 'any'
      sizeValue = ''
      sizeUnit = 'MB'
      sizeValueMax = ''
      sizeUnitMax = 'MB'
      dateFilter = 'any'
      dateValue = ''
      dateValueMax = ''
      typeFilter = 'both'
      results = []
      totalCount = 0
      cursorIndex = 0
      caseSensitive = false
      isSearching = false
      lastAiPrompt = null
      lastAiCaveat = null
      handTyped.ai = ''
      handTyped.filename = ''
      handTyped.regex = ''
      runOnMount = false
      lastRunQuery = null
    },
  }
}
