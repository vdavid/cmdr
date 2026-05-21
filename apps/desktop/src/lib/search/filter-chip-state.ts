/**
 * Pure helpers for deriving the display state of the filter chips
 * (Size, Modified, Search in) from the current search state.
 *
 * Kept side-effect free so unit tests can pin the chip-summary rules cheaply without mounting
 * the Svelte component. See `SearchFilterChips.svelte` for the markup consumer.
 */

import type { SizeFilter, SizeUnit, DateFilter } from './search-state.svelte'

/** Display state of a single filter chip. */
export interface FilterChipState {
  configured: boolean
  /** Summary shown when configured. Empty string when not configured. */
  summary: string
}

/** Returns the chip state for the Size filter. */
export function deriveSizeChip(
  sizeFilter: SizeFilter,
  sizeValue: string,
  sizeUnit: SizeUnit,
  sizeValueMax: string,
  sizeUnitMax: SizeUnit,
): FilterChipState {
  if (sizeFilter === 'any') return { configured: false, summary: '' }

  // A configured filter requires at least the first value (or both, for "between"). The chip
  // stays unconfigured if the user changed the comparator to "gte" but hasn't typed a number yet.
  const minNumeric = parseFloat(sizeValue)
  const minOk = !isNaN(minNumeric) && minNumeric > 0
  if (sizeFilter === 'gte') {
    if (!minOk) return { configured: false, summary: '' }
    return { configured: true, summary: `> ${sizeValue.trim()} ${sizeUnit}` }
  }
  if (sizeFilter === 'lte') {
    if (!minOk) return { configured: false, summary: '' }
    return { configured: true, summary: `< ${sizeValue.trim()} ${sizeUnit}` }
  }
  // between
  const maxNumeric = parseFloat(sizeValueMax)
  const maxOk = !isNaN(maxNumeric) && maxNumeric > 0
  if (!minOk && !maxOk) return { configured: false, summary: '' }
  if (minOk && !maxOk) return { configured: true, summary: `> ${sizeValue.trim()} ${sizeUnit}` }
  if (!minOk && maxOk) return { configured: true, summary: `< ${sizeValueMax.trim()} ${sizeUnitMax}` }
  // en dash for ranges (style guide: en dashes for ranges, never em).
  return {
    configured: true,
    summary: `${sizeValue.trim()} ${sizeUnit} – ${sizeValueMax.trim()} ${sizeUnitMax}`,
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
