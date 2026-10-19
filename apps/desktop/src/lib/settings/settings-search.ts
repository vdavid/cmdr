/**
 * Settings search, in the UI language with English as a fallback, through the shared
 * `#lib/utils/unicode-fuzzy` engine (the command palette's: any script, accents optional).
 */

import { foldForSearch, fuzzySearch, type FoldedText } from '#lib/utils/unicode-fuzzy.js'
import { availableLocales, resolvedCatalogLocale, tStringIn } from '#lib/intl/messages.svelte.js'
import { getUiLocale } from '#lib/intl/locale.js'
import { localeDisplayName } from '#lib/intl/locale-display-names.js'
import type { SearchableEntry, SettingSearchResult } from './types'
import { settingsRegistry } from './settings-registry'
import { searchableRowEntries } from './sections/searchable-rows'
import { sectionTitle } from './section-i18n'
import { searchAllCommands } from '#lib/commands/fuzzy-search.js'
import { isMacOS } from '#lib/shortcuts/key-capture.js'

// ============================================================================
// Search Index
// ============================================================================

interface SearchIndexEntry {
  entry: SearchableEntry
  /** The folded text uFuzzy searches; match ranges index it and the unfolded original alike. */
  searchableText: FoldedText
  /** Where the localized label sits in `searchableText`, `[start, end)`, from the SAME snapshot. */
  labelStart: number
  labelEnd: number
}

interface SearchIndex {
  /** The catalog the text was built from; a different one means the UI language changed. */
  locale: string
  entries: SearchIndexEntry[]
  haystack: FoldedText[]
}

let searchIndex: SearchIndex | null = null

/** The catalog the UI reads right now: `pt-PT` → `pt`, `ja-JP` → `en`. Keys the index. */
function currentCatalogLocale(): string {
  return resolvedCatalogLocale(getUiLocale())
}

/** English and its overlays (`en-GB`, `en-AU`, and the `en-XA` pseudolocale) get no English fallback. */
function isEnglish(locale: string): boolean {
  return locale.split('-')[0] === 'en'
}

/**
 * The entry's catalog keywords as `locale` wrote them: its `keywordsKey` message, split on commas.
 *
 * The ONE place keywords enter the index. A list the language hasn't translated resolves to the
 * English one, which the English fallback indexes already, so it counts as no keywords here.
 * Language-neutral `terms` are not keywords: `indexEntry` adds them once, in every language.
 */
function keywordsIn(locale: string, entry: SearchableEntry): string[] {
  if (entry.keywordsKey === undefined) return []
  const list = tStringIn(locale, entry.keywordsKey)
  if (!isEnglish(locale) && list === tStringIn('en', entry.keywordsKey)) return []
  return splitKeywordList(list)
}

/** Splits on the comma of any script a translator may reach for (`,` `，` `、` `،`), trimmed. */
function splitKeywordList(list: string): string[] {
  return list
    .split(/[,，、،]/)
    .map((word) => word.trim())
    .filter((word) => word !== '')
}

/**
 * Build the search index from the settings registry plus the searchable rows, in the current UI
 * language. Rebuilt lazily whenever that language changed since the last build.
 */
function buildSearchIndex(): SearchIndex {
  const locale = currentCatalogLocale()
  if (searchIndex?.locale === locale) return searchIndex

  // The index covers the WHOLE registry, including the Advanced section
  // (`section: ['Advanced']`): the Advanced page renders those grouped into cards
  // on the same `shouldShow` pipeline as every other section, so they must be
  // globally findable too (searching "prefetch" lights the Advanced sidebar entry
  // and shows the row in its card). `hidden` is intentionally NOT filtered here —
  // a hidden internal-state entry costs a stray match at worst, while filtering it
  // would need a second reason for a row to be missing. `buildSectionTree`
  // excludes `hidden` from nav.
  //
  // Searchable rows (the hand-rendered non-settings: "Clear index", "Get a
  // license") join here as equal entries. They carry no `SettingsValues` key and
  // add no nav entry — `buildSectionTree` reads the registry alone.
  //
  // A `macOSOnly` setting renders nowhere else, so off macOS it stays out, the
  // same rule `searchableRowEntries` applies to rows. Read when the index builds
  // rather than at import, so a test can flip the platform.
  const onMacOS = isMacOS()
  const settings = settingsRegistry.filter((setting) => onMacOS || setting.macOSOnly !== true)
  const entries = [...settings, ...searchableRowEntries()].map((entry) => indexEntry(entry, locale))
  searchIndex = { locale, entries, haystack: entries.map((e) => e.searchableText) }

  return searchIndex
}

