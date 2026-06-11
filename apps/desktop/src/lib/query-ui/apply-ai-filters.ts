/**
 * Shared filter-write helpers used by both query dialogs (Search + Selection) to
 * paint an AI translation's structured size/date result onto a `QueryFilterState`.
 *
 * Both consumers had their own near-identical copies; this is the one source of
 * truth. Each helper writes the chip comparator + value(s)/unit(s) and returns
 * whether it touched anything, so the wrapper can flag the changed chips for the
 * highlight flash. Callers reset the chips to `any` first when they want a clean
 * slate per AI run (the leak-prevention contract lives in the wrapper, not here).
 */

import { bytesToSize, type QueryFilterState } from './query-filter-state.svelte'

/**
 * Applies the AI's size bounds (bytes, either end optional) onto the size chip.
 * Returns true when at least one bound was set.
 */
export function applySizeFromAi(state: QueryFilterState, min: number | null, max: number | null): boolean {
  if (min == null && max == null) return false
  if (min != null && max != null) {
    state.setSizeFilter('between')
    const lo = bytesToSize(min)
    const hi = bytesToSize(max)
    state.setSizeValue(lo.value)
    state.setSizeUnit(lo.unit)
    state.setSizeValueMax(hi.value)
    state.setSizeUnitMax(hi.unit)
  } else if (min != null) {
    state.setSizeFilter('gte')
    const lo = bytesToSize(min)
    state.setSizeValue(lo.value)
    state.setSizeUnit(lo.unit)
  } else if (max != null) {
    state.setSizeFilter('lte')
    const hi = bytesToSize(max)
    state.setSizeValue(hi.value)
    state.setSizeUnit(hi.unit)
  }
  return true
}

/**
 * Applies the AI's date bounds (ISO strings, either end optional) onto the
 * Modified chip. Returns true when at least one bound was set.
 */
export function applyDateFromAi(state: QueryFilterState, after: string | null, before: string | null): boolean {
  if (after == null && before == null) return false
  if (after != null && before != null) {
    state.setDateFilter('between')
    state.setDateValue(after)
    state.setDateValueMax(before)
  } else if (after != null) {
    state.setDateFilter('after')
    state.setDateValue(after)
  } else if (before != null) {
    state.setDateFilter('before')
    state.setDateValue(before)
  }
  return true
}
