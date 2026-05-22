/**
 * Pure helpers for deriving the display state of the filter chips
 * (Size, Modified, Search in) from the current search state.
 *
 * Kept side-effect free so unit tests can pin the chip-summary rules cheaply without mounting
 * the Svelte component. See `SearchFilterChips.svelte` for the markup consumer.
 */

import type { SizeFilter, SizeUnit, DateFilter } from './query-filter-state.svelte'
import type { FileSizeFormat } from '$lib/settings/types'

/** Display state of a single filter chip. */
export interface FilterChipState {
  configured: boolean
  /** Summary shown when configured. Empty string when not configured. */
  summary: string
}

/**
 * Renders a `SizeUnit` for display, respecting the user's
 * `appearance.fileSizeFormat` setting. R3 B3: only the kilobyte cell varies
 * (SI = `kB`, binary = `KB`); bytes / megabytes / gigabytes are constant.
 * Default is `'binary'` so legacy callers that don't pass a format stay on the
 * raw enum-name rendering.
 */
function renderUnit(unit: SizeUnit, format: FileSizeFormat = 'binary'): string {
  if (unit === 'KB') return format === 'si' ? 'kB' : 'KB'
  return unit
}

/**
 * Returns the chip state for the Size filter. R3 B3: pipes the user's
 * `appearance.fileSizeFormat` through so the chip reads "100 kB" with SI
 * selected (matching the popover) instead of always showing "100 KB".
 */
export function deriveSizeChip(
  sizeFilter: SizeFilter,
  sizeValue: string,
  sizeUnit: SizeUnit,
  sizeValueMax: string,
  sizeUnitMax: SizeUnit,
  format: FileSizeFormat = 'binary',
): FilterChipState {
  if (sizeFilter === 'any') return { configured: false, summary: '' }

  const unitLabel = renderUnit(sizeUnit, format)
  const unitMaxLabel = renderUnit(sizeUnitMax, format)

  // A configured filter requires at least the first value (or both, for "between"). The chip
  // stays unconfigured if the user changed the comparator to "gte" but hasn't typed a number yet.
  const minNumeric = parseFloat(sizeValue)
  const minOk = !isNaN(minNumeric) && minNumeric > 0
  if (sizeFilter === 'gte') {
    if (!minOk) return { configured: false, summary: '' }
    return { configured: true, summary: `> ${sizeValue.trim()} ${unitLabel}` }
  }
  if (sizeFilter === 'lte') {
    if (!minOk) return { configured: false, summary: '' }
    return { configured: true, summary: `< ${sizeValue.trim()} ${unitLabel}` }
  }
  // between
  const maxNumeric = parseFloat(sizeValueMax)
  const maxOk = !isNaN(maxNumeric) && maxNumeric > 0
  if (!minOk && !maxOk) return { configured: false, summary: '' }
  if (minOk && !maxOk) return { configured: true, summary: `> ${sizeValue.trim()} ${unitLabel}` }
  if (!minOk && maxOk) return { configured: true, summary: `< ${sizeValueMax.trim()} ${unitMaxLabel}` }
  // en dash for ranges (style guide: en dashes for ranges, never em).
  return {
    configured: true,
    summary: `${sizeValue.trim()} ${unitLabel} – ${sizeValueMax.trim()} ${unitMaxLabel}`,
  }
}

/** Returns the chip state for the Modified filter. */
export function deriveDateChip(dateFilter: DateFilter, dateValue: string, dateValueMax: string): FilterChipState {
  if (dateFilter === 'any') return { configured: false, summary: '' }
  if (dateFilter === 'after') {
    if (!dateValue) return { configured: false, summary: '' }
    return { configured: true, summary: `after ${dateValue}` }
  }
  if (dateFilter === 'before') {
    if (!dateValue) return { configured: false, summary: '' }
    return { configured: true, summary: `before ${dateValue}` }
  }
  // between
  if (!dateValue && !dateValueMax) return { configured: false, summary: '' }
  if (dateValue && !dateValueMax) return { configured: true, summary: `after ${dateValue}` }
  if (!dateValue && dateValueMax) return { configured: true, summary: `before ${dateValueMax}` }
  return { configured: true, summary: `${dateValue} – ${dateValueMax}` }
}

/**
 * Returns the chip state for the cross-mode Pattern chip. Per search-fixup-brief
 * clarification 5, this chip is ALWAYS rendered alongside Size / Modified / Search in
 * and surfaces the active filename pattern, regex pattern, or AI-produced pattern.
 * "Active" means: in filename / regex mode it's the bar contents; in AI mode it's
 * the LLM-produced pattern (the bar holds the natural-language prompt). The chip
 * is unconfigured (empty pill that the user can ignore) when no pattern is set.
 */
export function derivePatternChip(input: {
  mode: 'ai' | 'filename' | 'regex'
  query: string
  aiPattern: string | null
}): FilterChipState {
  const value = input.mode === 'ai' ? (input.aiPattern ?? '').trim() : input.query.trim()
  if (!value) return { configured: false, summary: '' }
  // Truncate so the chip doesn't blow the row's width. Long patterns stay reachable
  // via the bar (or, for AI mode, via the AI transparency strip's "Refine" path).
  const MAX_LEN = 40
  const display = value.length > MAX_LEN ? value.slice(0, MAX_LEN - 1) + '…' : value
  return { configured: true, summary: display }
}

/** Returns the chip state for the Search in (scope) filter. */
export function deriveScopeChip(scope: string, excludeSystemDirs: boolean): FilterChipState {
  const trimmed = scope.trim()
  if (!trimmed) {
    // Even with no scope text, the chip is "configured" if the user disabled the system-dirs
    // hide toggle, because that's a non-default state worth visualizing.
    if (!excludeSystemDirs) return { configured: true, summary: 'includes system folders' }
    return { configured: false, summary: '' }
  }
  // Truncate long scope text for chip display. Keep the rest reachable via the popover.
  const MAX_LEN = 40
  const display = trimmed.length > MAX_LEN ? trimmed.slice(0, MAX_LEN - 1) + '…' : trimmed
  return { configured: true, summary: display }
}