/**
 * One entry's searchable text, in this order:
 *
 * 1. The localized part: translated section titles (like "Megjelenés › Színek és formátumok"),
 *    label, description, keywords, the language-neutral `terms`, and card-group title, so searching
 *    a card title surfaces its rows.
 * 2. Outside English, the English fallback: label, keywords, card title, and section path. Not the
 *    description: long common-word sentences make short queries noisy.
 * 3. For a `searchAllLocales` entry, its label in every loaded catalog plus every locale's own name.
 *
 * The label's offsets are recorded here, from the same strings, so a highlight never measures a
 * live label against stale text. Anything matched past the label (English included) shows no
 * highlight, by design.
 */
function indexEntry(entry: SearchableEntry, locale: string): SearchIndexEntry {
  const sectionPath = entry.section.map(sectionTitle).join(' › ')
  const label = entry.label
  const parts = [sectionPath, label, entry.description, ...keywordsIn(locale, entry), ...(entry.terms ?? [])]
  if (entry.card !== undefined) parts.push(entry.card)

  if (!isEnglish(locale)) {
    parts.push(tStringIn('en', entry.labelKey), ...keywordsIn('en', entry))
    if (entry.cardKey !== undefined) parts.push(tStringIn('en', entry.cardKey))
    parts.push(entry.section.join(' › '))
  }

  if (entry.searchAllLocales === true) parts.push(...everyLocaleTerms(entry))

  const labelStart = sectionPath.length + 1
  return {
    entry,
    searchableText: foldForSearch(parts.join(' ')),
    labelStart,
    labelEnd: labelStart + label.length,
  }
}

/** The entry's label in every loaded catalog, plus each locale's name in its own language, deduplicated. */
function everyLocaleTerms(entry: SearchableEntry): string[] {
  const locales = availableLocales()
  const terms = new Set<string>()
  for (const tag of locales) {
    terms.add(tStringIn(tag, entry.labelKey))
    terms.add(localeDisplayName(tag, locales))
  }
  return [...terms]
}

// ============================================================================
// Search Functions
// ============================================================================

/**
 * Search settings and searchable rows by query string.
 * Returns the entries that match, with match indices for highlighting.
 */
export function searchSettings(query: string): SettingSearchResult[] {
  return searchIndexed(query).map(({ indexed, matchedIndices }) => ({
    entry: indexed.entry,
    matchedIndices,
    searchableText: indexed.searchableText,
  }))
}

interface IndexedMatch {
  indexed: SearchIndexEntry
  /** Character indices into `indexed.searchableText`. */
  matchedIndices: number[]
}

/** The last search, reused: every visible `SettingRow` asks for its highlight with the same query. */
let lastSearch: { index: SearchIndex; query: string; matches: IndexedMatch[] } | null = null

/** Runs `query` over the current index, ranked, keeping each hit's index entry. */
function searchIndexed(query: string): IndexedMatch[] {
  const index = buildSearchIndex()
  if (lastSearch?.index === index && lastSearch.query === query) return lastSearch.matches
  const matches = runSearch(index, query)
  lastSearch = { index, query, matches }
  return matches
}

function runSearch(index: SearchIndex, query: string): IndexedMatch[] {
  // Empty query returns everything
  if (!query.trim()) {
    return index.entries.map((indexed) => ({ indexed, matchedIndices: [] }))
  }

  // A null idxs/order is uFuzzy's "no matches" shape; the guard also narrows the
  // result to the ranked case, so `info` is non-null in the map below.
  const [matchedIndices, info, order] = fuzzySearch(index.haystack, query)
  if (!matchedIndices || !order) {
    return []
  }

  // Build results with match information
  return order.map((idx) => {
    const indexed = index.entries[matchedIndices[idx]]
    // ranges is a flat array of [start, end) pairs (end exclusive)
    const ranges = info.ranges[idx]

    // Convert ranges to individual character indices
    const indices: number[] = []
    for (let i = 0; i < ranges.length; i += 2) {
      const start = ranges[i]
      const end = ranges[i + 1]
      for (let j = start; j < end; j++) {
        indices.push(j)
      }
    }

    return { indexed, matchedIndices: indices }
  })
}

/**
 * Get the sections that contain matching settings.
 * Used to filter the tree view during search.
 */
