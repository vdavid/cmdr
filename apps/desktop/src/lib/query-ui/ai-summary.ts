/**
 * Pure builder for the AI transparency strip's human-readable summary.
 *
 * The strip is a human-readable MIRROR of the structured filter state the AI produced; the live
 * filter chips are the editable source of truth (see `query-ui/CLAUDE.md`). This helper turns the
 * current chip-relevant state into the plain-language lines the strip renders: the produced
 * pattern (labelled glob/regex) and the filters the agent set (Size, Modified, Type).
 *
 * Kept side-effect free (reads plain values, not `$state`) so it's unit-testable without mounting
 * Svelte, and so the strip component stays a dumb renderer.
 */

import type { SizeFilter, SizeUnit, DateFilter, TypeFilter } from './query-filter-state.svelte'
import { deriveSizeChip, deriveDateChip } from './filter-chips/filter-chip-state'
import type { FileSizeFormat } from '$lib/settings/types'
import { tString } from '$lib/intl/messages.svelte'

/** One named filter line shown in the strip ("Size", "Modified", "Type" + its value). */
export interface AiSummaryFilter {
  label: string
  value: string
}

/** The strip's structured summary. The component renders the pattern, then the filter lines. */
export interface AiSummary {
  /** The produced glob/regex, or null when the AI set no pattern (filter-only translation). */
  pattern: string | null
  /** `'glob'` / `'regex'` / null. Drives the pattern's label ("Glob" vs "Regex" vs "Pattern"). */
  patternKind: 'glob' | 'regex' | null
  /** Size / Modified / Type lines, in display order. Only configured filters appear. */
  filters: AiSummaryFilter[]
}

/** Inputs the builder reads. Mirrors the chip-relevant slice of `QueryFilterState`. */
export interface AiSummaryInput {
  pattern: string | null
  patternKind: 'glob' | 'regex' | null
  sizeFilter: SizeFilter
  sizeValue: string
  sizeUnit: SizeUnit
  sizeValueMax: string
  sizeUnitMax: SizeUnit
  dateFilter: DateFilter
  dateValue: string
  dateValueMax: string
  typeFilter: TypeFilter
  fileSizeFormat?: FileSizeFormat
}

/** Plain-language label for the type toggle. `both` is the default, so it's omitted from the strip. */
function typeFilterSummary(typeFilter: TypeFilter): string | null {
  if (typeFilter === 'file') return tString('queryUi.ai.typeSummary.filesOnly')
  if (typeFilter === 'folder') return tString('queryUi.ai.typeSummary.foldersOnly')
  return null
}

/**
 * Builds the strip's summary from the current chip state. The pattern is shown verbatim (the chip
 * truncates for width; the strip shows the full pattern so the user can read exactly what ran).
 */
export function buildAiSummary(input: AiSummaryInput): AiSummary {
  const filters: AiSummaryFilter[] = []

  const size = deriveSizeChip(
    input.sizeFilter,
    input.sizeValue,
    input.sizeUnit,
    input.sizeValueMax,
    input.sizeUnitMax,
    input.fileSizeFormat ?? 'binary',
  )
  if (size.configured) filters.push({ label: tString('queryUi.ai.filter.size'), value: size.summary })

  const date = deriveDateChip(input.dateFilter, input.dateValue, input.dateValueMax)
  if (date.configured) filters.push({ label: tString('queryUi.ai.filter.modified'), value: date.summary })

  const typeSummary = typeFilterSummary(input.typeFilter)
  if (typeSummary) filters.push({ label: tString('queryUi.ai.filter.type'), value: typeSummary })

  const pattern = input.pattern?.trim() ? input.pattern.trim() : null

  return {
    pattern,
    patternKind: pattern ? input.patternKind : null,
    filters,
  }
}

/** The label for the pattern row: names the flavor when known, falls back to "Pattern". */
export function patternRowLabel(patternKind: 'glob' | 'regex' | null): string {
  if (patternKind === 'regex') return tString('queryUi.ai.patternLabel.regex')
  if (patternKind === 'glob') return tString('queryUi.ai.patternLabel.glob')
  return tString('queryUi.ai.patternLabel.generic')
}
