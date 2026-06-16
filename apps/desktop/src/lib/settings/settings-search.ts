/**
 * Settings search functionality using uFuzzy.
 * Same search engine and configuration as the command palette.
 */

import uFuzzy from '@leeoniya/ufuzzy'
import type { SettingDefinition, SettingSearchResult } from './types'
import { settingsRegistry } from './settings-registry'
import { searchAllCommands } from '$lib/commands/fuzzy-search'

// ============================================================================
// Search Configuration (same as command palette)
// ============================================================================

const fuzzy = new uFuzzy({
  intraMode: 1, // Fuzzy matching within words (catches typos)
  interIns: 3, // Max 3 insertions between matched characters
})

// ============================================================================
// Search Index
// ============================================================================

interface SearchIndexEntry {
  setting: SettingDefinition
  searchableText: string
}

let searchIndex: SearchIndexEntry[] | null = null

/**
 * Build the search index from the settings registry.
 * Lazily initialized on first search.
 */
function buildSearchIndex(): SearchIndexEntry[] {
  if (searchIndex) return searchIndex

  searchIndex = settingsRegistry
    .filter((s) => !s.showInAdvanced) // Advanced settings are searched separately
    .map((setting) => ({
      setting,
      searchableText: buildSearchableText(setting),
    }))

  return searchIndex
}

/**
 * Build searchable text for a setting by concatenating:
 * - Section path (like "Appearance > Colors and formats")
 * - Label
 * - Description
 * - Keywords
 * - Card-group title (resolved from `cardKey`), so searching a card title surfaces its rows
 *
 * The resolved `card` MUST stay LAST. `getMatchIndicesForLabel` reconstructs label-highlight
 * offsets assuming `section › … + ' ' + label` at the front; inserting `card` earlier would
 * silently mis-highlight labels. Appending is offset-safe.
 */
function buildSearchableText(setting: SettingDefinition): string {
  const parts = [setting.section.join(' › '), setting.label, setting.description, ...setting.keywords]
  if (setting.card !== undefined) parts.push(setting.card)
  return parts.join(' ').toLowerCase()
}

// ============================================================================
// Search Functions
// ============================================================================

/**
 * Search settings by query string.
 * Returns settings that match the query with match indices for highlighting.
 */
export function searchSettings(query: string): SettingSearchResult[] {
  const index = buildSearchIndex()

  // Empty query returns all settings
  if (!query.trim()) {
    return index.map((entry) => ({
      setting: entry.setting,
      matchedIndices: [],
      searchableText: entry.searchableText,
    }))
  }

  const haystack = index.map((e) => e.searchableText)
  // A null idxs/order is uFuzzy's "no matches" shape; the guard also narrows the
  // result to the ranked case, so `info` is non-null in the map below.
  const [matchedIndices, info, order] = fuzzy.search(haystack, query.toLowerCase())
  if (!matchedIndices || !order) {
    return []
  }

  // Build results with match information
  return order.map((idx) => {
    const entry = index[matchedIndices[idx]]
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

    return {
      setting: entry.setting,
      matchedIndices: indices,
      searchableText: entry.searchableText,
    }
  })
}

/**
 * Search only advanced settings.
 */
export function searchAdvancedSettings(query: string): SettingSearchResult[] {
  const advancedSettings = settingsRegistry.filter((s) => s.showInAdvanced)

  if (!query.trim()) {
    return advancedSettings.map((setting) => ({
      setting,
      matchedIndices: [],
      searchableText: buildSearchableText(setting),
    }))
  }

  const entries = advancedSettings.map((setting) => ({
    setting,
    searchableText: buildSearchableText(setting),
  }))

  const haystack = entries.map((e) => e.searchableText)
  const [matchedIndices, info, order] = fuzzy.search(haystack, query.toLowerCase())
  if (!matchedIndices || !order) {
    return []
  }

  return order.map((idx) => {
    const entry = entries[matchedIndices[idx]]
    const ranges = info.ranges[idx]

    const indices: number[] = []
    for (let i = 0; i < ranges.length; i += 2) {
      const start = ranges[i]
      const end = ranges[i + 1]
      for (let j = start; j < end; j++) {
        indices.push(j)
      }
    }

    return {
      setting: entry.setting,
      matchedIndices: indices,
      searchableText: entry.searchableText,
    }
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
    for (let i = 1; i <= result.setting.section.length; i++) {
      sections.add(result.setting.section.slice(0, i).join('/'))
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

  // Check if query matches license-related terms
  if (query.trim()) {
    const licenseKeywords = 'license key activation commercial personal upgrade buy purchase pricing'
    const lowerQuery = query.toLowerCase()
    if (licenseKeywords.includes(lowerQuery) || 'license'.includes(lowerQuery)) {
      sections.add('License')
    }
  }

  // Check if query matches AI-related terms
  if (query.trim()) {
    const aiKeywords =
      'ai artificial intelligence llm model openai api key local llama server provider context memory cloud anthropic groq together fireworks mistral ollama deepseek xai perplexity openrouter gemini azure lm-studio custom service'
    const lowerQuery = query.toLowerCase()
    if (aiKeywords.split(' ').some((kw) => kw.startsWith(lowerQuery)) || lowerQuery === 'ai') {
      sections.add('AI')
    }
  }

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
 * Clear the search index (for testing or when settings change).
 */
export function clearSearchIndex(): void {
  searchIndex = null
}

/**
 * Get the set of setting IDs that match the query.
 * Used to filter which settings to display in section components.
 */
export function getMatchingSettingIds(query: string): Set<string> {
  const results = searchSettings(query)
  return new Set(results.map((r) => r.setting.id))
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
        const settingSectionPath = r.setting.section.join('/')
        return settingSectionPath === sectionPrefix || settingSectionPath.startsWith(sectionPrefix + '/')
      })
      .map((r) => r.setting.id),
  )
}

/**
 * Get match indices for a specific setting's label.
 * Returns indices relative to the label text for highlighting.
 */
export function getMatchIndicesForLabel(query: string, settingId: string): number[] {
  if (!query.trim()) return []

  const results = searchSettings(query)
  const result = results.find((r) => r.setting.id === settingId)
  if (!result) return []

  // The matchedIndices are relative to searchableText which includes section path
  // We need to find where the label starts in searchableText and adjust indices
  const setting = result.setting
  // Match the format used in buildSearchableText: section.join(' › ') + ' ' + label
  const sectionText = setting.section.join(' › ') + ' '
  // searchableText is lowercased, so we need to work with the lowercased label length
  const labelStart = sectionText.toLowerCase().length
  const labelEnd = labelStart + setting.label.toLowerCase().length

  // Filter indices that fall within the label range and adjust them
  return result.matchedIndices.filter((idx) => idx >= labelStart && idx < labelEnd).map((idx) => idx - labelStart)
}