export function getMatchingSections(query: string): Set<string> {
  const results = searchSettings(query)
  const sections = new Set<string>()

  for (const result of results) {
    // Add all parent sections
    for (let i = 1; i <= result.entry.section.length; i++) {
      sections.add(result.entry.section.slice(0, i).join('/'))
    }
  }

  // Also check if any commands match for Keyboard shortcuts section. The full-registry
  // search, because that section renders (and rebinds) every command, not just
  // palette-visible ones.
  if (query.trim()) {
    const commandMatches = searchAllCommands(query)
    if (commandMatches.length > 0) {
      sections.add('Keyboard shortcuts')
    }
  }

  // Every other section lights up through its entries alone: a page with nothing to show for a
  // hit (License and AI once had hardcoded keyword lists here) must not light up at all.
  return sections
}

/**
 * Check if a section contains any matching settings.
 */
export function sectionHasMatches(sectionPath: string[], matchingSections: Set<string>): boolean {
  return matchingSections.has(sectionPath.join('/'))
}

/**
 * Highlight matched characters in text.
 * Returns an array of { text, matched } segments for rendering.
 */
export function highlightMatches(text: string, matchedIndices: number[]): Array<{ text: string; matched: boolean }> {
  if (matchedIndices.length === 0) {
    return [{ text, matched: false }]
  }

  const matchSet = new Set(matchedIndices)
  const segments: Array<{ text: string; matched: boolean }> = []
  let currentSegment = ''
  let currentMatched = matchSet.has(0)

  for (let i = 0; i < text.length; i++) {
    const isMatched = matchSet.has(i)

    if (isMatched !== currentMatched) {
      if (currentSegment) {
        segments.push({ text: currentSegment, matched: currentMatched })
      }
      currentSegment = text[i]
      currentMatched = isMatched
    } else {
      currentSegment += text[i]
    }
  }

  if (currentSegment) {
    segments.push({ text: currentSegment, matched: currentMatched })
  }

  return segments
}

/**
 * Create a `shouldShow` filter for a given search query.
 * Returns a function that checks whether a setting ID matches the query.
 * When the query is empty, all settings are shown.
 */
export function createShouldShow(searchQuery: string): (id: string) => boolean {
  const matchingIds = searchQuery.trim() ? getMatchingSettingIds(searchQuery) : null
  return (id: string) => !matchingIds || matchingIds.has(id)
}

/**
 * Whether any of the given setting ids is currently visible under `shouldShow`.
 *
 * Sections pass the SAME `createShouldShow(searchQuery)` predicate they use to
 * gate each row, so a card's frame (`{#if anyVisible(...)}`) and its rows
 * (`{#if shouldShow(id)}`) can never disagree: an all-filtered-out card hides
 * its frame too, so search never leaves empty cards behind. Pure; no registry
 * read (card visibility is section-owned, never re-derived from `card`).
 */
export function anyVisible(shouldShow: (id: string) => boolean, ...ids: string[]): boolean {
  return ids.some(shouldShow)
}

/**
 * Clear the search index. A language change needs no call (the index is keyed by catalog); this
 * is for tests that flip the platform.
 */
export function clearSearchIndex(): void {
  searchIndex = null
  lastSearch = null
}

/**
 * Get the set of setting IDs that match the query.
 * Used to filter which settings to display in section components.
 */
export function getMatchingSettingIds(query: string): Set<string> {
  const results = searchSettings(query)
  return new Set(results.map((r) => r.entry.id))
}

/**
 * Get matching setting IDs within a specific section.
 */
export function getMatchingSettingIdsInSection(query: string, sectionPath: string[]): Set<string> {
  const results = searchSettings(query)
  const sectionPrefix = sectionPath.join('/')

  return new Set(
    results
      .filter((r) => {
        const settingSectionPath = r.entry.section.join('/')
        return settingSectionPath === sectionPrefix || settingSectionPath.startsWith(sectionPrefix + '/')
      })
      .map((r) => r.entry.id),
  )
}

/**
 * Get match indices for a specific setting's label.
 * Returns indices relative to the label text for highlighting.
 */
export function getMatchIndicesForLabel(query: string, settingId: string): number[] {
  if (!query.trim()) return []

  const match = searchIndexed(query).find((m) => m.indexed.entry.id === settingId)
  if (!match) return []

  // The indices run over the whole searchable text; keep the ones inside the label, measured
  // when the text was built (folding keeps every offset, so they index the label as rendered).
  const { labelStart, labelEnd } = match.indexed
  return match.matchedIndices.filter((idx) => idx >= labelStart && idx < labelEnd).map((idx) => idx - labelStart)
}
