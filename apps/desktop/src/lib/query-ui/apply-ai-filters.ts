/**
 * Shared filter-write helpers used by both query dialogs (Search + Selection) to
 * paint an AI translation's structured size/date/type result onto a `QueryFilterState`.
 *
 * Both consumers had their own near-identical copies; this is the one source of
 * truth. Each helper writes the chip comparator + value(s)/unit(s) and returns
 * whether it touched anything, so the wrapper can flag the changed chips for the
 * highlight flash. Callers reset the SIZE and DATE chips to `any` first when they
 * want a clean slate per AI run (the leak-prevention contract lives in the wrapper,
 * not here).
 *
 * Type is the deliberate exception. `applyTypeFromAi` is leave-alone-if-null: the AI
 * RECEIVES the current `file | folder | both` setting as context and either sets a
 * new value or stays silent. When it stays silent (`isDirectory == null`) we keep the
 * user's current type rather than resetting to `both` (David-decided). So callers must
 * NOT reset `typeFilter` before calling this, the way they reset size/date. Don't
 * "consistency-fix" this into a pre-reset: that would discard a type the user picked
 * by hand on every AI run.
 */

import { bytesToSize, type QueryFilterState, type TypeFilter } from './query-filter-state.svelte'

/**
 * Applies the AI's size bounds (bytes, either end optional) onto the size chip.
 * Returns true when at least one bound was set.
 */
export function applySizeFromAi(state: QueryFilterState, min: number | null, max: number | null): boolean {
  if (min == null && max == null) return false
  if (min != null && max != null && min === max) {
    // Exact size (for example "size = 0" → find empty files): set `eq` so the chip reads
    // "= N" instead of "between N and N". `eq` is a UI label only (see `SizeFilter`).
    state.setSizeFilter('eq')
    const exact = bytesToSize(min)
    state.setSizeValue(exact.value)
    state.setSizeUnit(exact.unit)
  } else if (min != null && max != null) {
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

/**
 * Applies the AI's type decision (`isDirectory` from the wire: `true` → folders, `false` →
 * files, `null` → no opinion) onto the Type toggle. Returns true only when it actually wrote.
 *
 * Leave-alone-if-null is the whole point (see the module header): when the AI returns `null`,
 * the user's current `Both | Files | Folders` choice stands. Unlike `applySizeFromAi` /
 * `applyDateFromAi`, callers DON'T reset the type to `both` first.
 */
export function applyTypeFromAi(state: QueryFilterState, isDirectory: boolean | null): boolean {
  if (isDirectory == null) return false
  const next: TypeFilter = isDirectory ? 'folder' : 'file'
  state.setTypeFilter(next)
  return true
}
